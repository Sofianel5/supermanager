use std::{
    collections::BTreeMap,
    env, fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use reporter_protocol::{
    CreateRoomRequest, CreateRoomResponse, HookTurnReport, RoomMetadataResponse,
};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use toml_edit::{DocumentMut, Item, Table, value};

mod update;

pub use update::{SelfUpdateOutcome, maybe_auto_update, run_self_update};

const MANAGED_TOML_START: &str = "# supermanager:start";
const MANAGED_TOML_END: &str = "# supermanager:end";

const CLAUDE_SETTINGS_LOCAL: &str = ".claude/settings.local.json";
const CLAUDE_HOOK_COMMAND: &str = "supermanager hook-report --client claude";
const CODEX_CONFIG: &str = ".codex/config.toml";
const CODEX_HOOKS_JSON: &str = ".codex/hooks.json";
const CODEX_HOOK_COMMAND: &str = "supermanager hook-report --client codex";

const HOME_AUTH_STATE: &str = ".supermanager/auth.json";
const HOME_REPO_CONFIG: &str = ".supermanager/repos.json";
const DEVICE_CLIENT_ID: &str = "supermanager-cli";
const DEVICE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";
const DEVICE_SCOPE: &str = "openid profile email";
const HOOK_TIMEOUT_SECONDS: u64 = 10;
const REPORT_TIMEOUT_SECONDS: u64 = 5;
const API_TIMEOUT_SECONDS: u64 = 10;

pub const DEFAULT_SERVER_URL: &str = "https://api.supermanager.dev";

pub struct JoinConfig {
    pub server_url: String,
    pub organization_slug: Option<String>,
    pub room_id: String,
    pub repo_dir: PathBuf,
    pub home_dir: PathBuf,
}

pub struct JoinOutcome {
    pub room_id: String,
    pub employee_name: String,
    pub dashboard_url: String,
    pub repo_dir: PathBuf,
}

pub struct CreateRoomConfig {
    pub home_dir: PathBuf,
    pub organization_slug: Option<String>,
    pub server_url: String,
    pub name: Option<String>,
    pub cwd: PathBuf,
}

pub struct LoginConfig {
    pub home_dir: PathBuf,
    pub organization_slug: Option<String>,
    pub server_url: String,
}

pub struct LoginOutcome {
    pub active_org_slug: Option<String>,
    pub server_url: String,
}

pub struct CreateRoomOutcome {
    pub room_id: String,
    pub room_name: String,
    pub dashboard_url: String,
    pub join_command: String,
    pub repo_dir: PathBuf,
}

pub struct LeaveOutcome {
    pub repo_dir: PathBuf,
    pub removed_paths: Vec<String>,
}

pub struct ListRoomsOutcome {
    pub rooms: Vec<ListRoomEntry>,
}

pub struct ListRoomEntry {
    pub organization_slug: String,
    pub room_id: String,
    pub server_url: String,
    pub repo_dirs: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct RepoRoomConfig {
    api_key: String,
    api_key_id: String,
    organization_slug: String,
    server_url: String,
    room_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct AuthState {
    access_token: String,
    active_org_slug: Option<String>,
    server_url: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct HomeRepoConfig {
    #[serde(default)]
    repos: BTreeMap<String, RepoRoomConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct ViewerResponse {
    active_organization_id: Option<String>,
    organizations: Vec<ViewerOrganization>,
}

#[derive(Debug, Clone, Deserialize)]
struct ViewerOrganization {
    organization_id: String,
    organization_name: String,
    organization_slug: String,
}

#[derive(Debug, Deserialize)]
struct ConnectionResponse {
    api_key: String,
    api_key_id: String,
    dashboard_url: String,
    room_id: String,
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: Option<String>,
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

pub fn resolve_home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("HOME is not set"))
}

pub fn login(config: LoginConfig) -> Result<LoginOutcome> {
    let server_url = normalize_url(&config.server_url);
    let http = build_http_client(API_TIMEOUT_SECONDS)?;
    let device = request_device_code(&http, &server_url)?;
    let verification_url = device
        .verification_uri_complete
        .clone()
        .unwrap_or_else(|| device.verification_uri.clone());
    let polling_interval = device.interval.unwrap_or(5).max(1);

    let _ = open_url(&verification_url);
    println!();
    println!("  Open this URL to approve the CLI login:");
    println!("    {verification_url}");
    println!("  Code: {}", device.user_code);
    println!();
    println!("  Waiting for approval...");
    println!();

    let access_token =
        poll_for_access_token(&http, &server_url, &device.device_code, polling_interval)?;
    let viewer = get_viewer(&http, &server_url, &access_token)?;
    let active_org_slug = resolve_login_org_slug(&viewer, config.organization_slug.as_deref())?;

    write_auth_state(
        &auth_state_path(&config.home_dir),
        &AuthState {
            access_token,
            active_org_slug: active_org_slug.clone(),
            server_url: server_url.clone(),
        },
    )?;

    Ok(LoginOutcome {
        active_org_slug,
        server_url,
    })
}

pub fn logout(home_dir: &Path) -> Result<bool> {
    let path = auth_state_path(home_dir);
    match fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(
            anyhow::Error::new(error).context(format!("failed to remove {}", path.display()))
        ),
    }
}

pub fn create_room(config: CreateRoomConfig) -> Result<CreateRoomOutcome> {
    let repo_dir = resolve_repo_root(&config.cwd)?;
    let server_url = normalize_url(&config.server_url);
    let room_name = config
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| default_room_name(&repo_dir));
    let http = build_http_client(API_TIMEOUT_SECONDS)?;
    let mut auth_state = require_auth_state(&config.home_dir, &server_url)?;
    let viewer = get_viewer(&http, &server_url, &auth_state.access_token)?;
    let active_org = select_active_org(
        &viewer,
        &mut auth_state,
        config.organization_slug.as_deref(),
    )?;
    write_auth_state(&auth_state_path(&config.home_dir), &auth_state)?;
    let response = authed(&http, &auth_state.access_token)
        .post(format!("{server_url}/v1/rooms"))
        .json(&CreateRoomRequest {
            name: room_name.clone(),
            organization_slug: Some(active_org.organization_slug.clone()),
        })
        .send()
        .context("failed to create room")?;
    let response = ensure_success(response, "create room")?;
    let payload: CreateRoomResponse = response
        .json()
        .context("failed to parse create-room response JSON")?;

    Ok(CreateRoomOutcome {
        room_id: payload.room_id,
        room_name,
        dashboard_url: payload.dashboard_url,
        join_command: payload.join_command,
        repo_dir,
    })
}

pub fn join_repo(config: JoinConfig) -> Result<JoinOutcome> {
    let repo_dir = resolve_repo_root(&config.repo_dir)?;
    let employee_name = detect_employee_name(&repo_dir)?;
    let server_url = normalize_url(&config.server_url);
    let http = build_http_client(API_TIMEOUT_SECONDS)?;
    let mut auth_state = require_auth_state(&config.home_dir, &server_url)?;
    let viewer = get_viewer(&http, &server_url, &auth_state.access_token)?;
    let active_org = select_active_org(
        &viewer,
        &mut auth_state,
        config.organization_slug.as_deref(),
    )?;
    write_auth_state(&auth_state_path(&config.home_dir), &auth_state)?;
    let room = fetch_room(
        &http,
        &server_url,
        &auth_state.access_token,
        &config.room_id,
    )?;

    if room.organization_slug != active_org.organization_slug {
        bail!(
            "room {} belongs to organization {}, but the active organization is {}",
            room.room_id,
            room.organization_slug,
            active_org.organization_slug
        );
    }

    let connection = authed(&http, &auth_state.access_token)
        .post(format!(
            "{server_url}/v1/rooms/{}/connections",
            room.room_id
        ))
        .json(&json!({
            "repo_root": repo_dir.display().to_string()
        }))
        .send()
        .context("failed to create repo connection")?;
    let connection = ensure_success(connection, "create repo connection")?;
    let connection: ConnectionResponse = connection
        .json()
        .context("failed to parse repo-connection response JSON")?;

    let room_config = RepoRoomConfig {
        api_key: connection.api_key,
        api_key_id: connection.api_key_id,
        organization_slug: room.organization_slug.clone(),
        server_url,
        room_id: room.room_id.clone(),
    };

    upsert_repo_room_config(&config.home_dir, &repo_dir, room_config)?;
    install_repo_hooks(&repo_dir)?;

    Ok(JoinOutcome {
        room_id: connection.room_id,
        employee_name,
        dashboard_url: connection.dashboard_url,
        repo_dir,
    })
}

pub fn copy_to_clipboard(text: &str) -> Result<()> {
    let commands: &[(&str, &[&str])] = if cfg!(target_os = "macos") {
        &[("pbcopy", &[])]
    } else if cfg!(target_os = "windows") {
        &[("clip", &[])]
    } else {
        &[
            ("wl-copy", &[]),
            ("xclip", &["-selection", "clipboard"]),
            ("xsel", &["--clipboard", "--input"]),
        ]
    };

    let mut last_error = None;
    for (program, args) in commands {
        match run_clipboard_command(program, args, text) {
            Ok(()) => return Ok(()),
            Err(error) => last_error = Some(error),
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow!("no clipboard command available")))
}

pub fn leave_repo(repo_dir: &Path, home_dir: &Path) -> Result<LeaveOutcome> {
    let repo_dir = canonicalize_best_effort(repo_dir);
    if !repo_dir.exists() {
        bail!("repo path does not exist: {}", repo_dir.display());
    }

    let mut removed_paths = Vec::new();

    if remove_command_hook(&repo_dir.join(CLAUDE_SETTINGS_LOCAL), CLAUDE_HOOK_COMMAND)? {
        removed_paths.push(CLAUDE_SETTINGS_LOCAL.to_owned());
    }
    if remove_command_hook(&repo_dir.join(CODEX_HOOKS_JSON), CODEX_HOOK_COMMAND)? {
        removed_paths.push(CODEX_HOOKS_JSON.to_owned());
    }
    if remove_repo_room_config(home_dir, &repo_dir)? {
        removed_paths.push("$HOME/.supermanager/repos.json".to_owned());
    }

    if removed_paths.is_empty() {
        removed_paths.push("nothing to remove".to_owned());
    }

    Ok(LeaveOutcome {
        repo_dir,
        removed_paths,
    })
}

pub fn list_rooms(home_dir: &Path) -> Result<ListRoomsOutcome> {
    let path = home_repo_config_path(home_dir);
    let config = read_home_repo_config(&path)?;
    let mut grouped = BTreeMap::<(String, String, String), Vec<PathBuf>>::new();

    for (repo_dir, room_config) in config.repos {
        grouped
            .entry((
                room_config.room_id,
                room_config.server_url,
                room_config.organization_slug,
            ))
            .or_default()
            .push(PathBuf::from(repo_dir));
    }

    let rooms = grouped
        .into_iter()
        .map(
            |((room_id, server_url, organization_slug), mut repo_dirs)| {
                repo_dirs.sort();
                ListRoomEntry {
                    organization_slug,
                    room_id,
                    server_url,
                    repo_dirs,
                }
            },
        )
        .collect();

    Ok(ListRoomsOutcome { rooms })
}

pub fn report_hook_turn(client: &str, home_dir: &Path) -> Result<()> {
    let payload = read_hook_payload()?;
    let Some((repo_dir, report)) = build_hook_report(client, &payload)? else {
        return Ok(());
    };

    let Some(room_config) = get_repo_room_config(home_dir, &repo_dir)? else {
        return Ok(());
    };

    let url = format!(
        "{}/v1/hooks/turn",
        room_config.server_url.trim_end_matches('/')
    );

    let http = build_http_client(REPORT_TIMEOUT_SECONDS)?;

    let response = http
        .post(url)
        .header("x-api-key", &room_config.api_key)
        .json(&report)
        .send()
        .context("failed to post hook turn report")?;

    if !response.status().is_success() {
        bail!("hook turn report returned {}", response.status());
    }

    Ok(())
}

fn default_room_name(repo_dir: &Path) -> String {
    path_basename(repo_dir).unwrap_or_else(|| "supermanager room".to_owned())
}

fn install_repo_hooks(repo_dir: &Path) -> Result<()> {
    upsert_command_hooks(
        &repo_dir.join(CLAUDE_SETTINGS_LOCAL),
        &[("UserPromptSubmit", CLAUDE_HOOK_COMMAND), ("Stop", CLAUDE_HOOK_COMMAND)],
    )?;

    upsert_codex_config(&repo_dir.join(CODEX_CONFIG))?;
    upsert_command_hooks(
        &repo_dir.join(CODEX_HOOKS_JSON),
        &[("UserPromptSubmit", CODEX_HOOK_COMMAND), ("Stop", CODEX_HOOK_COMMAND)],
    )?;

    Ok(())
}

struct AuthedClient<'a> {
    http: &'a Client,
    token: &'a str,
}

impl<'a> AuthedClient<'a> {
    fn get(&self, url: String) -> reqwest::blocking::RequestBuilder {
        self.http.get(url).bearer_auth(self.token)
    }

    fn post(&self, url: String) -> reqwest::blocking::RequestBuilder {
        self.http.post(url).bearer_auth(self.token)
    }
}

fn authed<'a>(http: &'a Client, token: &'a str) -> AuthedClient<'a> {
    AuthedClient { http, token }
}

fn auth_state_path(home_dir: &Path) -> PathBuf {
    home_dir.join(HOME_AUTH_STATE)
}

fn require_auth_state(home_dir: &Path, server_url: &str) -> Result<AuthState> {
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

fn read_auth_state(path: &Path) -> Result<Option<AuthState>> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(
                anyhow::Error::new(error).context(format!("failed to read {}", path.display()))
            )
        }
    };
    if text.trim().is_empty() {
        return Ok(None);
    }

    let state = serde_json::from_str(&text)
        .with_context(|| format!("failed to parse JSON in {}", path.display()))?;
    Ok(Some(state))
}

