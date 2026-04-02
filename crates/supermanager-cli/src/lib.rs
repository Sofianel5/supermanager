use std::{
    collections::BTreeMap,
    env, fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
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

const MANAGED_TOML_START: &str = "# supermanager:start";
const MANAGED_TOML_END: &str = "# supermanager:end";

const CLAUDE_SETTINGS_LOCAL: &str = ".claude/settings.local.json";
const CLAUDE_HOOK_COMMAND: &str = "supermanager hook-report --client claude";
const CODEX_CONFIG: &str = ".codex/config.toml";
const CODEX_HOOKS_JSON: &str = ".codex/hooks.json";
const CODEX_HOOK_COMMAND: &str = "supermanager hook-report --client codex";

const HOME_REPO_CONFIG: &str = ".supermanager/repos.json";
const HOOK_TIMEOUT_SECONDS: u64 = 10;
const REPORT_TIMEOUT_SECONDS: u64 = 5;
const API_TIMEOUT_SECONDS: u64 = 10;

pub const DEFAULT_SERVER_URL: &str = "https://supermanager.fly.dev";
pub const DEFAULT_APP_URL: &str = "https://supermanager.pages.dev";

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
}

pub struct CreateRoomOutcome {
    pub room_id: String,
    pub room_name: String,
    pub dashboard_url: String,
    pub join_command: String,
}

pub struct LeaveOutcome {
    pub repo_dir: PathBuf,
    pub removed_paths: Vec<String>,
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

pub fn resolve_home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("HOME is not set"))
}

pub fn create_room(config: CreateRoomConfig) -> Result<CreateRoomOutcome> {
    let server_url = normalize_url(&config.server_url);
    let room_name = config
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| default_room_name(&config.cwd));
    let http = build_http_client(API_TIMEOUT_SECONDS)?;

    let response = http
        .post(format!("{server_url}/v1/rooms"))
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
    })
}

pub fn join_repo(config: JoinConfig) -> Result<JoinOutcome> {
    let repo_dir = canonicalize_best_effort(&config.repo_dir);
    if !repo_dir.exists() {
        bail!("repo path does not exist: {}", repo_dir.display());
    }

    let employee_name = detect_employee_name(&repo_dir)?;
    let server_url = normalize_url(&config.server_url);
    let app_url = normalize_url(&config.app_url);
    let dashboard_url = format!("{}/r/{}", app_url, config.room_id);

    let room_config = RepoRoomConfig {
        server_url,
        room_id: config.room_id.clone(),
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
        room_id: config.room_id,
        employee_name,
        dashboard_url,
        repo_dir,
    })
}

pub fn get_room(server_url: &str, room_id: &str) -> Result<RoomMetadataResponse> {
    let server_url = normalize_url(server_url);
    let http = build_http_client(API_TIMEOUT_SECONDS)?;
    let response = http
        .get(format!("{server_url}/r/{room_id}"))
        .send()
        .with_context(|| format!("failed to fetch room {room_id}"))?;

    let response = ensure_success(response, "get room")?;
    response
        .json()
        .context("failed to parse room response JSON")
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

    let response = http
        .post(url)
        .json(&report)
        .send()
        .context("failed to post hook turn report")?;

    if !response.status().is_success() {
        bail!("hook turn report returned {}", response.status());
    }

    Ok(())
}

fn default_room_name(cwd: &Path) -> String {
    let repo_root = resolve_repo_root(cwd);
    if let Some(name) = path_basename(&repo_root) {
        return name;
    }
    if let Some(name) = path_basename(cwd) {
        return name;
    }
    "supermanager room".to_owned()
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
    let repo_dir = resolve_repo_root(&cwd);
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

fn resolve_repo_root(cwd: &Path) -> PathBuf {
    if let Ok(Some(root)) = git_command_value(cwd, &["rev-parse", "--show-toplevel"]) {
        return canonicalize_best_effort(Path::new(&root));
    }
    canonicalize_best_effort(cwd)
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
            "https://app.supermanager.test/r/bright-fox"
        );

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

        assert_eq!(default_room_name(&nested_dir), "repo-name");

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
