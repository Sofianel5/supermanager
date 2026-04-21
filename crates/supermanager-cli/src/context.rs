use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result, anyhow, bail};
use reqwest::blocking::Client;
use serde::Deserialize;

use crate::{
    auth::{authed, require_auth_state},
    local::{
        get_repo_project_config, read_hook_payload, resolve_hook_repo_root, resolve_repo_root,
    },
    support::{
        API_TIMEOUT_SECONDS, CODEX_MEMORY_EXTENSION, CONTEXT_SYNC_STALE_AFTER_SECONDS,
        build_http_client, normalize_url, read_optional_text, write_text,
    },
    types::{
        ConfigFileUpdate, ConfigFileUpdateStatus, RepoProjectConfig, SyncContextConfig,
        SyncContextOutcome,
    },
};

const CLAUDE_PROJECT_MEMORY: &str = "CLAUDE.md";
const CLAUDE_IMPORTS_START: &str = "<!-- supermanager:context:start -->";
const CLAUDE_IMPORTS_END: &str = "<!-- supermanager:context:end -->";

#[derive(Debug, Deserialize)]
struct OrganizationAgentContextExportResponse {
    files: Vec<OrganizationAgentContextExportFile>,
}

#[derive(Debug, Deserialize)]
struct OrganizationAgentContextExportFile {
    path: String,
    content: String,
    #[serde(rename = "updated_at")]
    _updated_at: Option<String>,
}

#[derive(Clone, Copy)]
enum RefreshPolicy {
    Always,
    IfStale,
}

#[derive(Debug, Clone)]
struct CachedContextPaths {
    root_dir: PathBuf,
    memories_path: PathBuf,
    skills_path: PathBuf,
}

#[derive(Debug, Clone)]
struct CachedContextEntry {
    file_prefix: String,
    memories_path: PathBuf,
    skills_path: PathBuf,
}

pub fn sync_repo_context(config: SyncContextConfig) -> Result<SyncContextOutcome> {
    let repo_dir = resolve_repo_root(&config.cwd)?;
    let project_config = get_repo_project_config(&config.home_dir, &repo_dir)?.ok_or_else(|| {
        anyhow!(
            "repo {} is not joined to a supermanager project; run `supermanager join <project-id>` first",
            repo_dir.display()
        )
    })?;
    sync_repo_context_for_repo(
        &config.home_dir,
        &repo_dir,
        &project_config,
        RefreshPolicy::Always,
    )
}

pub fn sync_repo_context_from_hook(home_dir: &Path) -> Result<()> {
    let payload = read_hook_payload()?;
    let Some(repo_dir) = resolve_hook_repo_root(&payload)? else {
        return Ok(());
    };
    let Some(project_config) = get_repo_project_config(home_dir, &repo_dir)? else {
        return Ok(());
    };

    match sync_repo_context_for_repo(home_dir, &repo_dir, &project_config, RefreshPolicy::IfStale) {
        Ok(_) => Ok(()),
        Err(error) => {
            let _ = ensure_context_surfaces(home_dir, &repo_dir, &project_config);
            Err(error)
        }
    }
}

pub(crate) fn remove_repo_context(repo_dir: &Path) -> Result<Vec<String>> {
    let mut removed_paths = Vec::new();
    if remove_claude_import_block(&repo_dir.join(CLAUDE_PROJECT_MEMORY))? {
        removed_paths.push(CLAUDE_PROJECT_MEMORY.to_owned());
    }
    Ok(removed_paths)
}

pub(crate) fn prune_cached_context_after_leave(
    home_dir: &Path,
    removed_project_config: Option<&RepoProjectConfig>,
    remaining_configs: &[RepoProjectConfig],
) -> Result<Vec<String>> {
    let Some(removed_project_config) = removed_project_config else {
        return Ok(Vec::new());
    };

    let mut removed_paths = Vec::new();
    let has_matching_repo = remaining_configs.iter().any(|config| {
        normalize_url(&config.server_url) == normalize_url(&removed_project_config.server_url)
            && config.organization_slug == removed_project_config.organization_slug
    });

    if !has_matching_repo {
        let cached_paths = cached_context_paths(home_dir, removed_project_config);
        if cached_paths.root_dir.exists() {
            fs::remove_dir_all(&cached_paths.root_dir)
                .with_context(|| format!("failed to remove {}", cached_paths.root_dir.display()))?;
            removed_paths.push(cached_paths.root_dir.display().to_string());
        }
    }

    removed_paths.extend(rebuild_codex_memory_extension(home_dir)?.removed_paths);
    Ok(removed_paths)
}

