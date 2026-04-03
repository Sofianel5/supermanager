use std::{
    env, fs,
    io::Cursor,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail};
use flate2::read::GzDecoder;
use reqwest::header::{ACCEPT, USER_AGENT};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tar::Archive;
use tempfile::TempDir;

use crate::{API_TIMEOUT_SECONDS, build_http_client, read_optional_text, write_text};

const DEFAULT_RELEASE_REPO: &str = "Sofianel5/supermanager";
const INSTALL_REPO_ENV: &str = "SUPERMANAGER_INSTALL_REPO";
const INSTALL_VERSION_ENV: &str = "SUPERMANAGER_INSTALL_VERSION";
const AUTO_UPDATE_ENV: &str = "SUPERMANAGER_AUTO_UPDATE";

const BIN_NAME: &str = "supermanager";
const CHECKSUM_FILE: &str = "supermanager-checksums.txt";
const UPDATE_STATE_FILE: &str = ".supermanager/update-state.json";
const AUTO_UPDATE_INTERVAL_SECONDS: u64 = 60 * 60 * 24;
const USER_AGENT_VALUE: &str = concat!("supermanager/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelfUpdateOutcome {
    Updated {
        previous_version: String,
        current_version: String,
    },
    UpdateAvailable {
        current_version: String,
        latest_version: String,
    },
    AlreadyCurrent {
        version: String,
    },
    Unsupported {
        reason: String,
    },
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct UpdateState {
    last_checked_at: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubReleaseAsset {
    name: String,
    browser_download_url: String,
}

pub fn maybe_auto_update(home_dir: &Path) -> Result<Option<SelfUpdateOutcome>> {
    if auto_update_disabled() {
        return Ok(None);
    }

    if !auto_update_due(home_dir) {
        return Ok(None);
    }

    let result = run_self_update(false);
    write_update_state(
        home_dir,
        &UpdateState {
            last_checked_at: Some(now_unix_timestamp()?),
        },
    )?;

    match result? {
        outcome @ SelfUpdateOutcome::Updated { .. } => Ok(Some(outcome)),
        SelfUpdateOutcome::Unsupported { .. }
        | SelfUpdateOutcome::UpdateAvailable { .. }
        | SelfUpdateOutcome::AlreadyCurrent { .. } => Ok(None),
    }
}

pub fn run_self_update(check_only: bool) -> Result<SelfUpdateOutcome> {
    let executable = env::current_exe().context("failed to resolve the current executable")?;

    if let Some(reason) = unsupported_update_reason(&executable) {
        return Ok(SelfUpdateOutcome::Unsupported { reason });
    }

    let target = release_target().ok_or_else(|| {
        anyhow!(
            "unsupported platform: {}-{}",
            env::consts::ARCH,
            env::consts::OS
        )
    })?;
    let current_version = current_version()?;
    let release = fetch_release()?;
    let latest_version = parse_version(&release.tag_name)?;

    if latest_version <= current_version {
        return Ok(SelfUpdateOutcome::AlreadyCurrent {
            version: current_version.to_string(),
        });
    }

    if check_only {
        return Ok(SelfUpdateOutcome::UpdateAvailable {
            current_version: current_version.to_string(),
            latest_version: latest_version.to_string(),
        });
    }

    install_release(&release, target, &executable)?;

    Ok(SelfUpdateOutcome::Updated {
        previous_version: current_version.to_string(),
        current_version: latest_version.to_string(),
    })
}

fn auto_update_disabled() -> bool {
    env::var(AUTO_UPDATE_ENV)
        .map(|value| matches_falsey(&value))
        .unwrap_or(false)
}

fn matches_falsey(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "0" | "false" | "no" | "off"
    )
}

fn auto_update_due(home_dir: &Path) -> bool {
    let state = read_update_state(home_dir);
    let now = match now_unix_timestamp() {
        Ok(now) => now,
        Err(_) => return true,
    };

    match state.last_checked_at {
        Some(timestamp) => now.saturating_sub(timestamp) >= AUTO_UPDATE_INTERVAL_SECONDS,
        None => true,
    }
}

fn read_update_state(home_dir: &Path) -> UpdateState {
    let path = update_state_path(home_dir);
    let Ok(text) = read_optional_text(&path) else {
        return UpdateState::default();
    };
    if text.trim().is_empty() {
        return UpdateState::default();
    }

    serde_json::from_str(&text).unwrap_or_default()
}

fn write_update_state(home_dir: &Path, state: &UpdateState) -> Result<()> {
    let text = serde_json::to_string_pretty(state).context("failed to serialize update state")?;
    write_text(&update_state_path(home_dir), &(text + "\n"))
}

fn update_state_path(home_dir: &Path) -> PathBuf {
    home_dir.join(UPDATE_STATE_FILE)
}

fn unsupported_update_reason(executable: &Path) -> Option<String> {
    if release_target().is_none() {
        return Some(format!(
            "self-update is not published for {}-{}",
            env::consts::ARCH,
            env::consts::OS
        ));
    }

    if is_workspace_target_build(executable) {
        return Some(
            "self-update is disabled for workspace builds; install the released CLI instead"
                .to_owned(),
        );
    }

    None
}

fn is_workspace_target_build(path: &Path) -> bool {
    let mut saw_target = false;

    for component in path.components() {
        let component = component.as_os_str().to_string_lossy();

        if component == "target" {
            saw_target = true;
            continue;
        }

        if saw_target && matches!(component.as_ref(), "debug" | "release") {
            return true;
        }
    }

    false
}

fn release_target() -> Option<&'static str> {
    match (env::consts::ARCH, env::consts::OS) {
        ("aarch64", "macos") => Some("aarch64-apple-darwin"),
        ("x86_64", "macos") => Some("x86_64-apple-darwin"),
        ("x86_64", "linux") => Some("x86_64-unknown-linux-gnu"),
        _ => None,
    }
}

