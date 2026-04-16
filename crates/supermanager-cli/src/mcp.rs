use std::path::Path;

use anyhow::{Context, Result, anyhow};
use serde_json::{Map, Value, json};
use toml_edit::{DocumentMut, Item, Table, value};

use crate::{
    auth::{auth_state_path, read_auth_state, require_auth_state},
    support::{
        CODEX_CONFIG, ensure_object_field, normalize_url, read_json_object, read_optional_text,
        write_private_text,
    },
    types::{
        AuthState, ConfigFileUpdate, ConfigFileUpdateStatus, InstallMcpConfig, InstallMcpOutcome,
    },
};

const CLAUDE_GLOBAL_CONFIG: &str = ".claude.json";

pub fn install_mcp(config: InstallMcpConfig) -> Result<InstallMcpOutcome> {
    let auth_state =
        resolve_mcp_install_auth_state(&config.home_dir, config.server_url.as_deref())?;
    let server_url = normalize_url(&auth_state.server_url);
    let mcp_url = format!("{server_url}/mcp");
    let claude_config_path = config.home_dir.join(CLAUDE_GLOBAL_CONFIG);
    let codex_config_path = config.home_dir.join(CODEX_CONFIG);

    let claude_status =
        upsert_claude_mcp_config(&claude_config_path, &mcp_url, &auth_state.access_token)?;
    let codex_status =
        upsert_global_codex_mcp_config(&codex_config_path, &mcp_url, &auth_state.access_token)?;

    Ok(InstallMcpOutcome {
        server_url,
        mcp_url,
        file_updates: vec![
            ConfigFileUpdate {
                path: CLAUDE_GLOBAL_CONFIG.to_owned(),
                status: claude_status,
            },
            ConfigFileUpdate {
                path: CODEX_CONFIG.to_owned(),
                status: codex_status,
            },
        ],
    })
}

fn resolve_mcp_install_auth_state(home_dir: &Path, server_url: Option<&str>) -> Result<AuthState> {
    if let Some(server_url) = server_url {
        return require_auth_state(home_dir, server_url);
    }

    read_auth_state(&auth_state_path(home_dir))?
        .ok_or_else(|| anyhow!("not logged in; run `supermanager login` first"))
}

fn upsert_claude_mcp_config(
    path: &Path,
    mcp_url: &str,
    access_token: &str,
) -> Result<ConfigFileUpdateStatus> {
    let existed = path.exists();
    let existing = read_optional_text(path)?;
    let mut root = read_json_object(path)?;
    let mcp_servers = ensure_object_field(&mut root, "mcpServers")?;
    mcp_servers.insert(
        "supermanager".to_owned(),
        json!({
            "type": "http",
            "url": mcp_url,
            "headers": {
                "Authorization": format!("Bearer {access_token}")
            }
        }),
    );
    let next = render_private_json_object(path, &root)?;
    let status = classify_file_update(existed, &existing, &next);
    if status != ConfigFileUpdateStatus::Unchanged {
        write_private_text(path, &next)?;
    }

    Ok(status)
}

fn upsert_global_codex_mcp_config(
    path: &Path,
    mcp_url: &str,
    access_token: &str,
) -> Result<ConfigFileUpdateStatus> {
    let existed = path.exists();
    let existing = read_optional_text(path)?;
    let mut doc = parse_toml_document(&existing, path)?;
    let existing_mcp_servers = doc.as_table_mut().remove("mcp_servers");
    let mut mcp_servers = match existing_mcp_servers {
        Some(item) => item
            .into_table()
            .map_err(|_| anyhow!("{} has a non-table mcp_servers entry", path.display()))?,
        None => Table::new(),
    };

    mcp_servers.set_implicit(false);
    mcp_servers["supermanager"] =
        Item::Table(build_supermanager_codex_mcp_table(mcp_url, access_token));
    doc["mcp_servers"] = Item::Table(mcp_servers);

    let next = normalize_toml_text(doc.to_string());
    let status = classify_file_update(existed, &existing, &next);
    if status != ConfigFileUpdateStatus::Unchanged {
        write_private_text(path, &next)?;
    }

    Ok(status)
}

fn build_supermanager_codex_mcp_table(mcp_url: &str, access_token: &str) -> Table {
    let mut server = Table::new();
    server.set_implicit(false);
    server["url"] = value(mcp_url);

    let mut headers = Table::new();
    headers.set_implicit(false);
    headers["Authorization"] = value(format!("Bearer {access_token}"));
    server["http_headers"] = Item::Table(headers);

    server
}