fn write_auth_state(path: &Path, state: &AuthState) -> Result<()> {
    let text = serde_json::to_string_pretty(state)
        .with_context(|| format!("failed to serialize {}", path.display()))?;
    write_private_text(path, &(text + "\n"))
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

fn get_viewer(http: &Client, server_url: &str, access_token: &str) -> Result<ViewerResponse> {
    let response = authed(http, access_token)
        .get(format!("{server_url}/v1/me"))
        .send()
        .context("failed to fetch current account")?;
    let response = ensure_success(response, "fetch current account")?;

    response
        .json()
        .context("failed to parse current-account response JSON")
}

fn fetch_room(
    http: &Client,
    server_url: &str,
    access_token: &str,
    room_id: &str,
) -> Result<RoomMetadataResponse> {
    let response = authed(http, access_token)
        .get(format!("{server_url}/v1/rooms/{room_id}"))
        .send()
        .with_context(|| format!("failed to fetch room {room_id}"))?;
    let response = ensure_success(response, "get room")?;

    response
        .json()
        .context("failed to parse room response JSON")
}

/// Try to find a single preferred org from the viewer's memberships.
/// Resolution order: explicit slug > active org id > single-org auto-select.
fn find_preferred_org<'a>(
    viewer: &'a ViewerResponse,
    requested_slug: Option<&str>,
    fallback_slug: Option<&str>,
) -> Result<Option<&'a ViewerOrganization>> {
    let slug = requested_slug
        .or(fallback_slug)
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