fn current_version() -> Result<Version> {
    parse_version(env!("CARGO_PKG_VERSION"))
}

fn fetch_release() -> Result<GitHubRelease> {
    let repo = install_repo();
    let requested_version = install_version();
    let url = if requested_version == "latest" {
        format!("https://api.github.com/repos/{repo}/releases/latest")
    } else {
        format!("https://api.github.com/repos/{repo}/releases/tags/{requested_version}")
    };

    let http = build_http_client(API_TIMEOUT_SECONDS)?;
    let response = http
        .get(url)
        .header(USER_AGENT, USER_AGENT_VALUE)
        .header(ACCEPT, "application/vnd.github+json")
        .send()
        .context("failed to fetch CLI release metadata")?;

    let response = crate::ensure_success(response, "fetch CLI release metadata")?;
    response
        .json()
        .context("failed to parse CLI release metadata")
}

fn install_repo() -> String {
    env::var(INSTALL_REPO_ENV)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_RELEASE_REPO.to_owned())
}

fn install_version() -> String {
    env::var(INSTALL_VERSION_ENV)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "latest".to_owned())
}

fn install_release(release: &GitHubRelease, target: &str, executable: &Path) -> Result<()> {
    let archive_name = format!("{BIN_NAME}-{target}.tar.gz");
    let archive_asset = find_asset(release, &archive_name)?;
    let checksum_asset = find_asset(release, CHECKSUM_FILE)?;
    let archive_bytes = download_bytes(&archive_asset.browser_download_url)?;
    let checksum_bytes = download_bytes(&checksum_asset.browser_download_url)?;
    let checksum_text =
        String::from_utf8(checksum_bytes).context("release checksum asset was not valid UTF-8")?;

    verify_checksum(&archive_bytes, &checksum_text, &archive_name)?;

    let temp_dir = TempDir::new().context("failed to create a temporary update directory")?;
    let extracted_binary = extract_archive(&archive_bytes, temp_dir.path())?;
    replace_executable(executable, &extracted_binary)?;
    Ok(())
}

fn find_asset<'a>(release: &'a GitHubRelease, name: &str) -> Result<&'a GitHubReleaseAsset> {
    release
        .assets
        .iter()
        .find(|asset| asset.name == name)
        .ok_or_else(|| anyhow!("release asset not found: {name}"))
}

fn download_bytes(url: &str) -> Result<Vec<u8>> {
    let http = build_http_client(API_TIMEOUT_SECONDS)?;
    let response = http
        .get(url)
        .header(USER_AGENT, USER_AGENT_VALUE)
        .send()
        .with_context(|| format!("failed to download {url}"))?;

    let response = crate::ensure_success(response, "download CLI release asset")?;
    let bytes = response
        .bytes()
        .with_context(|| format!("failed to read response body from {url}"))?;
    Ok(bytes.to_vec())
}

