use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, anyhow, bail};
use reporter_protocol::SUPERMANAGER_INSTRUCTIONS_TEMPLATE;
use serde_json::{Map, Value, json};

const MANAGED_START: &str = "<!-- supermanager:start -->";
const MANAGED_END: &str = "<!-- supermanager:end -->";
const CLAUDE_APPROVAL: &str = "mcp__supermanager__submit_progress";

pub struct JoinConfig {
    pub server_url: String,
    pub room_id: String,
    pub secret: String,
    pub repo_dir: PathBuf,
    pub home_dir: PathBuf,
}

pub struct JoinOutcome {
    pub room_id: String,
    pub employee_name: String,
    pub dashboard_url: String,
    pub repo_dir: PathBuf,
}

pub struct LeaveOutcome {
    pub repo_dir: PathBuf,
    pub removed_paths: Vec<String>,
}

pub fn resolve_home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("HOME is not set"))
}

pub fn join_repo(config: JoinConfig) -> Result<JoinOutcome> {
    let repo_dir = config.repo_dir;
    if !repo_dir.exists() {
        bail!("repo path does not exist: {}", repo_dir.display());
    }

    let employee_name = detect_employee_name(&repo_dir)?;
    let base_url = normalize_base_url(&config.server_url);
    let mcp_url = format!(
        "{}/r/{}/mcp?secret={}",
        base_url, config.room_id, config.secret
    );
    let dashboard_url = format!("{}/r/{}", base_url, config.room_id);

    remove_global_claude_mcp();

    upsert_mcp_json(&repo_dir.join(".mcp.json"), &mcp_url)?;
    upsert_claude_settings(&config.home_dir.join(".claude/settings.json"))?;
    upsert_codex_config(&repo_dir.join(".codex/config.toml"), &mcp_url)?;
    upsert_mcp_json(&repo_dir.join(".codex-mcp.json"), &mcp_url)?;
    remove_codex_config(&config.home_dir.join(".codex/config.toml"))?;

    let instructions = render_instructions(&employee_name);
    for name in ["CLAUDE.md", "AGENTS.md"] {
        upsert_instruction_file(&repo_dir.join(name), &instructions)?;
    }

    Ok(JoinOutcome {
        room_id: config.room_id,
        employee_name,
        dashboard_url,
        repo_dir,
    })
}

pub fn leave_repo(repo_dir: &Path, home_dir: &Path) -> Result<LeaveOutcome> {
    if !repo_dir.exists() {
        bail!("repo path does not exist: {}", repo_dir.display());
    }

    remove_global_claude_mcp();

    let mut removed_paths = Vec::new();

    if remove_mcp_json_entry(&repo_dir.join(".mcp.json"))? {
        removed_paths.push(".mcp.json".to_owned());
    }
    if remove_claude_approval(&home_dir.join(".claude/settings.json"))? {
        removed_paths.push("$HOME/.claude/settings.json".to_owned());
    }
    if remove_codex_config(&repo_dir.join(".codex/config.toml"))? {
        removed_paths.push(".codex/config.toml".to_owned());
    }
    if remove_mcp_json_entry(&repo_dir.join(".codex-mcp.json"))? {
        removed_paths.push(".codex-mcp.json".to_owned());
    }
    for name in ["CLAUDE.md", "AGENTS.md"] {
        if remove_instruction_block(&repo_dir.join(name))? {
            removed_paths.push(name.to_owned());
        }
    }

    if removed_paths.is_empty() {
        removed_paths.push("nothing to remove".to_owned());
    }

    Ok(LeaveOutcome {
        repo_dir: repo_dir.to_path_buf(),
        removed_paths,
    })
}

fn detect_employee_name(repo_dir: &Path) -> Result<String> {
    if let Some(name) = git_config_value(repo_dir, &["config", "user.name"])? {
        return Ok(name);
    }
    if let Some(name) = git_config_value(repo_dir, &["config", "--global", "user.name"])? {
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
    if let Ok(output) = whoami {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if !text.is_empty() {
                return Ok(text);
            }
        }
    }

    bail!("could not detect employee name; set git user.name first")
}

