use anyhow::{Context, Result, bail};
use reporter_protocol::{CreateProjectRequest, CreateProjectResponse};
use serde::Deserialize;
use serde_json::json;

use crate::{
    auth::{authed, fetch_project, get_viewer, require_auth_state, resolve_active_org_interactive},
    context::sync_repo_context,
    local::{
        default_project_name, install_repo_hooks, resolve_repo_root, upsert_repo_project_config,
    },
    support::{API_TIMEOUT_SECONDS, build_http_client, ensure_success, normalize_url},
    types::{
        CreateProjectConfig, CreateProjectOutcome, JoinConfig, JoinOutcome, RepoProjectConfig,
        SyncContextConfig,
    },
};

#[derive(Debug, Deserialize)]
struct ConnectionResponse {
    api_key: String,
    api_key_id: String,
    dashboard_url: String,
    project_id: String,
}

pub fn create_project(config: CreateProjectConfig) -> Result<CreateProjectOutcome> {
    let repo_dir = resolve_repo_root(&config.cwd)?;
    let server_url = normalize_url(&config.server_url);
    let project_name = config
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| default_project_name(&repo_dir));
    let http = build_http_client(API_TIMEOUT_SECONDS)?;
    let mut auth_state = require_auth_state(&config.home_dir, &server_url)?;
    let viewer = get_viewer(&http, &server_url, &auth_state.access_token)?;
    let active_org = resolve_active_org_interactive(
        &http,
        &server_url,
        &mut auth_state,
        &config.home_dir,
        &viewer,
        config.organization_slug.as_deref(),
        &format!("supermanager create project --server {server_url}"),
    )?;
    let response = authed(&http, &auth_state.access_token)
        .post(format!("{server_url}/v1/projects"))
        .json(&CreateProjectRequest {
            name: project_name.clone(),
            organization_slug: Some(active_org.organization_slug.clone()),
        })
        .send()
        .context("failed to create project")?;
    let response = ensure_success(response, "create project")?;
    let payload: CreateProjectResponse = response
        .json()
        .context("failed to parse create-project response JSON")?;

    Ok(CreateProjectOutcome {
        project_id: payload.project_id,
        project_name,
        dashboard_url: payload.dashboard_url,
        join_command: payload.join_command,
        repo_dir,
    })
}

pub fn join_repo(config: JoinConfig) -> Result<JoinOutcome> {
    let repo_dir = resolve_repo_root(&config.repo_dir)?;
    let server_url = normalize_url(&config.server_url);
    let http = build_http_client(API_TIMEOUT_SECONDS)?;
    let mut auth_state = require_auth_state(&config.home_dir, &server_url)?;
    let viewer = get_viewer(&http, &server_url, &auth_state.access_token)?;
    let member_name = viewer.user.name.clone();
    let active_org = resolve_active_org_interactive(
        &http,
        &server_url,
        &mut auth_state,
        &config.home_dir,
        &viewer,
        config.organization_slug.as_deref(),
        &format!(
            "supermanager join {} --server {server_url}",
            config.project_id
        ),
    )?;
    let project = fetch_project(
        &http,
        &server_url,
        &auth_state.access_token,
        &config.project_id,
    )?;

    if project.organization_slug != active_org.organization_slug {
        bail!(
            "project {} belongs to organization {}, but the active organization is {}",
            project.project_id,
            project.organization_slug,
            active_org.organization_slug
        );
    }

    let connection = authed(&http, &auth_state.access_token)
        .post(format!(
            "{server_url}/v1/projects/{}/connections",
            project.project_id
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

    let project_config = RepoProjectConfig {
        api_key: connection.api_key,
        api_key_id: connection.api_key_id,
        organization_slug: project.organization_slug.clone(),
        server_url,
        project_id: project.project_id.clone(),
    };

    upsert_repo_project_config(&config.home_dir, &repo_dir, project_config)?;
    install_repo_hooks(&repo_dir)?;
    sync_repo_context(SyncContextConfig {
        home_dir: config.home_dir.clone(),
        cwd: repo_dir.clone(),
    })?;

    Ok(JoinOutcome {
        project_id: connection.project_id,
        member_name,
        dashboard_url: connection.dashboard_url,
        repo_dir,
    })
}
