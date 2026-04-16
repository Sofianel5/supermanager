use std::{
    fmt,
    io::{self, IsTerminal},
    path::Path,
};

use anyhow::{Result, anyhow, bail};
use inquire::{InquireError, Select, Text, validator::Validation};
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OrganizationMenuChoice {
    Existing(usize),
    CreateNew,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OrganizationMenuOption {
    choice: OrganizationMenuChoice,
    label: String,
}

impl fmt::Display for OrganizationMenuOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label)
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
    set_active_organization(http, server_url, &auth_state.access_token, &slug)?;
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
    let name = Text::new("Organization name")
        .with_placeholder("Acme Labs")
        .with_help_message("Slug is generated automatically. Esc cancels.")
        .with_validator(validate_organization_name)
        .prompt()
        .map_err(|error| prompt_error(error, "organization creation cancelled"))?;
    let name = name.trim();
    let slug = slugify_organization_name(name);

    Ok((name.to_owned(), slug))
}

fn prompt_for_organization_choice(
    organizations: &[ViewerOrganization],
    active_org_slug: Option<&str>,
) -> Result<OrganizationMenuChoice> {
    let options = organization_menu_options(organizations, active_org_slug);
    let selection = Select::new("Choose active organization", options)
        .with_help_message("Use Up/Down to move, Enter to select, Esc to cancel")
        .with_page_size((organizations.len() + 1).min(8))
        .with_starting_cursor(initial_organization_menu_selection(
            organizations,
            active_org_slug,
        ))
        .without_filtering()
        .prompt()
        .map_err(|error| prompt_error(error, "organization configuration cancelled"))?;

    Ok(selection.choice)
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

fn organization_menu_options(
    organizations: &[ViewerOrganization],
    active_org_slug: Option<&str>,
) -> Vec<OrganizationMenuOption> {
    let mut options = organizations
        .iter()
        .enumerate()
        .map(|(index, organization)| {
            let active_suffix = if Some(organization.organization_slug.as_str()) == active_org_slug
            {
                " [active]"
            } else {
                ""
            };

            OrganizationMenuOption {
                choice: OrganizationMenuChoice::Existing(index),
                label: format!(
                    "{} ({}){active_suffix}",
                    organization.organization_slug, organization.organization_name
                ),
            }
        })
        .collect::<Vec<_>>();

    options.push(OrganizationMenuOption {
        choice: OrganizationMenuChoice::CreateNew,
        label: "Create organization".to_owned(),
    });
    options
}

fn validate_organization_name(
    value: &str,
) -> std::result::Result<Validation, inquire::CustomUserError> {
    let name = value.trim();
    if name.is_empty() {
        return Ok(Validation::Invalid("Organization name is required.".into()));
    }

    if slugify_organization_name(name).is_empty() {
        return Ok(Validation::Invalid(
            "Use letters or numbers in the organization name.".into(),
        ));
    }

    Ok(Validation::Valid)
}

fn prompt_error(error: InquireError, cancelled_message: &'static str) -> anyhow::Error {
    match error {
        InquireError::OperationCanceled | InquireError::OperationInterrupted => {
            anyhow!(cancelled_message)
        }
        other => anyhow!("interactive prompt failed: {other}"),
    }
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

    #[test]
    fn organization_menu_options_marks_active_org_and_appends_create() {
        let organizations = vec![test_org("acme", "Acme"), test_org("beta", "Beta")];

        let options = organization_menu_options(&organizations, Some("beta"));

        assert_eq!(options.len(), 3);
        assert_eq!(options[0].label, "acme (Acme)");
        assert_eq!(options[1].label, "beta (Beta) [active]");
        assert_eq!(options[2].choice, OrganizationMenuChoice::CreateNew);
        assert_eq!(options[2].label, "Create organization");
    }

    #[test]
    fn organization_name_validator_rejects_empty_or_invalid_names() {
        assert_eq!(
            validate_organization_name("   ").unwrap(),
            Validation::Invalid("Organization name is required.".into())
        );
        assert_eq!(
            validate_organization_name("***").unwrap(),
            Validation::Invalid("Use letters or numbers in the organization name.".into())
        );
        assert_eq!(
            validate_organization_name("Acme Labs").unwrap(),
            Validation::Valid
        );
    }
}
