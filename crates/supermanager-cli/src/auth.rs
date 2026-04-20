use std::{
    env, fs, io,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use indicatif::ProgressBar;
use reporter_protocol::ProjectMetadataResponse;
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::json;

use crate::{
    orgs::configure_organizations_interactive_with_state,
    support::{
        API_TIMEOUT_SECONDS, DEVICE_CLIENT_ID, DEVICE_GRANT_TYPE, DEVICE_SCOPE, HOME_AUTH_STATE,
        build_http_client, ensure_interactive_terminal, ensure_success, is_interactive_terminal,
        normalize_url, open_url, write_private_text,
    },
    types::{AuthState, LoginConfig, LoginOutcome, ViewerOrganization, ViewerResponse},
};

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri_complete: String,
    interval: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct DeviceTokenSuccess {
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct DeviceTokenError {
    error: String,
    error_description: Option<String>,
}

pub(crate) struct AuthedClient<'a> {
    http: &'a Client,
    token: &'a str,
}

impl<'a> AuthedClient<'a> {
    pub(crate) fn get(&self, url: String) -> reqwest::blocking::RequestBuilder {
        self.http.get(url).bearer_auth(self.token)
    }

    pub(crate) fn post(&self, url: String) -> reqwest::blocking::RequestBuilder {
        self.http.post(url).bearer_auth(self.token)
    }
}

pub(crate) fn authed<'a>(http: &'a Client, token: &'a str) -> AuthedClient<'a> {
    AuthedClient { http, token }
}

pub fn resolve_home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("HOME is not set"))
}

pub fn login(config: LoginConfig) -> Result<LoginOutcome> {
    ensure_interactive_terminal("supermanager login")?;

    let server_url = normalize_url(&config.server_url);
    let http = build_http_client(API_TIMEOUT_SECONDS)?;
    let device = request_device_code(&http, &server_url)?;
    let verification_url = device.verification_uri_complete.clone();
    let polling_interval = device.interval.unwrap_or(5).max(1);
    let browser_opened = open_url(&verification_url).is_ok();

    print_device_login_instructions(&verification_url, &device.user_code, browser_opened);
    let spinner = ProgressBar::new_spinner();
    spinner.enable_steady_tick(Duration::from_millis(120));
    spinner.set_message("Waiting for approval in your browser...");

    let access_token =
        match poll_for_access_token(&http, &server_url, &device.device_code, polling_interval) {
            Ok(access_token) => {
                spinner.finish_and_clear();
                access_token
            }
            Err(error) => {
                spinner.finish_and_clear();
                return Err(error);
            }
        };
    let viewer = get_viewer(&http, &server_url, &access_token)?;
    let mut auth_state = AuthState {
        access_token,
        active_org_slug: None,
        server_url: server_url.clone(),
    };
    let active_org = resolve_active_org_interactive(
        &http,
        &server_url,
        &mut auth_state,
        &config.home_dir,
        &viewer,
        None,
        &format!("supermanager login --server {server_url}"),
    )?;

    Ok(LoginOutcome {
        server_url,
        active_org_slug: active_org.organization_slug,
    })
}

pub fn logout(home_dir: &Path) -> Result<bool> {
    let path = auth_state_path(home_dir);
    match fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => {
            Err(anyhow::Error::new(error).context(format!("failed to remove {}", path.display())))
        }
    }
}

pub(crate) fn auth_state_path(home_dir: &Path) -> PathBuf {
    home_dir.join(HOME_AUTH_STATE)
}

pub(crate) fn require_auth_state(home_dir: &Path, server_url: &str) -> Result<AuthState> {
    let normalized_server_url = normalize_url(server_url);
    let state = read_auth_state(&auth_state_path(home_dir))?.ok_or_else(|| {
        anyhow!(
            "not logged in to {normalized_server_url}; run `supermanager login --server {normalized_server_url}` first"
        )
    })?;

    if normalize_url(&state.server_url) != normalized_server_url {
        bail!(
            "logged in to {}, not {}; run `supermanager login --server {normalized_server_url}` first",
            state.server_url,
            normalized_server_url
        );
    }

    Ok(state)
}

pub(crate) fn read_auth_state(path: &Path) -> Result<Option<AuthState>> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(
                anyhow::Error::new(error).context(format!("failed to read {}", path.display()))
            );
        }
    };
    if text.trim().is_empty() {
        return Ok(None);
    }

    let state = serde_json::from_str(&text)
        .with_context(|| format!("failed to parse JSON in {}", path.display()))?;
    Ok(Some(state))
}