fn git_config_value(repo_dir: &Path, args: &[&str]) -> Result<Option<String>> {
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

fn normalize_base_url(url: &str) -> String {
    url.trim_end_matches('/').to_owned()
}

fn render_instructions(employee_name: &str) -> String {
    SUPERMANAGER_INSTRUCTIONS_TEMPLATE.replace("SUPERMANAGER_EMPLOYEE_NAME", employee_name)
}

fn remove_global_claude_mcp() {
    let _ = Command::new("claude")
        .args(["mcp", "remove", "supermanager"])
        .output();
}

fn upsert_mcp_json(path: &Path, url: &str) -> Result<()> {
    let mut root = read_json_object(path)?;
    let server = json!({
        "type": "http",
        "url": url,
    });
    let servers = ensure_object_field(&mut root, "mcpServers")?;
    servers.insert("supermanager".to_owned(), server);
    write_json_object(path, &root)
}

fn remove_mcp_json_entry(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }

    let mut root = read_json_object(path)?;
    let Some(servers_value) = root.get_mut("mcpServers") else {
        return Ok(false);
    };
    let Some(servers) = servers_value.as_object_mut() else {
        bail!("{} has a non-object mcpServers field", path.display());
    };

    let removed = servers.remove("supermanager").is_some();
    if !removed {
        return Ok(false);
    }

    if servers.is_empty() {
        root.remove("mcpServers");
    }

    if root.is_empty() {
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))?;
    } else {
        write_json_object(path, &root)?;
    }

    Ok(true)
}

fn upsert_claude_settings(path: &Path) -> Result<()> {
    let mut root = read_json_object(path)?;
    let permissions = ensure_object_field(&mut root, "permissions")?;
    let allow = permissions
        .entry("allow".to_owned())
        .or_insert_with(|| Value::Array(Vec::new()));
    let allow = allow
        .as_array_mut()
        .ok_or_else(|| anyhow!("{} has a non-array permissions.allow field", path.display()))?;

    let exists = allow
        .iter()
        .any(|entry| entry.as_str() == Some(CLAUDE_APPROVAL));
    if !exists {
        allow.push(Value::String(CLAUDE_APPROVAL.to_owned()));
    }

    write_json_object(path, &root)
}

fn remove_claude_approval(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }

    let mut root = read_json_object(path)?;
    let mut removed = false;

    if let Some(permissions) = root.get_mut("permissions").and_then(Value::as_object_mut) {
        if let Some(allow) = permissions.get_mut("allow").and_then(Value::as_array_mut) {
            let before = allow.len();
            allow.retain(|entry| {
                let keep = entry
                    .as_str()
                    .map(|value| !value.contains("supermanager"))
                    .unwrap_or(true);
                if !keep {
                    removed = true;
                }
                keep
            });

            if allow.is_empty() {
                permissions.remove("allow");
            }
            if before == 0 {
                removed = false;
            }
        }
        if permissions.is_empty() {
            root.remove("permissions");
        }
    }

    if removed {
        if root.is_empty() {
            fs::remove_file(path)
                .with_context(|| format!("failed to remove {}", path.display()))?;
        } else {
            write_json_object(path, &root)?;
        }
    }

    Ok(removed)
}

fn upsert_codex_config(path: &Path, url: &str) -> Result<()> {
    let existing = read_optional_text(path)?;
    let stripped = strip_managed_toml_sections(&existing);
    let mut next = stripped.trim_end().to_owned();
    if !next.is_empty() {
        next.push_str("\n\n");
    }
    next.push_str("[mcp_servers.supermanager]\n");
    next.push_str(&format!("url = \"{}\"\n\n", escape_toml_string(url)));
    next.push_str("[mcp_servers.supermanager.tools.submit_progress]\n");
    next.push_str("approval_mode = \"approve\"\n");
    write_text(path, &next)
}

fn remove_codex_config(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }

    let existing = read_optional_text(path)?;
    let stripped = strip_managed_toml_sections(&existing);
    let trimmed = stripped.trim();
    if trimmed.is_empty() {
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))?;
    } else {
        write_text(path, &format!("{trimmed}\n"))?;
    }
    Ok(existing != stripped)
}

fn strip_managed_toml_sections(text: &str) -> String {
    let mut out = Vec::new();
    let mut skip = false;

    for line in text.lines() {
        let trimmed = line.trim();
        if is_section_header(trimmed) {
            skip = matches!(
                trimmed,
                "[mcp_servers.supermanager]" | "[mcp_servers.supermanager.tools.submit_progress]"
            );
            if skip {
                continue;
            }
        }

        if !skip {
            out.push(line);
        }
    }

    out.join("\n")
}

fn is_section_header(line: &str) -> bool {
    line.starts_with('[') && line.ends_with(']')
}

fn upsert_instruction_file(path: &Path, rendered: &str) -> Result<()> {
    let existing = read_optional_text(path)?;
    let updated = replace_or_append_block(&existing, rendered)?;
    write_text(path, &updated)
}

