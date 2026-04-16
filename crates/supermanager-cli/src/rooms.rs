use anyhow::{Context, Result, bail};
use reporter_protocol::{CreateRoomRequest, CreateRoomResponse};
use serde::Deserialize;
use serde_json::json;

use crate::{
    auth::{authed, fetch_room, get_viewer, require_auth_state, resolve_active_org_interactive},
    local::{
        default_room_name, detect_employee_name, install_repo_hooks, resolve_repo_root,
        upsert_repo_room_config,
    },
    support::{API_TIMEOUT_SECONDS, build_http_client, ensure_success, normalize_url},
    types::{CreateRoomConfig, CreateRoomOutcome, JoinConfig, JoinOutcome, RepoRoomConfig},
};

#[derive(Debug, Deserialize)]
struct ConnectionResponse {
    api_key: String,
    api_key_id: String,
    dashboard_url: String,
    room_id: String,
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
    let active_org = resolve_active_org_interactive(
        &http,
        &server_url,
        &mut auth_state,
        &config.home_dir,
        &viewer,
        config.organization_slug.as_deref(),
        &format!("supermanager create room --server {server_url}"),
    )?;
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
    let active_org = resolve_active_org_interactive(
        &http,
        &server_url,
        &mut auth_state,
        &config.home_dir,
        &viewer,
        config.organization_slug.as_deref(),
        &format!("supermanager join {} --server {server_url}", config.room_id),
    )?;
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