fn resolve_login_org_slug(
    viewer: &ViewerResponse,
    requested_slug: Option<&str>,
) -> Result<Option<String>> {
    Ok(find_preferred_org(viewer, requested_slug, None)?
        .map(|organization| organization.organization_slug.clone()))
}

fn select_active_org(
    viewer: &ViewerResponse,
    auth_state: &mut AuthState,
    requested_slug: Option<&str>,
) -> Result<ViewerOrganization> {
    let organization = find_preferred_org(
        viewer,
        requested_slug,
        auth_state.active_org_slug.as_deref(),
    )?
    .ok_or_else(|| {
        anyhow!(
            "multiple organizations are available; rerun with `--org <slug>`. Available organizations: {}",
            format_org_choices(viewer)
        )
    })?
    .clone();

    auth_state.active_org_slug = Some(organization.organization_slug.clone());
    Ok(organization)
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

fn open_url(url: &str) -> Result<()> {
    let commands: &[(&str, &[&str])] = if cfg!(target_os = "macos") {
        &[("open", &[])]
    } else if cfg!(target_os = "windows") {
        &[("cmd", &["/C", "start", ""])]
    } else {
        &[("xdg-open", &[])]
    };

    let mut last_error = None;
    for (program, args) in commands {
        let result = Command::new(program)
            .args(*args)
            .arg(url)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .and_then(|mut child| child.wait());
        match result {
            Ok(status) if status.success() => return Ok(()),
            Ok(status) => {
                last_error = Some(anyhow!("{program} exited with {status}"));
            }
            Err(error) => {
                last_error = Some(error.into());
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow!("no browser opener available")))
}

fn path_basename(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
}

fn build_http_client(timeout_seconds: u64) -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(timeout_seconds))
        .build()
        .context("failed to build HTTP client")
}