pub(crate) fn write_auth_state(path: &Path, state: &AuthState) -> Result<()> {
    let text = serde_json::to_string_pretty(state)
        .with_context(|| format!("failed to serialize {}", path.display()))?;
    write_private_text(path, &(text + "\n"))
}

fn print_device_login_instructions(verification_url: &str, user_code: &str, browser_opened: bool) {
    println!();
    println!("  Approve this CLI login");
    println!();
    println!("    URL       {verification_url}");
    println!("    Code      {user_code}");
    if browser_opened {
        println!("    Browser   opened automatically");
    } else {
        println!("    Browser   open the URL manually");
    }
    println!();
}

fn request_device_code(http: &Client, server_url: &str) -> Result<DeviceCodeResponse> {
    let response = http
        .post(format!("{server_url}/api/auth/device/code"))
        .json(&json!({
            "client_id": DEVICE_CLIENT_ID,
            "scope": DEVICE_SCOPE,
        }))
        .send()
        .context("failed to start device login")?;
    let response = ensure_success(response, "start device login")?;

    response
        .json()
        .context("failed to parse device-login response JSON")
}

fn poll_for_access_token(
    http: &Client,
    server_url: &str,
    device_code: &str,
    interval_seconds: u64,
) -> Result<String> {
    let mut poll_interval = interval_seconds.max(1);

    loop {
        let response = http
            .post(format!("{server_url}/api/auth/device/token"))
            .json(&json!({
                "grant_type": DEVICE_GRANT_TYPE,
                "device_code": device_code,
                "client_id": DEVICE_CLIENT_ID,
            }))
            .send()
            .context("failed to poll device login")?;

        if response.status().is_success() {
            let payload: DeviceTokenSuccess = response
                .json()
                .context("failed to parse device-token response JSON")?;
            return Ok(payload.access_token);
        }

        let status = response.status();
        let body = response.text().unwrap_or_default();
        let parsed = serde_json::from_str::<DeviceTokenError>(&body).ok();

        match parsed.as_ref().map(|error| error.error.as_str()) {
            Some("authorization_pending") => {
                thread::sleep(Duration::from_secs(poll_interval));
                continue;
            }
            Some("slow_down") => {
                poll_interval += 5;
                thread::sleep(Duration::from_secs(poll_interval));
                continue;
            }
            Some("access_denied") => {
                bail!("device login was denied");
            }
            Some("expired_token") => {
                bail!("device login expired before it was approved");
            }
            _ => {
                if let Some(error) = parsed {
                    let detail = error
                        .error_description
                        .unwrap_or_else(|| "device login failed".to_owned());
                    bail!("device login returned {status}: {detail}");
                }
                bail!("device login returned {status}: {body}");
            }
        }
    }
}

pub(crate) fn get_viewer(
    http: &Client,
    server_url: &str,
    access_token: &str,
) -> Result<ViewerResponse> {
    let response = authed(http, access_token)
        .get(format!("{server_url}/v1/me"))
        .send()
        .context("failed to fetch current account")?;
    let response = ensure_success(response, "fetch current account")?;

    response
        .json()
        .context("failed to parse current-account response JSON")
}

pub(crate) fn fetch_project(
    http: &Client,
    server_url: &str,
    access_token: &str,
    project_id: &str,
) -> Result<ProjectMetadataResponse> {
    let response = authed(http, access_token)
        .get(format!("{server_url}/v1/projects/{project_id}"))
        .send()
        .with_context(|| format!("failed to fetch project {project_id}"))?;
    let response = ensure_success(response, "get project")?;

    response
        .json()
        .context("failed to parse project response JSON")
}

pub(crate) fn create_organization(
    http: &Client,
    server_url: &str,
    access_token: &str,
    name: &str,
    slug: &str,
) -> Result<()> {
    let response = authed(http, access_token)
        .post(format!("{server_url}/api/auth/organization/create"))
        .json(&json!({
            "name": name,
            "slug": slug,
            "keepCurrentActiveOrganization": false,
        }))
        .send()
        .context("failed to create organization")?;
    ensure_success(response, "create organization")?;
    Ok(())
}