fn sync_repo_context_for_repo(
    home_dir: &Path,
    repo_dir: &Path,
    project_config: &RepoProjectConfig,
    refresh_policy: RefreshPolicy,
) -> Result<SyncContextOutcome> {
    let cache_paths = cached_context_paths(home_dir, project_config);
    let mut file_updates = Vec::new();

    if should_refresh(&cache_paths, refresh_policy)? {
        let auth_state = require_auth_state(home_dir, &project_config.server_url)?;
        let http = build_http_client(API_TIMEOUT_SECONDS)?;
        let export = fetch_organization_agent_context_export(
            &http,
            &project_config.server_url,
            &auth_state.access_token,
            &project_config.organization_slug,
        )?;
        file_updates.extend(write_cached_export_files(&cache_paths, &export.files)?);
    }

    file_updates.extend(ensure_context_surfaces(home_dir, repo_dir, project_config)?);

    Ok(SyncContextOutcome {
        organization_slug: project_config.organization_slug.clone(),
        repo_dir: repo_dir.to_path_buf(),
        file_updates,
    })
}

fn ensure_context_surfaces(
    home_dir: &Path,
    repo_dir: &Path,
    project_config: &RepoProjectConfig,
) -> Result<Vec<ConfigFileUpdate>> {
    let cache_paths = cached_context_paths(home_dir, project_config);
    if !cache_paths.memories_path.is_file() || !cache_paths.skills_path.is_file() {
        return Ok(Vec::new());
    }

    let mut file_updates = Vec::new();
    let claude_path = repo_dir.join(CLAUDE_PROJECT_MEMORY);
    let claude_status = upsert_claude_imports(
        &claude_path,
        &[&cache_paths.memories_path, &cache_paths.skills_path],
    )?;
    file_updates.push(ConfigFileUpdate {
        path: claude_path.display().to_string(),
        status: claude_status,
    });

    let codex_updates = rebuild_codex_memory_extension(home_dir)?;
    file_updates.extend(codex_updates.file_updates);

    Ok(file_updates)
}

fn fetch_organization_agent_context_export(
    http: &Client,
    server_url: &str,
    access_token: &str,
    organization_slug: &str,
) -> Result<OrganizationAgentContextExportResponse> {
    let server_url = normalize_url(server_url);
    let response = authed(http, access_token)
        .get(format!(
            "{server_url}/v1/organizations/{organization_slug}/agent-context"
        ))
        .send()
        .with_context(|| {
            format!("failed to fetch exported agent context for organization {organization_slug}")
        })?;
    let response = crate::support::ensure_success(response, "fetch agent context")?;

    response
        .json()
        .context("failed to parse exported agent-context response JSON")
}

fn should_refresh(cache_paths: &CachedContextPaths, refresh_policy: RefreshPolicy) -> Result<bool> {
    match refresh_policy {
        RefreshPolicy::Always => Ok(true),
        RefreshPolicy::IfStale => {
            if !cache_paths.memories_path.is_file() || !cache_paths.skills_path.is_file() {
                return Ok(true);
            }

            let latest_age =
                latest_file_age(&[&cache_paths.memories_path, &cache_paths.skills_path])?;
            Ok(latest_age >= Duration::from_secs(CONTEXT_SYNC_STALE_AFTER_SECONDS))
        }
    }
}

fn latest_file_age(paths: &[&Path]) -> Result<Duration> {
    let now = SystemTime::now();
    let latest_modified = paths
        .iter()
        .map(|path| {
            fs::metadata(path)
                .with_context(|| format!("failed to stat {}", path.display()))?
                .modified()
                .with_context(|| format!("failed to read mtime for {}", path.display()))
        })
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .max()
        .ok_or_else(|| anyhow!("no cache files available"))?;

    Ok(now
        .duration_since(latest_modified)
        .unwrap_or_else(|_| Duration::from_secs(0)))
}