fn ensure_success(
    response: reqwest::blocking::Response,
    action: &str,
) -> Result<reqwest::blocking::Response> {
    if response.status().is_success() {
        return Ok(response);
    }

    let status = response.status();
    let body = response.text().unwrap_or_default();
    let body = body.trim();
    if body.is_empty() {
        bail!("{action} returned {status}");
    }
    bail!("{action} returned {status}: {body}");
}

fn run_clipboard_command(program: &str, args: &[&str], text: &str) -> Result<()> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to start {program}"))?;

    let Some(mut stdin) = child.stdin.take() else {
        bail!("{program} did not expose stdin");
    };
    stdin
        .write_all(text.as_bytes())
        .with_context(|| format!("failed to write clipboard contents to {program}"))?;
    drop(stdin);

    let output = child
        .wait_with_output()
        .with_context(|| format!("failed to wait for {program}"))?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if stderr.is_empty() {
        bail!("{program} exited with {}", output.status);
    }
    bail!("{program} exited with {}: {stderr}", output.status);
}

fn build_hook_report(client: &str, payload: &Value) -> Result<Option<(PathBuf, HookTurnReport)>> {
    if !payload.is_object() {
        return Ok(None);
    }

    let cwd = payload
        .get("cwd")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or(env::current_dir().context("failed to resolve current directory")?);
    let repo_dir = resolve_repo_root(&cwd)?;
    let employee_name = detect_employee_name(&repo_dir)?;

    let report = HookTurnReport {
        employee_name,
        client: client.to_owned(),
        repo_root: repo_dir.display().to_string(),
        branch: git_command_value(&repo_dir, &["branch", "--show-current"])?,
        payload: payload.clone(),
    };

    Ok(Some((repo_dir, report)))
}