fn parse_toml_document(text: &str, path: &Path) -> Result<DocumentMut> {
    if text.trim().is_empty() {
        return Ok(DocumentMut::new());
    }

    text.parse::<DocumentMut>()
        .with_context(|| format!("failed to parse TOML in {}", path.display()))
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

fn render_private_json_object(path: &Path, root: &Map<String, Value>) -> Result<String> {
    let value = Value::Object(root.clone());
    serde_json::to_string_pretty(&value)
        .map(|text| text + "\n")
        .with_context(|| format!("failed to serialize JSON for {}", path.display()))
}

fn classify_file_update(existed: bool, existing: &str, next: &str) -> ConfigFileUpdateStatus {
    if existing == next {
        ConfigFileUpdateStatus::Unchanged
    } else if existed && !existing.trim().is_empty() {
        ConfigFileUpdateStatus::Updated
    } else {
        ConfigFileUpdateStatus::Created
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;
    use crate::auth::write_auth_state;

    #[test]
    fn install_mcp_writes_global_configs_from_login_state() {
        let root = test_dir("install-mcp");
        let home_dir = root.join("home");
        fs::create_dir_all(&home_dir).unwrap();

        write_auth_state(
            &auth_state_path(&home_dir),
            &AuthState {
                access_token: "token-123".to_owned(),
                active_org_slug: Some("acme".to_owned()),
                server_url: "http://127.0.0.1:8787/".to_owned(),
            },
        )
        .unwrap();

        let outcome = install_mcp(InstallMcpConfig {
            home_dir: home_dir.clone(),
            server_url: None,
        })
        .unwrap();

        assert_eq!(outcome.server_url, "http://127.0.0.1:8787");
        assert_eq!(outcome.mcp_url, "http://127.0.0.1:8787/mcp");
        assert_eq!(
            outcome
                .file_updates
                .iter()
                .map(|update| (update.path.as_str(), update.status))
                .collect::<Vec<_>>(),
            vec![
                (CLAUDE_GLOBAL_CONFIG, ConfigFileUpdateStatus::Created),
                (CODEX_CONFIG, ConfigFileUpdateStatus::Created),
            ]
        );

        let claude_config: Value =
            serde_json::from_str(&fs::read_to_string(home_dir.join(CLAUDE_GLOBAL_CONFIG)).unwrap())
                .unwrap();
        assert_eq!(
            claude_config["mcpServers"]["supermanager"]["type"],
            Value::String("http".to_owned())
        );
        assert_eq!(
            claude_config["mcpServers"]["supermanager"]["url"],
            Value::String("http://127.0.0.1:8787/mcp".to_owned())
        );
        assert_eq!(
            claude_config["mcpServers"]["supermanager"]["headers"]["Authorization"],
            Value::String("Bearer token-123".to_owned())
        );

        let codex_config = fs::read_to_string(home_dir.join(CODEX_CONFIG)).unwrap();
        assert!(codex_config.contains("[mcp_servers.supermanager]"));
        assert!(codex_config.contains("url = \"http://127.0.0.1:8787/mcp\""));
        assert!(codex_config.contains("[mcp_servers.supermanager.http_headers]"));
        assert!(codex_config.contains("Authorization = \"Bearer token-123\""));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn upsert_claude_mcp_config_preserves_existing_settings() {
        let root = test_dir("claude-mcp-config");
        let config_path = root.join(CLAUDE_GLOBAL_CONFIG);

        write_private_text(
            &config_path,
            "{\n  \"model\": \"opus\",\n  \"mcpServers\": {\n    \"paper\": {\n      \"type\": \"http\",\n      \"url\": \"http://paper.test/mcp\"\n    }\n  }\n}\n",
        )
        .unwrap();

        let status =
            upsert_claude_mcp_config(&config_path, "http://supermanager.test/mcp", "token-123")
                .unwrap();
        assert_eq!(status, ConfigFileUpdateStatus::Updated);

        let config: Value =
            serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
        assert_eq!(config["model"], Value::String("opus".to_owned()));
        assert_eq!(
            config["mcpServers"]["paper"]["url"],
            Value::String("http://paper.test/mcp".to_owned())
        );
        assert_eq!(
            config["mcpServers"]["supermanager"]["url"],
            Value::String("http://supermanager.test/mcp".to_owned())
        );
        assert_eq!(
            config["mcpServers"]["supermanager"]["headers"]["Authorization"],
            Value::String("Bearer token-123".to_owned())
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn upsert_global_codex_mcp_config_preserves_other_servers() {
        let root = test_dir("codex-mcp-config");
        let config_path = root.join(CODEX_CONFIG);

        write_private_text(
            &config_path,
            "[mcp_servers.paper]\nurl = \"http://paper.test/mcp\"\n",
        )
        .unwrap();

        let status = upsert_global_codex_mcp_config(
            &config_path,
            "http://supermanager.test/mcp",
            "token-123",
        )
        .unwrap();
        assert_eq!(status, ConfigFileUpdateStatus::Updated);

        let codex_config = fs::read_to_string(&config_path).unwrap();
        assert!(codex_config.contains("[mcp_servers.paper]"));
        assert!(codex_config.contains("url = \"http://paper.test/mcp\""));
        assert!(codex_config.contains("[mcp_servers.supermanager]"));
        assert!(codex_config.contains("url = \"http://supermanager.test/mcp\""));
        assert!(codex_config.contains("[mcp_servers.supermanager.http_headers]"));
        assert!(codex_config.contains("Authorization = \"Bearer token-123\""));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn install_mcp_reports_unchanged_when_configs_are_current() {
        let root = test_dir("install-mcp-unchanged");
        let home_dir = root.join("home");
        fs::create_dir_all(&home_dir).unwrap();

        write_auth_state(
            &auth_state_path(&home_dir),
            &AuthState {
                access_token: "token-123".to_owned(),
                active_org_slug: Some("acme".to_owned()),
                server_url: "http://127.0.0.1:8787/".to_owned(),
            },
        )
        .unwrap();

        install_mcp(InstallMcpConfig {
            home_dir: home_dir.clone(),
            server_url: None,
        })
        .unwrap();
        let outcome = install_mcp(InstallMcpConfig {
            home_dir: home_dir.clone(),
            server_url: None,
        })
        .unwrap();

        assert_eq!(
            outcome
                .file_updates
                .iter()
                .map(|update| (update.path.as_str(), update.status))
                .collect::<Vec<_>>(),
            vec![
                (CLAUDE_GLOBAL_CONFIG, ConfigFileUpdateStatus::Unchanged),
                (CODEX_CONFIG, ConfigFileUpdateStatus::Unchanged),
            ]
        );

        fs::remove_dir_all(root).unwrap();
    }

    fn test_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "supermanager-{name}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
}