fn write_cached_export_files(
    cache_paths: &CachedContextPaths,
    files: &[OrganizationAgentContextExportFile],
) -> Result<Vec<ConfigFileUpdate>> {
    let mut file_updates = Vec::new();

    for file in files {
        let destination = match file.path.as_str() {
            "memories.md" => &cache_paths.memories_path,
            "skills.md" => &cache_paths.skills_path,
            other => {
                bail!("unknown exported context file: {other}");
            }
        };
        let status = write_text_if_changed(destination, &file.content)?;
        file_updates.push(ConfigFileUpdate {
            path: destination.display().to_string(),
            status,
        });
    }

    Ok(file_updates)
}

struct RebuildCodexOutcome {
    file_updates: Vec<ConfigFileUpdate>,
    removed_paths: Vec<String>,
}

fn rebuild_codex_memory_extension(home_dir: &Path) -> Result<RebuildCodexOutcome> {
    let entries = collect_cached_context_entries(home_dir)?;
    let extension_root = codex_memory_extension_root(home_dir);
    let resources_dir = extension_root.join("resources");

    if entries.is_empty() {
        let mut removed_paths = Vec::new();
        if extension_root.exists() {
            fs::remove_dir_all(&extension_root)
                .with_context(|| format!("failed to remove {}", extension_root.display()))?;
            removed_paths.push(extension_root.display().to_string());
        }
        return Ok(RebuildCodexOutcome {
            file_updates: Vec::new(),
            removed_paths,
        });
    }

    fs::create_dir_all(&resources_dir)
        .with_context(|| format!("failed to create {}", resources_dir.display()))?;

    let mut file_updates = Vec::new();
    let mut expected_files = Vec::new();

    let instructions_path = extension_root.join("instructions.md");
    let instructions_status =
        write_text_if_changed(&instructions_path, &render_codex_extension_instructions())?;
    file_updates.push(ConfigFileUpdate {
        path: instructions_path.display().to_string(),
        status: instructions_status,
    });

    for entry in &entries {
        for (kind, source_path) in [
            ("memories", &entry.memories_path),
            ("skills", &entry.skills_path),
        ] {
            let destination = resources_dir.join(format!("{}--{kind}.md", entry.file_prefix));
            expected_files.push(destination.clone());
            let content = read_optional_text(source_path)?;
            let status = write_text_if_changed(&destination, &content)?;
            file_updates.push(ConfigFileUpdate {
                path: destination.display().to_string(),
                status,
            });
        }
    }

    let mut removed_paths = Vec::new();
    if resources_dir.exists() {
        for entry in fs::read_dir(&resources_dir)
            .with_context(|| format!("failed to read {}", resources_dir.display()))?
        {
            let entry =
                entry.with_context(|| format!("failed to inspect {}", resources_dir.display()))?;
            let path = entry.path();
            if path.is_file() && !expected_files.iter().any(|expected| expected == &path) {
                fs::remove_file(&path)
                    .with_context(|| format!("failed to remove {}", path.display()))?;
                removed_paths.push(path.display().to_string());
            }
        }
    }

    Ok(RebuildCodexOutcome {
        file_updates,
        removed_paths,
    })
}