fn read_hook_payload() -> Result<Value> {
    let mut raw = String::new();
    io::stdin()
        .read_to_string(&mut raw)
        .context("failed to read hook payload from stdin")?;

    if raw.trim().is_empty() {
        return Ok(Value::Null);
    }

    let value =
        serde_json::from_str(&raw).context("failed to parse hook payload JSON from stdin")?;
    Ok(value)
}

fn detect_employee_name(repo_dir: &Path) -> Result<String> {
    if let Some(name) = git_command_value(repo_dir, &["config", "user.name"])? {
        return Ok(name);
    }
    if let Some(name) = git_command_value(repo_dir, &["config", "--global", "user.name"])? {
        return Ok(name);
    }
    if let Some(name) = env::var_os("USER")
        .or_else(|| env::var_os("USERNAME"))
        .and_then(|value| {
            let text = value.to_string_lossy().trim().to_owned();
            if text.is_empty() { None } else { Some(text) }
        })
    {
        return Ok(name);
    }

    let whoami = Command::new("whoami").current_dir(repo_dir).output();
    if let Ok(output) = whoami
        && output.status.success()
    {
        let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !text.is_empty() {
            return Ok(text);
        }
    }

    bail!("could not detect employee name; set git user.name first")
}

fn normalize_url(url: &str) -> String {
    url.trim_end_matches('/').to_owned()
}

fn resolve_repo_root(cwd: &Path) -> Result<PathBuf> {
    let cwd = canonicalize_best_effort(cwd);
    if !cwd.exists() {
        bail!("repo path does not exist: {}", cwd.display());
    }

    let Some(root) = git_command_value(&cwd, &["rev-parse", "--show-toplevel"])? else {
        bail!("not inside a git repository: {}", cwd.display());
    };
    Ok(canonicalize_best_effort(Path::new(&root)))
}

fn canonicalize_best_effort(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn git_command_value(repo_dir: &Path, args: &[&str]) -> Result<Option<String>> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_dir)
        .output();

    let output = match output {
        Ok(output) => output,
        Err(_) => return Ok(None),
    };
    if !output.status.success() {
        return Ok(None);
    }

    let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if text.is_empty() {
        Ok(None)
    } else {
        Ok(Some(text))
    }
}

fn upsert_repo_room_config(
    home_dir: &Path,
    repo_dir: &Path,
    room_config: RepoRoomConfig,
) -> Result<()> {
    let path = home_repo_config_path(home_dir);
    let mut config = read_home_repo_config(&path)?;
    config.repos.insert(repo_key(repo_dir), room_config);
    write_home_repo_config(&path, &config)
}

fn get_repo_room_config(home_dir: &Path, repo_dir: &Path) -> Result<Option<RepoRoomConfig>> {
    let path = home_repo_config_path(home_dir);
    let config = read_home_repo_config(&path)?;
    Ok(config.repos.get(&repo_key(repo_dir)).cloned())
}

fn remove_repo_room_config(home_dir: &Path, repo_dir: &Path) -> Result<bool> {
    let path = home_repo_config_path(home_dir);
    if !path.exists() {
        return Ok(false);
    }

    let mut config = read_home_repo_config(&path)?;
    let removed = config.repos.remove(&repo_key(repo_dir)).is_some();
    if !removed {
        return Ok(false);
    }

    write_home_repo_config(&path, &config)?;
    Ok(true)
}

fn repo_key(repo_dir: &Path) -> String {
    canonicalize_best_effort(repo_dir).display().to_string()
}

fn home_repo_config_path(home_dir: &Path) -> PathBuf {
    home_dir.join(HOME_REPO_CONFIG)
}

fn read_home_repo_config(path: &Path) -> Result<HomeRepoConfig> {
    if !path.exists() {
        return Ok(HomeRepoConfig::default());
    }

    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    if text.trim().is_empty() {
        return Ok(HomeRepoConfig::default());
    }

    serde_json::from_str(&text)
        .with_context(|| format!("failed to parse JSON in {}", path.display()))
}

fn write_home_repo_config(path: &Path, config: &HomeRepoConfig) -> Result<()> {
    if config.repos.is_empty() {
        if path.exists() {
            fs::remove_file(path)
                .with_context(|| format!("failed to remove {}", path.display()))?;
        }
        return Ok(());
    }

    let text = serde_json::to_string_pretty(config)
        .with_context(|| format!("failed to serialize {}", path.display()))?;
    write_private_text(path, &(text + "\n"))
}