pub(crate) fn set_active_organization(
    http: &Client,
    server_url: &str,
    access_token: &str,
    organization_slug: &str,
) -> Result<()> {
    let response = authed(http, access_token)
        .post(format!("{server_url}/api/auth/organization/set-active"))
        .json(&json!({
            "organizationSlug": organization_slug,
        }))
        .send()
        .context("failed to set active organization")?;
    ensure_success(response, "set active organization")?;
    Ok(())
}

/// Try to find a single preferred org from the viewer's memberships.
/// Resolution order: explicit slug > active org id > single-org auto-select.
fn find_preferred_org<'a>(
    viewer: &'a ViewerResponse,
    requested_slug: Option<&str>,
) -> Result<Option<&'a ViewerOrganization>> {
    let slug = requested_slug
        .map(str::trim)
        .filter(|slug| !slug.is_empty());

    if let Some(slug) = slug {
        let organization = find_org_by_slug(viewer, slug).ok_or_else(|| {
            anyhow!(
                "organization {slug} is not available to this account. Available organizations: {}",
                format_org_choices(viewer)
            )
        })?;
        return Ok(Some(organization));
    }

    if let Some(active) = viewer
        .active_organization_id
        .as_deref()
        .and_then(|organization_id| find_org_by_id(viewer, organization_id))
    {
        return Ok(Some(active));
    }

    if viewer.organizations.len() == 1 {
        return Ok(Some(&viewer.organizations[0]));
    }

    Ok(None)
}

pub(crate) fn select_active_org(
    viewer: &ViewerResponse,
    auth_state: &mut AuthState,
    requested_slug: Option<&str>,
) -> Result<ViewerOrganization> {
    let organization = find_preferred_org(viewer, requested_slug)?.ok_or_else(|| {
        anyhow!(
            "multiple organizations are available; rerun with `--org <slug>`. Available organizations: {}",
            format_org_choices(viewer)
        )
    })?.clone();

    auth_state.active_org_slug = Some(organization.organization_slug.clone());
    Ok(organization)
}

pub(crate) fn resolve_active_org_interactive(
    http: &Client,
    server_url: &str,
    auth_state: &mut AuthState,
    home_dir: &Path,
    viewer: &ViewerResponse,
    requested_slug: Option<&str>,
    rerun_command: &str,
) -> Result<ViewerOrganization> {
    if viewer.organizations.is_empty() {
        if !is_interactive_terminal() {
            bail!(
                "no organizations are available to this account; run `supermanager orgs configure` to create one or ask your manager for an invite link"
            );
        }

        return match configure_organizations_interactive_with_state(
            http, server_url, auth_state, home_dir, viewer,
        )? {
            crate::types::ConfigureOrganizationsOutcome::Selected {
                organization_slug, ..
            } => refreshed_selected_org(
                http,
                server_url,
                &auth_state.access_token,
                &organization_slug,
            ),
            crate::types::ConfigureOrganizationsOutcome::InviteRequested => {
                bail!(
                    "no organization selected. Ask your manager for an email-bound invite link, then use that email address to accept it and run `{rerun_command}` again"
                );
            }
        };
    }

    match select_active_org(viewer, auth_state, requested_slug) {
        Ok(organization) => {
            if viewer_active_org_slug(viewer) != Some(organization.organization_slug.as_str()) {
                set_active_organization(
                    http,
                    server_url,
                    &auth_state.access_token,
                    &organization.organization_slug,
                )?;
            }
            write_auth_state(&auth_state_path(home_dir), auth_state)?;
            Ok(organization)
        }
        Err(error) if requested_slug.is_some() => Err(error),
        Err(_) if !is_interactive_terminal() => {
            bail!(
                "multiple organizations are available; rerun with `--org <slug>` or set the active organization with `supermanager orgs configure`. Available organizations: {}",
                format_org_choices(viewer)
            );
        }
        Err(_) => match configure_organizations_interactive_with_state(
            http, server_url, auth_state, home_dir, viewer,
        )? {
            crate::types::ConfigureOrganizationsOutcome::Selected {
                organization_slug, ..
            } => refreshed_selected_org(
                http,
                server_url,
                &auth_state.access_token,
                &organization_slug,
            ),
            crate::types::ConfigureOrganizationsOutcome::InviteRequested => {
                bail!(
                    "no organization selected. Ask your manager for an email-bound invite link, then use that email address to accept it and run `{rerun_command}` again"
                );
            }
        },
    }
}