fn remove_instruction_block(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }

    let existing = read_optional_text(path)?;
    let Some((start, end)) = find_managed_block(&existing) else {
        return Ok(false);
    };

    let mut updated = String::with_capacity(existing.len());
    updated.push_str(existing[..start].trim_end());
    if !updated.is_empty() {
        updated.push('\n');
    }
    updated.push_str(existing[end..].trim_start());

    if !updated.ends_with('\n') {
        updated.push('\n');
    }

    write_text(path, &updated)?;
    Ok(true)
}

fn replace_or_append_block(existing: &str, rendered: &str) -> Result<String> {
    if let Some((start, end)) = find_managed_block(existing) {
        let mut updated = String::with_capacity(existing.len() + rendered.len());
        updated.push_str(&existing[..start]);
        updated.push_str(rendered);
        updated.push_str(&existing[end..]);
        if !updated.ends_with('\n') {
            updated.push('\n');
        }
        return Ok(updated);
    }

    let mut updated = existing.trim_end().to_owned();
    if !updated.is_empty() {
        updated.push_str("\n\n");
    }
    updated.push_str(rendered);
    if !updated.ends_with('\n') {
        updated.push('\n');
    }
    Ok(updated)
}

fn find_managed_block(text: &str) -> Option<(usize, usize)> {
    let start = text.find(MANAGED_START)?;
    let end_marker = text[start..].find(MANAGED_END)?;
    let mut end = start + end_marker + MANAGED_END.len();
    if text[end..].starts_with('\n') {
        end += 1;
    }
    Some((start, end))
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

fn escape_toml_string(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use tempfile::tempdir;

    #[test]
    fn join_and_leave_manage_repo_files() {
        let temp = tempdir().unwrap();
        let repo_dir = temp.path().join("repo");
        let home_dir = temp.path().join("home");
        fs::create_dir_all(&repo_dir).unwrap();
        fs::create_dir_all(&home_dir).unwrap();

        write_text(
            &repo_dir.join(".codex/config.toml"),
            "[existing]\nvalue = \"keep\"\n",
        )
        .unwrap();
        write_text(&repo_dir.join("AGENTS.md"), "# Repo Instructions\n").unwrap();
        write_text(
            &home_dir.join(".claude/settings.json"),
            "{\n  \"permissions\": {\n    \"allow\": [\"other\"]\n  }\n}\n",
        )
        .unwrap();

        join_repo(JoinConfig {
            server_url: "http://127.0.0.1:8787/".to_owned(),
            room_id: "bright-fox".to_owned(),
            secret: "secret123".to_owned(),
            repo_dir: repo_dir.clone(),
            home_dir: home_dir.clone(),
        })
        .unwrap();

        let mcp = fs::read_to_string(repo_dir.join(".mcp.json")).unwrap();
        assert!(mcp.contains("\"supermanager\""));
        assert!(mcp.contains("/r/bright-fox/mcp?secret=secret123"));

        let codex = fs::read_to_string(repo_dir.join(".codex/config.toml")).unwrap();
        assert!(codex.contains("[existing]"));
        assert!(codex.contains("[mcp_servers.supermanager]"));
        assert!(codex.contains("approval_mode = \"approve\""));

        let agents = fs::read_to_string(repo_dir.join("AGENTS.md")).unwrap();
        assert!(agents.contains("**Employee: "));
        assert!(agents.contains(MANAGED_START));

        let claude_settings = fs::read_to_string(home_dir.join(".claude/settings.json")).unwrap();
        assert!(claude_settings.contains(CLAUDE_APPROVAL));
        assert!(claude_settings.contains("\"other\""));

        let outcome = leave_repo(&repo_dir, &home_dir).unwrap();
        assert!(!outcome.removed_paths.is_empty());

        let codex_after = fs::read_to_string(repo_dir.join(".codex/config.toml")).unwrap();
        assert!(!codex_after.contains("[mcp_servers.supermanager]"));
        assert!(codex_after.contains("[existing]"));

        let agents_after = fs::read_to_string(repo_dir.join("AGENTS.md")).unwrap();
        assert!(!agents_after.contains(MANAGED_START));

        let claude_after = fs::read_to_string(home_dir.join(".claude/settings.json")).unwrap();
        assert!(!claude_after.contains(CLAUDE_APPROVAL));
        assert!(claude_after.contains("\"other\""));
    }

    #[test]
    fn replace_or_append_updates_existing_block() {
        let original =
            "header\n\n<!-- supermanager:start -->old<!-- supermanager:end -->\nfooter\n";
        let updated = replace_or_append_block(
            original,
            "<!-- supermanager:start -->new<!-- supermanager:end -->\n",
        )
        .unwrap();
        assert_eq!(
            updated,
            "header\n\n<!-- supermanager:start -->new<!-- supermanager:end -->\nfooter\n"
        );
    }
}