fn upsert_command_hooks(path: &Path, hooks_to_add: &[(&str, &str)]) -> Result<()> {
    let mut root = read_json_object(path)?;
    let hooks = ensure_object_field(&mut root, "hooks")?;

    for &(event, command) in hooks_to_add {
        let entries = hooks
            .entry(event.to_owned())
            .or_insert_with(|| Value::Array(Vec::new()));
        let entries = entries
            .as_array_mut()
            .ok_or_else(|| anyhow!("{} has a non-array hooks.{event} field", path.display()))?;

        if !entries
            .iter()
            .any(|entry| entry_contains_command(entry, command))
        {
            entries.push(json!({
                "hooks": [{
                    "type": "command",
                    "command": command,
                    "timeout": HOOK_TIMEOUT_SECONDS
                }]
            }));
        }
    }

    write_json_object(path, &root)
}

fn remove_command_hook(path: &Path, command: &str) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }

    let mut root = read_json_object(path)?;
    let mut removed = false;

    if let Some(hooks) = root.get_mut("hooks").and_then(Value::as_object_mut) {
        let event_names = hooks.keys().cloned().collect::<Vec<_>>();
        for event in event_names {
            let Some(entries) = hooks.get_mut(&event).and_then(Value::as_array_mut) else {
                continue;
            };

            let mut keep = Vec::with_capacity(entries.len());
            for mut entry in std::mem::take(entries) {
                if remove_command_from_entry(&mut entry, command) {
                    removed = true;
                }
                if !entry_has_empty_hooks(&entry) {
                    keep.push(entry);
                }
            }
            *entries = keep;

            if entries.is_empty() {
                hooks.remove(&event);
            }
        }

        if hooks.is_empty() {
            root.remove("hooks");
        }
    }

    if !removed {
        return Ok(false);
    }

    if root.is_empty() {
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))?;
    } else {
        write_json_object(path, &root)?;
    }

    Ok(true)
}

fn entry_contains_command(entry: &Value, command: &str) -> bool {
    entry
        .get("hooks")
        .and_then(Value::as_array)
        .map(|hooks| {
            hooks.iter().any(|hook| {
                hook.get("type").and_then(Value::as_str) == Some("command")
                    && hook.get("command").and_then(Value::as_str) == Some(command)
            })
        })
        .unwrap_or(false)
}

fn remove_command_from_entry(entry: &mut Value, command: &str) -> bool {
    let Some(hooks) = entry.get_mut("hooks").and_then(Value::as_array_mut) else {
        return false;
    };

    let before = hooks.len();
    hooks.retain(|hook| {
        !(hook.get("type").and_then(Value::as_str) == Some("command")
            && hook.get("command").and_then(Value::as_str) == Some(command))
    });
    before != hooks.len()
}

fn entry_has_empty_hooks(entry: &Value) -> bool {
    entry
        .get("hooks")
        .and_then(Value::as_array)
        .map(|hooks| hooks.is_empty())
        .unwrap_or(false)
}

fn upsert_codex_config(path: &Path) -> Result<()> {
    let existing = read_optional_text(path)?;
    let normalized = strip_managed_toml_markers(&existing);
    let mut doc = parse_toml_document(&normalized, path)?;

    remove_legacy_supermanager_mcp(&mut doc);
    upsert_codex_features_table(&mut doc, path)?;

    let next = normalize_toml_text(doc.to_string());

    if next.trim().is_empty() {
        return Ok(());
    }

    if next != existing {
        let mut normalized = next;
        if !normalized.ends_with('\n') {
            normalized.push('\n');
        }
        write_text(path, &normalized)?;
    }

    Ok(())
}

fn parse_toml_document(text: &str, path: &Path) -> Result<DocumentMut> {
    if text.trim().is_empty() {
        return Ok(DocumentMut::new());
    }

    text.parse::<DocumentMut>()
        .with_context(|| format!("failed to parse TOML in {}", path.display()))
}