fn verify_checksum(archive_bytes: &[u8], checksum_text: &str, archive_name: &str) -> Result<()> {
    let expected = checksum_for_asset(checksum_text, archive_name)?;
    let actual = format!("{:x}", Sha256::digest(archive_bytes));

    if actual == expected {
        return Ok(());
    }

    bail!("checksum mismatch for {archive_name}")
}

fn checksum_for_asset(checksum_text: &str, archive_name: &str) -> Result<String> {
    checksum_text
        .lines()
        .find_map(|line| {
            let mut parts = line.split_whitespace();
            let hash = parts.next()?;
            let name = parts.next()?.trim_start_matches('*');
            (name == archive_name).then(|| hash.to_owned())
        })
        .ok_or_else(|| anyhow!("missing checksum for {archive_name}"))
}

fn extract_archive(archive_bytes: &[u8], output_dir: &Path) -> Result<PathBuf> {
    let decoder = GzDecoder::new(Cursor::new(archive_bytes));
    let mut archive = Archive::new(decoder);
    archive
        .unpack(output_dir)
        .context("failed to unpack CLI release archive")?;

    let binary_path = output_dir.join(BIN_NAME);
    if binary_path.is_file() {
        Ok(binary_path)
    } else {
        bail!("release archive did not contain {}", binary_path.display())
    }
}

fn replace_executable(executable: &Path, new_binary: &Path) -> Result<()> {
    let parent = executable
        .parent()
        .ok_or_else(|| anyhow!("current executable has no parent directory"))?;
    let staged_path = parent.join(format!(".{BIN_NAME}.update-{}", std::process::id()));

    if staged_path.exists() {
        fs::remove_file(&staged_path)
            .with_context(|| format!("failed to remove {}", staged_path.display()))?;
    }

    fs::copy(new_binary, &staged_path)
        .with_context(|| format!("failed to stage {}", staged_path.display()))?;

    #[cfg(unix)]
    {
        let permissions = fs::metadata(new_binary)
            .with_context(|| format!("failed to read {}", new_binary.display()))?
            .permissions();
        fs::set_permissions(&staged_path, permissions)
            .with_context(|| format!("failed to update {}", staged_path.display()))?;
    }

    fs::rename(&staged_path, executable).with_context(|| {
        format!(
            "failed to replace {} with the downloaded release",
            executable.display()
        )
    })?;

    Ok(())
}

fn parse_version(raw: &str) -> Result<Version> {
    Version::parse(raw.trim().trim_start_matches('v'))
        .with_context(|| format!("failed to parse version `{raw}`"))
}

fn now_unix_timestamp() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is set before the Unix epoch")?
        .as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_version_accepts_github_style_tags() {
        let version = parse_version("v1.2.3").unwrap();
        assert_eq!(version, Version::new(1, 2, 3));
    }

    #[test]
    fn checksum_for_asset_reads_sha256sum_lines() {
        let checksum = checksum_for_asset(
            "abc123  supermanager-x86_64-unknown-linux-gnu.tar.gz\n",
            "supermanager-x86_64-unknown-linux-gnu.tar.gz",
        )
        .unwrap();

        assert_eq!(checksum, "abc123");
    }

    #[test]
    fn auto_update_due_respects_throttle_window() {
        let now = 200_000;
        assert!(update_due(None, now));
        assert!(!update_due(
            Some(now - (AUTO_UPDATE_INTERVAL_SECONDS - 1)),
            now
        ));
        assert!(update_due(Some(now - AUTO_UPDATE_INTERVAL_SECONDS), now));
    }

    #[test]
    fn workspace_target_build_detection_matches_cargo_paths() {
        assert!(is_workspace_target_build(Path::new(
            "/tmp/supermanager/target/debug/supermanager"
        )));
        assert!(is_workspace_target_build(Path::new(
            "/tmp/supermanager/target/release/supermanager"
        )));
        assert!(!is_workspace_target_build(Path::new(
            "/usr/local/bin/supermanager"
        )));
    }

    fn update_due(last_checked_at: Option<u64>, now: u64) -> bool {
        match last_checked_at {
            Some(timestamp) => now.saturating_sub(timestamp) >= AUTO_UPDATE_INTERVAL_SECONDS,
            None => true,
        }
    }
}