fn collect_cached_context_entries(home_dir: &Path) -> Result<Vec<CachedContextEntry>> {
    let root = cached_context_root(home_dir);
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();
    for server_entry in
        fs::read_dir(&root).with_context(|| format!("failed to read {}", root.display()))?
    {
        let server_entry =
            server_entry.with_context(|| format!("failed to inspect {}", root.display()))?;
        let server_path = server_entry.path();
        if !server_path.is_dir() {
            continue;
        }

        let Some(server_key) = server_path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };

        for org_entry in fs::read_dir(&server_path)
            .with_context(|| format!("failed to read {}", server_path.display()))?
        {
            let org_entry = org_entry
                .with_context(|| format!("failed to inspect {}", server_path.display()))?;
            let org_path = org_entry.path();
            if !org_path.is_dir() {
                continue;
            }

            let Some(organization_slug) = org_path.file_name().and_then(|name| name.to_str())
            else {
                continue;
            };
            let memories_path = org_path.join("memories.md");
            let skills_path = org_path.join("skills.md");
            if !memories_path.is_file() || !skills_path.is_file() {
                continue;
            }

            entries.push(CachedContextEntry {
                file_prefix: format!("{server_key}--{organization_slug}"),
                memories_path,
                skills_path,
            });
        }
    }

    entries.sort_by(|left, right| left.file_prefix.cmp(&right.file_prefix));
    Ok(entries)
}

fn upsert_claude_imports(path: &Path, imports: &[&Path]) -> Result<ConfigFileUpdateStatus> {
    let existed = path.exists();
    let existing = read_optional_text(path)?;
    let block = render_claude_import_block(imports);
    let next = upsert_managed_markdown_block(&existing, &block);
    if next == existing {
        return Ok(ConfigFileUpdateStatus::Unchanged);
    }

    write_text(path, &next)?;
    Ok(classify_file_update(existed, &existing, &next))
}

fn render_claude_import_block(imports: &[&Path]) -> String {
    let import_lines = imports
        .iter()
        .map(|path| format!("- @{}", path.display()))
        .collect::<Vec<_>>()
        .join("\n");
    [
        CLAUDE_IMPORTS_START,
        "## Supermanager Context",
        import_lines.as_str(),
        CLAUDE_IMPORTS_END,
    ]
    .join("\n")
}

fn upsert_managed_markdown_block(existing: &str, block: &str) -> String {
    if let Some((start, end)) = managed_markdown_block_range(existing) {
        let mut next = String::new();
        next.push_str(existing[..start].trim_end());
        if !next.trim().is_empty() {
            next.push_str("\n\n");
        }
        next.push_str(block);
        let suffix = existing[end..].trim();
        if !suffix.is_empty() {
            next.push_str("\n\n");
            next.push_str(suffix);
        }
        next.push('\n');
        return next;
    }

    if existing.trim().is_empty() {
        return format!("{block}\n");
    }

    format!("{}\n\n{block}\n", existing.trim_end())
}

fn remove_claude_import_block(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }

    let existing = read_optional_text(path)?;
    let Some((start, end)) = managed_markdown_block_range(&existing) else {
        return Ok(false);
    };

    let prefix = existing[..start].trim_end();
    let suffix = existing[end..].trim_start();
    let next = if prefix.is_empty() && suffix.is_empty() {
        String::new()
    } else if prefix.is_empty() {
        format!("{suffix}\n")
    } else if suffix.is_empty() {
        format!("{prefix}\n")
    } else {
        format!("{prefix}\n\n{suffix}\n")
    };

    if next.is_empty() {
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))?;
    } else {
        write_text(path, &next)?;
    }
    Ok(true)
}

fn managed_markdown_block_range(text: &str) -> Option<(usize, usize)> {
    let start = text.find(CLAUDE_IMPORTS_START)?;
    let end_marker = text[start..].find(CLAUDE_IMPORTS_END)?;
    let end = start + end_marker + CLAUDE_IMPORTS_END.len();
    Some((start, end))
}