fn strip_managed_toml_markers(text: &str) -> String {
    text.lines()
        .filter(|line| {
            let trimmed = line.trim();
            trimmed != MANAGED_TOML_START && trimmed != MANAGED_TOML_END
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn remove_legacy_supermanager_mcp(doc: &mut DocumentMut) {
    if let Some(mcp_servers) = doc.get_mut("mcp_servers")
        && let Some(table) = mcp_servers.as_table_like_mut()
    {
        table.remove("supermanager");
        if table.is_empty() {
            *mcp_servers = Item::None;
        }
    }
}

fn upsert_codex_features_table(doc: &mut DocumentMut, path: &Path) -> Result<()> {
    let existing = doc.as_table_mut().remove("features");
    let mut features = match existing {
        Some(item) => item
            .into_table()
            .map_err(|_| anyhow!("{} has a non-table features entry", path.display()))?,
        None => Table::new(),
    };

    features.set_implicit(false);
    features["codex_hooks"] = value(true);
    doc["features"] = Item::Table(features);
    Ok(())
}

fn normalize_toml_text(mut text: String) -> String {
    while text.ends_with('\n') {
        text.pop();
    }
    if !text.is_empty() {
        text.push('\n');
    }
    text
}

fn read_json_object(path: &Path) -> Result<Map<String, Value>> {
    if !path.exists() {
        return Ok(Map::new());
    }

    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    if text.trim().is_empty() {
        return Ok(Map::new());
    }

    let value: Value = serde_json::from_str(&text)
        .with_context(|| format!("failed to parse JSON in {}", path.display()))?;
    value
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow!("{} does not contain a JSON object", path.display()))
}

fn write_json_object(path: &Path, root: &Map<String, Value>) -> Result<()> {
    let value = Value::Object(root.clone());
    let text = serde_json::to_string_pretty(&value)
        .with_context(|| format!("failed to serialize JSON for {}", path.display()))?;
    write_text(path, &(text + "\n"))
}

fn ensure_object_field<'a>(
    root: &'a mut Map<String, Value>,
    key: &str,
) -> Result<&'a mut Map<String, Value>> {
    let value = root
        .entry(key.to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    value
        .as_object_mut()
        .ok_or_else(|| anyhow!("{key} is not a JSON object"))
}

fn read_optional_text(path: &Path) -> Result<String> {
    if !path.exists() {
        return Ok(String::new());
    }
    fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))
}

fn write_text(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, text).with_context(|| format!("failed to write {}", path.display()))
}

