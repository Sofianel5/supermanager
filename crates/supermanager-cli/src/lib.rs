use std::{
    collections::BTreeMap,
    env, fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use reporter_protocol::{
    AuthConfigResponse, CliRefreshRequest, CliRefreshResponse, CreateInviteRequest,
    CreateRoomRequest, CreateRoomResponse, CurrentUserResponse, HookTurnReport, InviteResponse,
    RoomMetadataResponse,
};
use reqwest::blocking::{Client, Response};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use toml_edit::{DocumentMut, Item, Table, value};
use workos_client::types::{
    AuthenticateBody, DeviceAuthenticateRequest, DeviceAuthorizationRequest,
};

mod update;

pub use update::{SelfUpdateOutcome, maybe_auto_update, run_self_update};

const MANAGED_TOML_START: &str = "# supermanager:start";
const MANAGED_TOML_END: &str = "# supermanager:end";

const CLAUDE_SETTINGS_LOCAL: &str = ".claude/settings.local.json";
const CLAUDE_HOOK_COMMAND: &str = "supermanager hook-report --client claude";
const CODEX_CONFIG: &str = ".codex/config.toml";
const CODEX_HOOKS_JSON: &str = ".codex/hooks.json";
const CODEX_HOOK_COMMAND: &str = "supermanager hook-report --client codex";

const HOME_REPO_CONFIG: &str = ".supermanager/repos.json";
const HOME_AUTH_CONFIG: &str = ".supermanager/auth.json";
const HOOK_TIMEOUT_SECONDS: u64 = 10;
const REPORT_TIMEOUT_SECONDS: u64 = 5;
const API_TIMEOUT_SECONDS: u64 = 10;
const LOGIN_POLL_TIMEOUT_SECONDS: u64 = 180;

pub const DEFAULT_SERVER_URL: &str = "https://api.supermanager.dev";
pub const DEFAULT_APP_URL: &str = "https://supermanager.dev";

pub struct JoinConfig {
    pub server_url: String,
    pub app_url: String,
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
    pub server_url: String,
    pub name: Option<String>,
    pub cwd: PathBuf,
    pub home_dir: PathBuf,
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
    pub room_id: String,
    pub server_url: String,
    pub repo_dirs: Vec<PathBuf>,
}

pub struct LoginConfig {
    pub server_url: String,
    pub home_dir: PathBuf,
}

pub struct LoginOutcome {
    pub user: CurrentUserResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredAuth {
    server_url: String,
    access_token: String,
    refresh_token: String,
    access_expires_at: String,
    user: CurrentUserResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct RepoRoomConfig {
    server_url: String,
    room_id: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct HomeRepoConfig {
    #[serde(default)]
    repos: BTreeMap<String, RepoRoomConfig>,
}

enum DevicePoll {
    Pending,
    Complete(workos_client::types::AuthenticateResponse),
}

#[derive(Debug, Deserialize)]
struct WorkosAuthErrorResponse {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

pub fn resolve_home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("HOME is not set"))
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
    let auth = require_auth(&server_url, &config.home_dir)?;

    let response = http
        .post(format!("{server_url}/v1/rooms"))
        .bearer_auth(auth.access_token)
        .json(&CreateRoomRequest {
            name: room_name.clone(),
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
    let app_url = normalize_url(&config.app_url);
    let room_id = config.room_id.trim().to_ascii_uppercase();
    let dashboard_url = format!("{}/r/{}", app_url, room_id);

    let room_config = RepoRoomConfig {
        server_url,
        room_id: room_id.clone(),
    };

    upsert_repo_room_config(&config.home_dir, &repo_dir, room_config)?;

    upsert_command_hook(
        &repo_dir.join(CLAUDE_SETTINGS_LOCAL),
        "UserPromptSubmit",
        CLAUDE_HOOK_COMMAND,
    )?;
    upsert_command_hook(
        &repo_dir.join(CLAUDE_SETTINGS_LOCAL),
        "Stop",
        CLAUDE_HOOK_COMMAND,
    )?;

    upsert_codex_config(&repo_dir.join(CODEX_CONFIG))?;
    upsert_command_hook(
        &repo_dir.join(CODEX_HOOKS_JSON),
        "UserPromptSubmit",
        CODEX_HOOK_COMMAND,
    )?;
    upsert_command_hook(&repo_dir.join(CODEX_HOOKS_JSON), "Stop", CODEX_HOOK_COMMAND)?;

    Ok(JoinOutcome {
        room_id,
        employee_name,
        dashboard_url,
        repo_dir,
    })
}

pub fn get_room(server_url: &str, room_id: &str, home_dir: &Path) -> Result<RoomMetadataResponse> {
    let server_url = normalize_url(server_url);
    let http = build_http_client(API_TIMEOUT_SECONDS)?;
    let auth = require_auth(&server_url, home_dir)?;
    let response = http
        .get(format!("{server_url}/r/{room_id}"))
        .bearer_auth(auth.access_token)
        .send()
        .with_context(|| format!("failed to fetch room {room_id}"))?;

    let response = ensure_success(response, "get room")?;
    response
        .json()
        .context("failed to parse room response JSON")
}

pub fn login(config: LoginConfig) -> Result<LoginOutcome> {
    let server_url = normalize_url(&config.server_url);
    let http = build_http_client(API_TIMEOUT_SECONDS)?;
    let auth_config = fetch_auth_config(&http, &server_url)?;
    let device = start_device_login(&http, &auth_config.client_id)?;

    open_browser(&device.verification_uri_complete).with_context(|| {
        format!(
            "failed to open browser to {}",
            device.verification_uri_complete
        )
    })?;

    let expires_in = u64::try_from(device.expires_in).unwrap_or(LOGIN_POLL_TIMEOUT_SECONDS);
    let poll_interval_seconds = u64::try_from(device.interval.max(1)).unwrap_or(1);
    let deadline = std::time::Instant::now() + Duration::from_secs(expires_in);
    let poll_interval = Duration::from_secs(poll_interval_seconds);

    let auth = loop {
        if std::time::Instant::now() >= deadline {
            bail!("timed out waiting for browser login");
        }

        match poll_device_login(&http, &auth_config.client_id, &device.device_code)? {
            DevicePoll::Pending => {
                std::thread::sleep(poll_interval);
            }
            DevicePoll::Complete(response) => {
                let auth = StoredAuth {
                    server_url: server_url.clone(),
                    access_expires_at: access_token_expires_at(&response.access_token)?,
                    access_token: response.access_token,
                    refresh_token: response.refresh_token,
                    user: map_workos_user(response.user),
                };
                break auth;
            }
        }
    };

    write_auth(&config.home_dir, &auth)?;
    Ok(LoginOutcome { user: auth.user })
}

pub fn logout(home_dir: &Path) -> Result<bool> {
    let path = auth_config_path(home_dir);
    if !path.exists() {
        return Ok(false);
    }
    fs::remove_file(&path).with_context(|| format!("failed to remove {}", path.display()))?;
    Ok(true)
}

pub fn whoami(server_url: &str, home_dir: &Path) -> Result<CurrentUserResponse> {
    let server_url = normalize_url(server_url);
    let mut auth = require_auth(&server_url, home_dir)?;
    let http = build_http_client(API_TIMEOUT_SECONDS)?;
    current_user_request(&http, &server_url, home_dir, &mut auth)
}

pub fn create_link_invite(
    server_url: &str,
    room_id: &str,
    home_dir: &Path,
) -> Result<InviteResponse> {
    let server_url = normalize_url(server_url);
    let mut auth = require_auth(&server_url, home_dir)?;
    let http = build_http_client(API_TIMEOUT_SECONDS)?;
    authorized_json(
        &http,
        &server_url,
        home_dir,
        &mut auth,
        http.post(format!("{server_url}/r/{room_id}/invites/link")),
        "create link invite",
    )
}

pub fn create_email_invite(
    server_url: &str,
    room_id: &str,
    email: &str,
    home_dir: &Path,
) -> Result<InviteResponse> {
    let server_url = normalize_url(server_url);
    let mut auth = require_auth(&server_url, home_dir)?;
    let http = build_http_client(API_TIMEOUT_SECONDS)?;
    authorized_json(
        &http,
        &server_url,
        home_dir,
        &mut auth,
        http.post(format!("{server_url}/r/{room_id}/invites/email"))
            .json(&CreateInviteRequest {
                target_email: Some(email.to_owned()),
            }),
        "create email invite",
    )
}

pub fn accept_invite(
    server_url: &str,
    token: &str,
    home_dir: &Path,
) -> Result<RoomMetadataResponse> {
    let server_url = normalize_url(server_url);
    let mut auth = require_auth(&server_url, home_dir)?;
    let http = build_http_client(API_TIMEOUT_SECONDS)?;
    let response: reporter_protocol::AcceptInviteResponse = authorized_json(
        &http,
        &server_url,
        home_dir,
        &mut auth,
        http.post(format!("{server_url}/v1/invites/accept")).json(
            &reporter_protocol::AcceptInviteRequest {
                token: token.to_owned(),
            },
        ),
        "accept invite",
    )?;
    Ok(response.room)
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
    let mut grouped = BTreeMap::<(String, String), Vec<PathBuf>>::new();

    for (repo_dir, room_config) in config.repos {
        grouped
            .entry((room_config.room_id, room_config.server_url))
            .or_default()
            .push(PathBuf::from(repo_dir));
    }

    let rooms = grouped
        .into_iter()
        .map(|((room_id, server_url), mut repo_dirs)| {
            repo_dirs.sort();
            ListRoomEntry {
                room_id,
                server_url,
                repo_dirs,
            }
        })
        .collect();

    Ok(ListRoomsOutcome { rooms })
}

pub fn joined_room_for_path(
    home_dir: &Path,
    cwd: &Path,
    server_url: &str,
) -> Result<Option<String>> {
    let repo_dir = resolve_repo_root(cwd)?;
    let Some(room_config) = get_repo_room_config(home_dir, &repo_dir)? else {
        return Ok(None);
    };
    if normalize_url(&room_config.server_url) != normalize_url(server_url) {
        return Ok(None);
    }
    Ok(Some(room_config.room_id))
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
        "{}/r/{}/hooks/turn",
        room_config.server_url.trim_end_matches('/'),
        room_config.room_id,
    );

    let http = build_http_client(REPORT_TIMEOUT_SECONDS)?;
    let mut auth = require_auth(&room_config.server_url, home_dir)?;

    let response = authorized_send(
        &http,
        &room_config.server_url,
        home_dir,
        &mut auth,
        http.post(url).json(&report),
        "post hook turn report",
    )?;
    ensure_success(response, "post hook turn report")?;

    Ok(())
}

fn default_room_name(repo_dir: &Path) -> String {
    path_basename(repo_dir).unwrap_or_else(|| "supermanager room".to_owned())
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

fn auth_config_path(home_dir: &Path) -> PathBuf {
    home_dir.join(HOME_AUTH_CONFIG)
}

fn read_auth(home_dir: &Path) -> Result<StoredAuth> {
    let path = auth_config_path(home_dir);
    let text =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&text)
        .with_context(|| format!("failed to parse JSON in {}", path.display()))
}

fn write_auth(home_dir: &Path, auth: &StoredAuth) -> Result<()> {
    let text = serde_json::to_string_pretty(auth).context("failed to serialize auth config")?;
    write_text(&auth_config_path(home_dir), &(text + "\n"))
}

fn require_auth(server_url: &str, home_dir: &Path) -> Result<StoredAuth> {
    let auth = read_auth(home_dir)
        .with_context(|| "not logged in; run `supermanager login` first".to_owned())?;
    if normalize_url(&auth.server_url) != server_url {
        bail!(
            "login was issued for {}, but command is targeting {}; run `supermanager login --server {server_url}`",
            auth.server_url,
            server_url
        );
    }
    Ok(auth)
}

fn fetch_auth_config(http: &Client, server_url: &str) -> Result<AuthConfigResponse> {
    let response = http
        .get(format!("{server_url}/v1/auth/config"))
        .send()
        .context("failed to load auth config")?;
    let response = ensure_success(response, "load auth config")?;
    response
        .json()
        .context("failed to parse auth config response JSON")
}

fn start_device_login(
    http: &Client,
    client_id: &str,
) -> Result<workos_client::types::DeviceAuthorizationResponse> {
    let response = http
        .post("https://api.workos.com/user_management/authorize/device")
        .json(&DeviceAuthorizationRequest {
            client_id: client_id.to_owned(),
        })
        .send()
        .context("failed to start WorkOS device login")?;
    let response = ensure_success(response, "start device login")?;
    response
        .json()
        .context("failed to parse device-login response JSON")
}

fn poll_device_login(http: &Client, client_id: &str, device_code: &str) -> Result<DevicePoll> {
    let response = http
        .post("https://api.workos.com/user_management/authenticate")
        .json(&AuthenticateBody::DeviceAuthenticateRequest(
            DeviceAuthenticateRequest {
                client_id: client_id.to_owned(),
                device_code: device_code.to_owned(),
                grant_type: "urn:ietf:params:oauth:grant-type:device_code".to_owned(),
            },
        ))
        .send()
        .context("failed to poll WorkOS device login")?;

    if response.status() == reqwest::StatusCode::BAD_REQUEST {
        let error = parse_workos_auth_error(response)?;
        let code = error.code.or(error.error).unwrap_or_default();
        if matches!(code.as_str(), "authorization_pending" | "slow_down") {
            return Ok(DevicePoll::Pending);
        }

        let message = error
            .error_description
            .or(error.message)
            .unwrap_or_else(|| "device login failed".to_owned());
        bail!("{message}");
    }

    let response = ensure_success(response, "complete device login")?;
    Ok(DevicePoll::Complete(response.json().context(
        "failed to parse device-login poll response JSON",
    )?))
}

fn parse_workos_auth_error(response: Response) -> Result<WorkosAuthErrorResponse> {
    response
        .json()
        .context("failed to parse WorkOS auth error response JSON")
}

fn access_token_expires_at(access_token: &str) -> Result<String> {
    #[derive(Deserialize)]
    struct SessionClaims {
        exp: i64,
    }

    let payload = access_token
        .split('.')
        .nth(1)
        .ok_or_else(|| anyhow!("access token payload is missing"))?;
    let decoded = URL_SAFE_NO_PAD
        .decode(payload)
        .context("failed to decode access token payload")?;
    let claims: SessionClaims =
        serde_json::from_slice(&decoded).context("failed to parse access token payload JSON")?;
    let expires_at = time::OffsetDateTime::from_unix_timestamp(claims.exp)
        .context("invalid access token expiry")?;
    expires_at
        .format(&time::format_description::well_known::Rfc3339)
        .context("failed to format access token expiry")
}

fn map_workos_user(user: workos_client::types::User) -> CurrentUserResponse {
    let first = user
        .first_name
        .as_deref()
        .map(str::trim)
        .unwrap_or_default();
    let last = user.last_name.as_deref().map(str::trim).unwrap_or_default();
    let display_name = match (first.is_empty(), last.is_empty()) {
        (false, false) => format!("{first} {last}"),
        (false, true) => first.to_owned(),
        (true, false) => last.to_owned(),
        (true, true) => user.email.clone(),
    };

    CurrentUserResponse {
        user_id: user.id,
        display_name,
        primary_email: user.email,
        avatar_url: user.profile_picture_url,
    }
}

fn open_browser(url: &str) -> Result<()> {
    let commands: &[(&str, &[&str])] = if cfg!(target_os = "macos") {
        &[("open", &[])]
    } else if cfg!(target_os = "windows") {
        &[("cmd", &["/C", "start", ""])]
    } else {
        &[("xdg-open", &[])]
    };

    let mut last_error = None;
    for (program, args) in commands {
        match Command::new(program)
            .args(*args)
            .arg(url)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => {
                let output = child.wait_with_output()?;
                if output.status.success() {
                    return Ok(());
                }
                last_error = Some(anyhow!(
                    "{} exited with {}: {}",
                    program,
                    output.status,
                    String::from_utf8_lossy(&output.stderr).trim()
                ));
            }
            Err(error) => last_error = Some(error.into()),
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow!("no browser launcher available")))
}

fn current_user_request(
    http: &Client,
    server_url: &str,
    home_dir: &Path,
    auth: &mut StoredAuth,
) -> Result<CurrentUserResponse> {
    authorized_json(
        http,
        server_url,
        home_dir,
        auth,
        http.get(format!("{server_url}/v1/me")),
        "whoami",
    )
}

fn authorized_json<T: for<'de> Deserialize<'de>>(
    http: &Client,
    server_url: &str,
    home_dir: &Path,
    auth: &mut StoredAuth,
    request: reqwest::blocking::RequestBuilder,
    action: &str,
) -> Result<T> {
    let response = authorized_send(http, server_url, home_dir, auth, request, action)?;
    let response = ensure_success(response, action)?;
    response
        .json()
        .with_context(|| format!("failed to parse {action} response JSON"))
}

fn authorized_send(
    http: &Client,
    server_url: &str,
    home_dir: &Path,
    auth: &mut StoredAuth,
    request: reqwest::blocking::RequestBuilder,
    action: &str,
) -> Result<Response> {
    if token_expired(&auth.access_expires_at) {
        refresh_auth(http, server_url, home_dir, auth)?;
    }

    let retry = request
        .try_clone()
        .ok_or_else(|| anyhow!("failed to clone HTTP request for {action}"))?;
    let response = request
        .bearer_auth(&auth.access_token)
        .send()
        .with_context(|| format!("failed to {action}"))?;

    if response.status() != reqwest::StatusCode::UNAUTHORIZED {
        return Ok(response);
    }

    refresh_auth(http, server_url, home_dir, auth)?;
    retry
        .bearer_auth(&auth.access_token)
        .send()
        .with_context(|| format!("failed to retry {action}"))
}

fn refresh_auth(
    http: &Client,
    server_url: &str,
    home_dir: &Path,
    auth: &mut StoredAuth,
) -> Result<()> {
    let response = http
        .post(format!("{server_url}/v1/auth/cli/refresh"))
        .json(&CliRefreshRequest {
            refresh_token: auth.refresh_token.clone(),
        })
        .send()
        .context("failed to refresh login")?;
    let response = ensure_success(response, "refresh login")?;
    let payload: CliRefreshResponse = response
        .json()
        .context("failed to parse refresh response JSON")?;
    auth.access_token = payload.access_token;
    auth.refresh_token = payload.refresh_token;
    auth.access_expires_at = payload.access_expires_at;
    auth.user = payload.user;
    write_auth(home_dir, auth)
}

fn token_expired(iso_timestamp: &str) -> bool {
    let timestamp = time::OffsetDateTime::parse(
        iso_timestamp,
        &time::format_description::well_known::Rfc3339,
    );
    match timestamp {
        Ok(timestamp) => timestamp <= time::OffsetDateTime::now_utc() + time::Duration::minutes(1),
        Err(_) => true,
    }
}

pub fn invite_token_from_input(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    if !trimmed.contains("://") {
        return None;
    }
    let url = reqwest::Url::parse(trimmed).ok()?;
    let segments = url.path_segments()?.collect::<Vec<_>>();
    let token = segments
        .windows(2)
        .find(|window| window[0] == "invite")
        .map(|window| window[1].to_owned())?;
    if token.trim().is_empty() {
        None
    } else {
        Some(token)
    }
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
    write_text(path, &(text + "\n"))
}

fn upsert_command_hook(path: &Path, event: &str, command: &str) -> Result<()> {
    let mut root = read_json_object(path)?;
    let hooks = ensure_object_field(&mut root, "hooks")?;
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
        let home_dir = root.join("home");
        fs::create_dir_all(&repo_dir).unwrap();
        fs::create_dir_all(&home_dir).unwrap();
        init_git_repo(&repo_dir);

        write_text(
            &repo_dir.join(CODEX_CONFIG),
            "[features]\nother_flag = true\ncodex_hooks = false\n",
        )
        .unwrap();

        let outcome = join_repo(JoinConfig {
            server_url: "http://127.0.0.1:8787/".to_owned(),
            app_url: "https://app.supermanager.test/".to_owned(),
            room_id: "bright-fox".to_owned(),
            repo_dir: repo_dir.clone(),
            home_dir: home_dir.clone(),
        })
        .unwrap();

        let codex_config = fs::read_to_string(repo_dir.join(CODEX_CONFIG)).unwrap();
        assert!(codex_config.contains("[features]"));
        assert!(codex_config.contains("other_flag = true"));
        assert!(codex_config.contains("codex_hooks = true"));
        assert!(!codex_config.contains("codex_hooks = false"));

        let hooks = fs::read_to_string(repo_dir.join(CODEX_HOOKS_JSON)).unwrap();
        assert!(hooks.contains(CODEX_HOOK_COMMAND));
        assert_eq!(
            outcome.dashboard_url,
            "https://app.supermanager.test/r/BRIGHT-FOX"
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn join_repo_uses_repo_root_for_nested_paths() {
        let root = test_dir("join-repo-nested");
        let repo_dir = root.join("repo");
        let nested_dir = repo_dir.join("nested");
        let home_dir = root.join("home");
        fs::create_dir_all(&nested_dir).unwrap();
        fs::create_dir_all(&home_dir).unwrap();
        init_git_repo(&repo_dir);

        let outcome = join_repo(JoinConfig {
            server_url: "http://127.0.0.1:8787/".to_owned(),
            app_url: "https://app.supermanager.test/".to_owned(),
            room_id: "bright-fox".to_owned(),
            repo_dir: nested_dir.clone(),
            home_dir: home_dir.clone(),
        })
        .unwrap();

        assert_eq!(outcome.repo_dir, canonicalize_best_effort(&repo_dir));
        assert!(
            get_repo_room_config(&home_dir, &nested_dir)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            get_repo_room_config(&home_dir, &repo_dir)
                .unwrap()
                .unwrap()
                .room_id,
            "BRIGHT-FOX"
        );

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
                server_url: "https://api.supermanager.dev".to_owned(),
                room_id: "ALPHA1".to_owned(),
            },
        );
        config.repos.insert(
            repo_key(&repo_a),
            RepoRoomConfig {
                server_url: "https://api.supermanager.dev".to_owned(),
                room_id: "ALPHA1".to_owned(),
            },
        );
        config.repos.insert(
            repo_key(&repo_c),
            RepoRoomConfig {
                server_url: "http://127.0.0.1:8787".to_owned(),
                room_id: "BETA22".to_owned(),
            },
        );

        write_home_repo_config(&home_repo_config_path(&home_dir), &config).unwrap();

        let outcome = list_rooms(&home_dir).unwrap();

        assert_eq!(outcome.rooms.len(), 2);
        assert_eq!(outcome.rooms[0].room_id, "ALPHA1");
        assert_eq!(outcome.rooms[0].server_url, "https://api.supermanager.dev");
        assert_eq!(
            outcome.rooms[0].repo_dirs,
            vec![
                canonicalize_best_effort(&repo_a),
                canonicalize_best_effort(&repo_b)
            ]
        );
        assert_eq!(outcome.rooms[1].room_id, "BETA22");
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