fn write_text_if_changed(path: &Path, content: &str) -> Result<ConfigFileUpdateStatus> {
    let existed = path.exists();
    let existing = read_optional_text(path)?;
    if existing == content {
        return Ok(ConfigFileUpdateStatus::Unchanged);
    }

    write_text(path, content)?;
    Ok(classify_file_update(existed, &existing, content))
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

fn cached_context_paths(home_dir: &Path, project_config: &RepoProjectConfig) -> CachedContextPaths {
    let server_key = slugify_path_component(&normalize_url(&project_config.server_url));
    let organization_slug = slugify_path_component(&project_config.organization_slug);
    let root_dir = cached_context_root(home_dir)
        .join(&server_key)
        .join(&organization_slug);
    CachedContextPaths {
        memories_path: root_dir.join("memories.md"),
        skills_path: root_dir.join("skills.md"),
        root_dir,
    }
}

fn cached_context_root(home_dir: &Path) -> PathBuf {
    home_dir.join(".supermanager").join("agent-context")
}

fn codex_memory_extension_root(home_dir: &Path) -> PathBuf {
    home_dir.join(CODEX_MEMORY_EXTENSION)
}

fn slugify_path_component(input: &str) -> String {
    let mut output = String::new();
    let mut previous_dash = false;

    for ch in input.chars() {
        let normalized = if ch.is_ascii_alphanumeric() {
            previous_dash = false;
            Some(ch.to_ascii_lowercase())
        } else if previous_dash {
            None
        } else {
            previous_dash = true;
            Some('-')
        };

        if let Some(ch) = normalized {
            output.push(ch);
        }
    }

    let trimmed = output.trim_matches('-').to_owned();
    if trimmed.is_empty() {
        "default".to_owned()
    } else {
        trimmed
    }
}

fn render_codex_extension_instructions() -> String {
    [
        "# Supermanager Memory Extension",
        "",
        "These resources are exported from Supermanager organization memory and skills workflows.",
        "",
        "- Treat each resource as imported durable context from a specific organization.",
        "- Preserve organization scoping instead of collapsing unrelated organizations together.",
        "- Use imported skills as reusable guidance, not as proof of live system state.",
        "- Prefer MCP or direct tools for fresh data that may have changed since export time.",
        "",
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    #[test]
    fn upsert_claude_imports_appends_and_replaces_managed_block() {
        let root = test_dir("claude-imports");
        let claude_path = root.join("CLAUDE.md");
        write_text(&claude_path, "# Local rules\n").unwrap();

        let first = upsert_claude_imports(
            &claude_path,
            &[Path::new("/tmp/memories.md"), Path::new("/tmp/skills.md")],
        )
        .unwrap();
        assert_eq!(first, ConfigFileUpdateStatus::Updated);

        let second = upsert_claude_imports(
            &claude_path,
            &[Path::new("/tmp/memories.md"), Path::new("/tmp/skills.md")],
        )
        .unwrap();
        assert_eq!(second, ConfigFileUpdateStatus::Unchanged);

        let contents = fs::read_to_string(&claude_path).unwrap();
        assert!(contents.contains("# Local rules"));
        assert_eq!(contents.matches(CLAUDE_IMPORTS_START).count(), 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn remove_claude_import_block_deletes_file_when_block_is_only_content() {
        let root = test_dir("remove-claude-imports");
        let claude_path = root.join("CLAUDE.md");
        write_text(
            &claude_path,
            &render_claude_import_block(&[Path::new("/tmp/memories.md")]),
        )
        .unwrap();

        assert!(remove_claude_import_block(&claude_path).unwrap());
        assert!(!claude_path.exists());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rebuild_codex_memory_extension_removes_stale_resources() {
        let root = test_dir("rebuild-codex-memory-extension");
        let home_dir = root.join("home");
        let cache_dir = home_dir
            .join(".supermanager")
            .join("agent-context")
            .join("api-supermanager-dev")
            .join("acme");
        fs::create_dir_all(&cache_dir).unwrap();
        write_text(&cache_dir.join("memories.md"), "# Memories\n").unwrap();
        write_text(&cache_dir.join("skills.md"), "# Skills\n").unwrap();

        let extension_root = home_dir.join(CODEX_MEMORY_EXTENSION);
        let stale_path = extension_root.join("resources").join("stale.md");
        write_text(&stale_path, "stale").unwrap();

        let outcome = rebuild_codex_memory_extension(&home_dir).unwrap();
        assert!(
            outcome
                .removed_paths
                .iter()
                .any(|path| path.ends_with("stale.md"))
        );
        assert!(extension_root.join("instructions.md").exists());
        assert!(
            extension_root
                .join("resources")
                .join("api-supermanager-dev--acme--memories.md")
                .exists()
        );

        fs::remove_dir_all(root).unwrap();
    }

    fn test_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("supermanager-context-{name}-{nonce}"))
    }
}