fn write_private_text(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("failed to write {}", path.display()))?;
        file.write_all(text.as_bytes())
            .with_context(|| format!("failed to write {}", path.display()))?;
    }

    #[cfg(not(unix))]
    {
        write_text(path, text)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    #[test]
    fn join_repo_enables_codex_hooks_in_existing_features_section() {
        let root = test_dir("join-repo");
        let repo_dir = root.join("repo");
        fs::create_dir_all(&repo_dir).unwrap();
        init_git_repo(&repo_dir);

        write_text(
            &repo_dir.join(CODEX_CONFIG),
            "[features]\nother_flag = true\ncodex_hooks = false\n",
        )
        .unwrap();

        install_repo_hooks(&repo_dir).unwrap();

        let codex_config = fs::read_to_string(repo_dir.join(CODEX_CONFIG)).unwrap();
        assert!(codex_config.contains("[features]"));
        assert!(codex_config.contains("other_flag = true"));
        assert!(codex_config.contains("codex_hooks = true"));
        assert!(!codex_config.contains("codex_hooks = false"));

        let hooks = fs::read_to_string(repo_dir.join(CODEX_HOOKS_JSON)).unwrap();
        assert!(hooks.contains(CODEX_HOOK_COMMAND));

        fs::remove_dir_all(root).unwrap();
    }

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
    fn resolve_repo_root_fails_outside_git_repo() {
        let root = test_dir("resolve-repo-root-missing");
        let repo_dir = root.join("not-a-repo");
        fs::create_dir_all(&repo_dir).unwrap();

        let error = resolve_repo_root(&repo_dir).unwrap_err().to_string();
        assert!(error.contains("not inside a git repository"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn upsert_codex_config_adds_features_section_when_missing() {
        let root = test_dir("codex-config-missing");
        let config_path = root.join("config.toml");

        write_text(
            &config_path,
            "[model_providers.openai]\nname = \"OpenAI\"\n",
        )
        .unwrap();
        upsert_codex_config(&config_path).unwrap();

        let codex_config = fs::read_to_string(&config_path).unwrap();
        assert!(codex_config.contains("[model_providers.openai]"));
        assert!(codex_config.contains("[features]"));
        assert!(codex_config.contains("codex_hooks = true"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn upsert_codex_config_rewrites_dotted_key_to_true() {
        let root = test_dir("codex-config-dotted");
        let config_path = root.join("config.toml");

        write_text(&config_path, "features.codex_hooks = false\n").unwrap();
        upsert_codex_config(&config_path).unwrap();

        let codex_config = fs::read_to_string(&config_path).unwrap();
        assert!(codex_config.contains("features.codex_hooks = true"));
        assert!(!codex_config.contains("features.codex_hooks = false"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn upsert_codex_config_removes_legacy_supermanager_mcp_sections() {
        let root = test_dir("codex-config-legacy-mcp");
        let config_path = root.join("config.toml");

        write_text(
            &config_path,
            "[mcp_servers.supermanager]\nurl = \"http://example.test\"\n\n[mcp_servers.supermanager.tools.submit_progress]\napproval_mode = \"approve\"\n",
        )
        .unwrap();
        upsert_codex_config(&config_path).unwrap();

        let codex_config = fs::read_to_string(&config_path).unwrap();
        assert!(!codex_config.contains("mcp_servers.supermanager"));
        assert!(codex_config.contains("[features]"));
        assert!(codex_config.contains("codex_hooks = true"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn build_hook_report_preserves_raw_payload() {
        let root = test_dir("hook-report");
        let repo_dir = root.join("repo");
        let nested_dir = repo_dir.join("nested");
        fs::create_dir_all(&nested_dir).unwrap();
        init_git_repo(&repo_dir);

        let payload = json!({
            "hook_event_name": "Stop",
            "session_id": "abc123",
            "cwd": nested_dir.display().to_string(),
            "last_assistant_message": "Implemented the hook pipeline",
            "extra": {
                "nested": true
            }
        });

        let (resolved_repo, report) = build_hook_report("codex", &payload).unwrap().unwrap();

        assert_eq!(resolved_repo, canonicalize_best_effort(&repo_dir));
        assert_eq!(report.client, "codex");
        assert_eq!(
            report.repo_root,
            canonicalize_best_effort(&repo_dir).display().to_string()
        );
        assert_eq!(report.payload, payload);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn default_room_name_prefers_repo_root_name() {
        let root = test_dir("default-room-name");
        let repo_dir = root.join("repo-name");
        let nested_dir = repo_dir.join("nested");
        fs::create_dir_all(&nested_dir).unwrap();
        init_git_repo(&repo_dir);

        let resolved_repo_dir = resolve_repo_root(&nested_dir).unwrap();
        assert_eq!(default_room_name(&resolved_repo_dir), "repo-name");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn list_rooms_groups_repo_memberships_by_room() {
        let root = test_dir("list-rooms");
        let home_dir = root.join("home");
        let repo_a = root.join("repo-a");
        let repo_b = root.join("repo-b");
        let repo_c = root.join("repo-c");
        fs::create_dir_all(&home_dir).unwrap();
        fs::create_dir_all(&repo_a).unwrap();
        fs::create_dir_all(&repo_b).unwrap();
        fs::create_dir_all(&repo_c).unwrap();

        let mut config = HomeRepoConfig::default();
        config.repos.insert(
            repo_key(&repo_b),
            RepoRoomConfig {
                api_key: "key-b".to_owned(),
                api_key_id: "key-b-id".to_owned(),
                organization_slug: "acme".to_owned(),
                server_url: "https://api.supermanager.dev".to_owned(),
                room_id: "ALPHA1".to_owned(),
            },
        );
        config.repos.insert(
            repo_key(&repo_a),
            RepoRoomConfig {
                api_key: "key-a".to_owned(),
                api_key_id: "key-a-id".to_owned(),
                organization_slug: "acme".to_owned(),
                server_url: "https://api.supermanager.dev".to_owned(),
                room_id: "ALPHA1".to_owned(),
            },
        );
        config.repos.insert(
            repo_key(&repo_c),
            RepoRoomConfig {
                api_key: "key-c".to_owned(),
                api_key_id: "key-c-id".to_owned(),
                organization_slug: "beta".to_owned(),
                server_url: "http://127.0.0.1:8787".to_owned(),
                room_id: "BETA22".to_owned(),
            },
        );

        write_home_repo_config(&home_repo_config_path(&home_dir), &config).unwrap();

        let outcome = list_rooms(&home_dir).unwrap();

        assert_eq!(outcome.rooms.len(), 2);
        assert_eq!(outcome.rooms[0].room_id, "ALPHA1");
        assert_eq!(outcome.rooms[0].organization_slug, "acme");
        assert_eq!(outcome.rooms[0].server_url, "https://api.supermanager.dev");
        assert_eq!(
            outcome.rooms[0].repo_dirs,
            vec![
                canonicalize_best_effort(&repo_a),
                canonicalize_best_effort(&repo_b)
            ]
        );
        assert_eq!(outcome.rooms[1].room_id, "BETA22");
        assert_eq!(outcome.rooms[1].organization_slug, "beta");
        assert_eq!(outcome.rooms[1].server_url, "http://127.0.0.1:8787");
        assert_eq!(
            outcome.rooms[1].repo_dirs,
            vec![canonicalize_best_effort(&repo_c)]
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn list_rooms_returns_empty_when_not_joined_anywhere() {
        let root = test_dir("list-rooms-empty");
        let home_dir = root.join("home");
        fs::create_dir_all(&home_dir).unwrap();

        let outcome = list_rooms(&home_dir).unwrap();

        assert!(outcome.rooms.is_empty());

        fs::remove_dir_all(root).unwrap();
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

    fn init_git_repo(path: &Path) {
        let init = Command::new("git")
            .args(["init", "-q"])
            .current_dir(path)
            .status()
            .unwrap();
        assert!(init.success());

        let user = Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(path)
            .status()
            .unwrap();
        assert!(user.success());
    }
}
