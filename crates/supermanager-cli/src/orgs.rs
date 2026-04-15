use std::{
    io::{self, IsTerminal, Write},
    path::Path,
};

use anyhow::{Context, Result, anyhow, bail};
use crossterm::{
    ExecutableCommand,
    cursor::{self, Hide, Show},
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    terminal::{self, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use reqwest::blocking::Client;

use crate::{
    auth::{
        auth_state_path, create_organization, get_viewer, require_auth_state,
        set_active_organization, viewer_active_org_slug, write_auth_state,
    },
    support::{API_TIMEOUT_SECONDS, build_http_client, normalize_url},
    types::{
        AuthState, ConfigureOrganizationsConfig, ConfigureOrganizationsOutcome,
        CreateOrganizationConfig, CreateOrganizationOutcome, ListOrganizationEntry,
        ListOrganizationsConfig, ListOrganizationsOutcome, ViewerOrganization,
    },
};

enum OrganizationMenuChoice {
    Existing(usize),
    CreateNew,
}

struct TerminalScreen {
    stdout: io::Stdout,
}

impl TerminalScreen {
    fn enter() -> Result<Self> {
        let mut stdout = io::stdout();
        terminal::enable_raw_mode().context("failed to enable raw terminal mode")?;
        let setup_result = stdout
            .execute(EnterAlternateScreen)
            .and_then(|stdout| stdout.execute(Hide))
            .map(|_| ())
            .context("failed to initialize terminal UI");

        if let Err(error) = setup_result {
            let _ = terminal::disable_raw_mode();
            return Err(error);
        }

        Ok(Self { stdout })
    }

    fn stdout(&mut self) -> &mut io::Stdout {
        &mut self.stdout
    }
}

impl Drop for TerminalScreen {
    fn drop(&mut self) {
        let _ = self.stdout.execute(Show);
        let _ = self.stdout.execute(LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}

pub fn list_organizations(config: ListOrganizationsConfig) -> Result<ListOrganizationsOutcome> {
    let server_url = normalize_url(&config.server_url);
    let http = build_http_client(API_TIMEOUT_SECONDS)?;
    let auth_state = require_auth_state(&config.home_dir, &server_url)?;
    let viewer = get_viewer(&http, &server_url, &auth_state.access_token)?;

    Ok(ListOrganizationsOutcome {
        active_org_slug: viewer_active_org_slug(&viewer).map(ToOwned::to_owned),
        organizations: viewer
            .organizations
            .into_iter()
            .map(|organization| ListOrganizationEntry {
                organization_name: organization.organization_name,
                organization_slug: organization.organization_slug,
            })
            .collect(),
    })
}

pub fn create_organization_interactive(
    config: CreateOrganizationConfig,
) -> Result<CreateOrganizationOutcome> {
    ensure_interactive_terminal("supermanager orgs create")?;

    let server_url = normalize_url(&config.server_url);
    let http = build_http_client(API_TIMEOUT_SECONDS)?;
    let mut auth_state = require_auth_state(&config.home_dir, &server_url)?;
    let (name, slug) =
        create_and_activate_organization(&http, &server_url, &mut auth_state, &config.home_dir)?;

    Ok(CreateOrganizationOutcome {
        organization_name: name,
        organization_slug: slug,
    })
}

pub fn configure_organizations_interactive(
    config: ConfigureOrganizationsConfig,
) -> Result<ConfigureOrganizationsOutcome> {
    ensure_interactive_terminal("supermanager orgs configure")?;

    let server_url = normalize_url(&config.server_url);
    let http = build_http_client(API_TIMEOUT_SECONDS)?;
    let mut auth_state = require_auth_state(&config.home_dir, &server_url)?;
    let viewer = get_viewer(&http, &server_url, &auth_state.access_token)?;
    let active_org_slug = viewer_active_org_slug(&viewer);

    match prompt_for_organization_choice(&viewer.organizations, active_org_slug)? {
        OrganizationMenuChoice::Existing(index) => {
            let organization = viewer
                .organizations
                .get(index)
                .ok_or_else(|| anyhow!("organization selection is out of bounds"))?;

            set_active_organization(
                &http,
                &server_url,
                &auth_state.access_token,
                &organization.organization_slug,
            )?;
            auth_state.active_org_slug = Some(organization.organization_slug.clone());
            write_auth_state(&auth_state_path(&config.home_dir), &auth_state)?;

            Ok(ConfigureOrganizationsOutcome {
                created_new: false,
                organization_name: organization.organization_name.clone(),
                organization_slug: organization.organization_slug.clone(),
            })
        }
        OrganizationMenuChoice::CreateNew => {
            let (name, slug) = create_and_activate_organization(
                &http,
                &server_url,
                &mut auth_state,
                &config.home_dir,
            )?;

            Ok(ConfigureOrganizationsOutcome {
                created_new: true,
                organization_name: name,
                organization_slug: slug,
            })
        }
    }
}

fn create_and_activate_organization(
    http: &Client,
    server_url: &str,
    auth_state: &mut AuthState,
    home_dir: &Path,
) -> Result<(String, String)> {
    let (name, slug) = prompt_for_organization_name()?;

    create_organization(http, server_url, &auth_state.access_token, &name, &slug)?;
    auth_state.active_org_slug = Some(slug.clone());
    write_auth_state(&auth_state_path(home_dir), auth_state)?;

    Ok((name, slug))
}

fn ensure_interactive_terminal(command: &str) -> Result<()> {
    if io::stdin().is_terminal() && io::stdout().is_terminal() {
        return Ok(());
    }

    bail!("{command} requires an interactive terminal");
}

fn prompt_for_organization_name() -> Result<(String, String)> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    println!();
    println!("  Create organization");
    println!();

    loop {
        print!("    Name       ");
        stdout.flush().context("failed to flush stdout")?;

        let mut line = String::new();
        let bytes = stdin
            .read_line(&mut line)
            .context("failed to read organization name")?;
        if bytes == 0 {
            bail!("organization creation cancelled");
        }

        let name = line.trim();
        if name.is_empty() {
            println!("    Error      organization name is required");
            println!();
            continue;
        }

        let slug = slugify_organization_name(name);
        if slug.is_empty() {
            println!("    Error      use letters or numbers in the organization name");
            println!();
            continue;
        }

        println!("    Slug       {slug}");
        println!();
        return Ok((name.to_owned(), slug));
    }
}

fn prompt_for_organization_choice(
    organizations: &[ViewerOrganization],
    active_org_slug: Option<&str>,
) -> Result<OrganizationMenuChoice> {
    let total_items = organizations.len() + 1;
    let mut selection = initial_organization_menu_selection(organizations, active_org_slug);
    let mut screen = TerminalScreen::enter()?;

    loop {
        render_organization_menu(screen.stdout(), organizations, selection, active_org_slug)?;

        let Event::Key(key) = event::read().context("failed to read terminal input")? else {
            continue;
        };

        if key.kind != KeyEventKind::Press {
            continue;
        }

        match key.code {
            KeyCode::Up => {
                selection = move_menu_selection(selection, total_items, -1);
            }
            KeyCode::Down => {
                selection = move_menu_selection(selection, total_items, 1);
            }
            KeyCode::Enter => {
                if selection == organizations.len() {
                    return Ok(OrganizationMenuChoice::CreateNew);
                }
                return Ok(OrganizationMenuChoice::Existing(selection));
            }
            KeyCode::Esc => bail!("organization configuration cancelled"),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                bail!("organization configuration cancelled");
            }
            _ => {}
        }
    }
}

fn render_organization_menu(
    stdout: &mut io::Stdout,
    organizations: &[ViewerOrganization],
    selection: usize,
    active_org_slug: Option<&str>,
) -> Result<()> {
    stdout
        .execute(cursor::MoveTo(0, 0))
        .context("failed to position terminal cursor")?;
    stdout
        .execute(terminal::Clear(ClearType::All))
        .context("failed to clear terminal")?;

    writeln!(stdout, "  Configure active organization")?;
    writeln!(stdout)?;
    writeln!(stdout, "  Use Up/Down arrows and Enter. Esc cancels.")?;
    writeln!(stdout)?;

    for (index, organization) in organizations.iter().enumerate() {
        let marker = if index == selection { ">" } else { " " };
        let active = if Some(organization.organization_slug.as_str()) == active_org_slug {
            " [active]"
        } else {
            ""
        };
        writeln!(
            stdout,
            "  {marker} {} ({}){active}",
            organization.organization_slug, organization.organization_name
        )?;
    }

    let marker = if selection == organizations.len() {
        ">"
    } else {
        " "
    };
    writeln!(stdout)?;
    writeln!(stdout, "  {marker} Create organization")?;
    stdout.flush().context("failed to flush terminal")
}

fn initial_organization_menu_selection(
    organizations: &[ViewerOrganization],
    active_org_slug: Option<&str>,
) -> usize {
    active_org_slug
        .and_then(|organization_slug| {
            organizations
                .iter()
                .position(|organization| organization.organization_slug == organization_slug)
        })
        .unwrap_or(0)
}

fn move_menu_selection(current: usize, item_count: usize, delta: isize) -> usize {
    if item_count == 0 {
        return 0;
    }

    let item_count = item_count as isize;
    let next = (current as isize + delta).rem_euclid(item_count);
    next as usize
}

fn slugify_organization_name(value: &str) -> String {
    let mut slug = String::new();
    let mut previous_was_separator = false;

    for ch in value.trim().chars().flat_map(char::to_lowercase) {
        if slug.len() >= 64 {
            break;
        }

        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            previous_was_separator = false;
        } else if !slug.is_empty() && !previous_was_separator {
            slug.push('-');
            previous_was_separator = true;
        }
    }

    while slug.ends_with('-') {
        slug.pop();
    }

    slug
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_org(slug: &str, name: &str) -> ViewerOrganization {
        ViewerOrganization {
            organization_id: slug.to_owned(),
            organization_name: name.to_owned(),
            organization_slug: slug.to_owned(),
        }
    }

    #[test]
    fn slugify_organization_name_matches_web_rules() {
        assert_eq!(slugify_organization_name("Acme Labs"), "acme-labs");
        assert_eq!(slugify_organization_name("  ACME___Labs  "), "acme-labs");
        assert_eq!(slugify_organization_name("***"), "");
        assert_eq!(
            slugify_organization_name("a".repeat(80).as_str()),
            "a".repeat(64)
        );
    }

    #[test]
    fn move_menu_selection_wraps_in_both_directions() {
        assert_eq!(move_menu_selection(0, 3, -1), 2);
        assert_eq!(move_menu_selection(2, 3, 1), 0);
        assert_eq!(move_menu_selection(1, 3, 1), 2);
    }

    #[test]
    fn initial_organization_menu_selection_prefers_active_slug() {
        let organizations = vec![test_org("acme", "Acme"), test_org("beta", "Beta")];

        assert_eq!(
            initial_organization_menu_selection(&organizations, Some("beta")),
            1
        );
        assert_eq!(initial_organization_menu_selection(&organizations, None), 0);
        assert_eq!(
            initial_organization_menu_selection(&organizations, Some("missing")),
            0
        );
    }
}
