use std::{
    fs,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use reqwest::blocking::Client;
use serde_json::{Map, Value};

pub(crate) const MANAGED_TOML_START: &str = "# supermanager:start";
pub(crate) const MANAGED_TOML_END: &str = "# supermanager:end";

pub(crate) const CLAUDE_SETTINGS_LOCAL: &str = ".claude/settings.local.json";
pub(crate) const CLAUDE_HOOK_COMMAND: &str = "supermanager hook-report --client claude";
pub(crate) const CODEX_CONFIG: &str = ".codex/config.toml";
pub(crate) const CODEX_HOOKS_JSON: &str = ".codex/hooks.json";
pub(crate) const CODEX_HOOK_COMMAND: &str = "supermanager hook-report --client codex";
pub(crate) const CODEX_MEMORY_EXTENSION: &str = ".codex/memories_extensions/supermanager";
pub(crate) const CONTEXT_SYNC_HOOK_COMMAND: &str = "supermanager hook-sync-context";

pub(crate) const HOME_AUTH_STATE: &str = ".supermanager/auth.json";
pub(crate) const HOME_REPO_CONFIG: &str = ".supermanager/repos.json";
pub(crate) const DEVICE_CLIENT_ID: &str = "supermanager-cli";
pub(crate) const DEVICE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";
pub(crate) const DEVICE_SCOPE: &str = "openid profile email";
pub(crate) const HOOK_TIMEOUT_SECONDS: u64 = 10;
pub(crate) const REPORT_TIMEOUT_SECONDS: u64 = 5;
pub(crate) const API_TIMEOUT_SECONDS: u64 = 10;
pub(crate) const CONTEXT_SYNC_STALE_AFTER_SECONDS: u64 = 6 * 60 * 60;

pub const DEFAULT_SERVER_URL: &str = "https://api.supermanager.dev";

pub(crate) fn build_http_client(timeout_seconds: u64) -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(timeout_seconds))
        .build()
        .context("failed to build HTTP client")
}

pub(crate) fn ensure_success(
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

pub(crate) fn normalize_url(url: &str) -> String {
    url.trim_end_matches('/').to_owned()
}

pub(crate) fn is_interactive_terminal() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

pub(crate) fn ensure_interactive_terminal(command: &str) -> Result<()> {
    if is_interactive_terminal() {
        return Ok(());
    }

    bail!("{command} requires an interactive terminal");
}

pub(crate) fn open_url(url: &str) -> Result<()> {
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

pub(crate) fn path_basename(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
}

pub(crate) fn canonicalize_best_effort(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

pub(crate) fn run_clipboard_command(program: &str, args: &[&str], text: &str) -> Result<()> {
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

pub(crate) fn read_optional_text(path: &Path) -> Result<String> {
    if !path.exists() {
        return Ok(String::new());
    }
    fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))
}

pub(crate) fn write_text(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, text).with_context(|| format!("failed to write {}", path.display()))
}

pub(crate) fn write_private_text(path: &Path, text: &str) -> Result<()> {
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

pub(crate) fn read_json_object(path: &Path) -> Result<Map<String, Value>> {
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

pub(crate) fn write_json_object(path: &Path, root: &Map<String, Value>) -> Result<()> {
    let value = Value::Object(root.clone());
    let text = serde_json::to_string_pretty(&value)
        .with_context(|| format!("failed to serialize JSON for {}", path.display()))?;
    write_text(path, &(text + "\n"))
}

pub(crate) fn ensure_object_field<'a>(
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