pub(crate) fn viewer_active_org_slug(viewer: &ViewerResponse) -> Option<&str> {
    viewer
        .active_organization_id
        .as_deref()
        .and_then(|organization_id| find_org_by_id(viewer, organization_id))
        .map(|organization| organization.organization_slug.as_str())
}

fn find_org_by_id<'a>(
    viewer: &'a ViewerResponse,
    organization_id: &str,
) -> Option<&'a ViewerOrganization> {
    viewer
        .organizations
        .iter()
        .find(|organization| organization.organization_id == organization_id)
}

fn find_org_by_slug<'a>(
    viewer: &'a ViewerResponse,
    organization_slug: &str,
) -> Option<&'a ViewerOrganization> {
    viewer
        .organizations
        .iter()
        .find(|organization| organization.organization_slug == organization_slug)
}

fn format_org_choices(viewer: &ViewerResponse) -> String {
    let mut organizations = viewer
        .organizations
        .iter()
        .map(|organization| {
            format!(
                "{} ({})",
                organization.organization_slug, organization.organization_name
            )
        })
        .collect::<Vec<_>>();
    organizations.sort();
    organizations.join(", ")
}

fn refreshed_selected_org(
    http: &Client,
    server_url: &str,
    access_token: &str,
    organization_slug: &str,
) -> Result<ViewerOrganization> {
    let viewer = get_viewer(http, server_url, access_token)?;
    find_org_by_slug(&viewer, organization_slug)
        .cloned()
        .ok_or_else(|| anyhow!("organization {organization_slug} is not available after setup"))
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    #[test]
    fn write_auth_state_round_trips() {
        let root = test_dir("auth-state");
        let home_dir = root.join("home");
        fs::create_dir_all(&home_dir).unwrap();
        let path = auth_state_path(&home_dir);
        let expected = AuthState {
            access_token: "token-123".to_owned(),
            active_org_slug: Some("acme".to_owned()),
            server_url: "https://api.supermanager.dev".to_owned(),
        };

        write_auth_state(&path, &expected).unwrap();

        assert_eq!(read_auth_state(&path).unwrap(), Some(expected));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn select_active_org_prefers_session_active_org_over_stored_slug() {
        let viewer = test_viewer(
            Some("org-beta"),
            vec![
                test_org_with_id("org-acme", "acme", "Acme"),
                test_org_with_id("org-beta", "beta", "Beta"),
            ],
        );
        let mut auth_state = AuthState {
            access_token: "token-123".to_owned(),
            active_org_slug: Some("acme".to_owned()),
            server_url: "https://api.supermanager.dev".to_owned(),
        };

        let outcome = select_active_org(&viewer, &mut auth_state, None).unwrap();

        assert_eq!(outcome.organization_slug, "beta");
        assert_eq!(auth_state.active_org_slug.as_deref(), Some("beta"));
    }

    #[test]
    fn select_active_org_requires_explicit_choice_when_multiple_orgs_have_no_preference() {
        let viewer = test_viewer(
            None,
            vec![test_org("acme", "Acme"), test_org("beta", "Beta")],
        );
        let mut auth_state = AuthState {
            access_token: "token-123".to_owned(),
            active_org_slug: None,
            server_url: "https://api.supermanager.dev".to_owned(),
        };

        let error = select_active_org(&viewer, &mut auth_state, None)
            .unwrap_err()
            .to_string();

        assert!(error.contains("multiple organizations are available"));
        assert!(error.contains("acme (Acme)"));
        assert!(error.contains("beta (Beta)"));
    }

    fn test_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = env::temp_dir().join(format!(
            "supermanager-{name}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn test_viewer(
        active_organization_id: Option<&str>,
        organizations: Vec<ViewerOrganization>,
    ) -> ViewerResponse {
        ViewerResponse {
            active_organization_id: active_organization_id.map(ToOwned::to_owned),
            organizations,
            user: crate::types::ViewerUser {
                name: "Dana".to_owned(),
            },
        }
    }

    fn test_org(slug: &str, name: &str) -> ViewerOrganization {
        test_org_with_id(slug, slug, name)
    }

    fn test_org_with_id(id: &str, slug: &str, name: &str) -> ViewerOrganization {
        ViewerOrganization {
            organization_id: id.to_owned(),
            organization_name: name.to_owned(),
            organization_slug: slug.to_owned(),
        }
    }
}
