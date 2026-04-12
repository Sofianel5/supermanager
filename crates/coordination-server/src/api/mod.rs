mod agent;
mod auth;
mod sse;
pub mod summarize;

use std::{fs, path::PathBuf, sync::Arc};

use anyhow::Context;
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use reporter_protocol::{
    CreateRoomRequest, CreateRoomResponse, FeedResponse, HookTurnReport, IngestResponse, Room,
    RoomMetadataResponse, RoomSnapshot, StoredHookEvent,
};
use serde::Deserialize;
use tokio::sync::broadcast;

use crate::store::Db;
use summarize::SummaryStatusEvent;

pub use agent::RoomSummaryAgent;
pub use auth::{
    AuthConfig, accept_invite, auth_config, create_email_invite, create_link_invite, current_user,
    refresh_cli_token,
};
pub use sse::stream_feed;

const DEFAULT_PUBLIC_API_URL: &str = "https://api.supermanager.dev";
const DEFAULT_PUBLIC_APP_URL: &str = "https://supermanager.dev";
const FEED_PAGE_DEFAULT: i64 = 10;
const FEED_PAGE_MAX: i64 = 100;

#[derive(Debug, Deserialize)]
pub struct FeedQuery {
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub before: Option<i64>,
}

#[derive(Clone)]
pub struct HookFeedEvent {
    pub room_id: String,
    pub event: StoredHookEvent,
}

#[derive(Clone, Debug)]
pub struct StoragePaths {
    pub data_dir: PathBuf,
    pub codex_home: PathBuf,
    pub rooms_dir: PathBuf,
}

impl StoragePaths {
    pub fn new(data_dir: PathBuf) -> Self {
        let codex_home = data_dir.join("codex");
        let rooms_dir = data_dir.join("rooms");
        Self {
            data_dir,
            codex_home,
            rooms_dir,
        }
    }

    pub fn initialize(&self) -> anyhow::Result<()> {
        for path in [&self.data_dir, &self.codex_home, &self.rooms_dir] {
            fs::create_dir_all(path)
                .with_context(|| format!("failed to create storage dir {}", path.display()))?;
        }
        Ok(())
    }

    pub fn check_ready(&self) -> anyhow::Result<()> {
        for path in [&self.data_dir, &self.codex_home, &self.rooms_dir] {
            if !path.is_dir() {
                anyhow::bail!("storage dir missing or not a directory: {}", path.display());
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Db>,
    pub agent: RoomSummaryAgent,
    pub hook_events: broadcast::Sender<HookFeedEvent>,
    pub summary_events: broadcast::Sender<SummaryStatusEvent>,
    pub storage: StoragePaths,
    pub public_api_url: String,
    pub public_app_url: String,
    pub auth: AuthConfig,
}

pub async fn health(State(state): State<AppState>) -> Result<&'static str, (StatusCode, String)> {
    state.db.ping().await.map_err(service_unavailable_error)?;
    state
        .storage
        .check_ready()
        .map_err(service_unavailable_error)?;
    Ok("ok")
}

pub async fn create_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateRoomRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let user = auth::require_user(&state, &headers).await?;
    let room = auth::create_room_for_owner(&state, &req.name, &user.user_id).await?;
    Ok((
        StatusCode::CREATED,
        Json(CreateRoomResponse {
            dashboard_url: dashboard_url(&state.public_app_url, &room.room_id),
            join_command: cli_join_command(
                &state.public_api_url,
                &state.public_app_url,
                &room.room_id,
            ),
            room_id: room.room_id,
        }),
    ))
}

pub async fn ingest_hook_turn(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
    Json(report): Json<HookTurnReport>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let (_, membership) = auth::ensure_room_member(&state, &headers, &room_id).await?;
    let room = resolve_room(&state, &membership.room_id).await?;
    let room_id = room.room_id;

    let stored = state
        .db
        .insert_hook_event(&room_id, &report)
        .await
        .map_err(internal_error)?;
    let event_id = stored.event_id;
    let received_at = stored.received_at.clone();

    let _ = state.hook_events.send(HookFeedEvent {
        room_id: room_id.clone(),
        event: stored.clone(),
    });

    state
        .agent
        .enqueue(room_id.clone(), stored)
        .await
        .map_err(internal_error)?;

    Ok((
        StatusCode::ACCEPTED,
        Json(IngestResponse {
            event_id,
            received_at,
        }),
    ))
}

pub async fn get_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
) -> Result<Json<RoomMetadataResponse>, (StatusCode, String)> {
    let (_, membership) = auth::ensure_room_member(&state, &headers, &room_id).await?;
    let room = resolve_room(&state, &membership.room_id).await?;
    Ok(Json(RoomMetadataResponse {
        room_id: room.room_id,
        name: room.name,
        created_at: room.created_at,
        viewer_role: membership.role,
    }))
}

pub async fn get_feed(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
    Query(q): Query<FeedQuery>,
) -> Result<Json<FeedResponse>, (StatusCode, String)> {
    let (_, membership) = auth::ensure_room_member(&state, &headers, &room_id).await?;
    let room = resolve_room(&state, &membership.room_id).await?;
    let limit = q.limit.unwrap_or(FEED_PAGE_DEFAULT).clamp(1, FEED_PAGE_MAX);
    let events = state
        .db
        .get_hook_events(&room.room_id, q.before, None, Some(limit))
        .await
        .map_err(internal_error)?;
    Ok(Json(FeedResponse { events }))
}

pub async fn get_manager_summary(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
) -> Result<Json<RoomSnapshot>, (StatusCode, String)> {
    let (_, membership) = auth::ensure_room_member(&state, &headers, &room_id).await?;
    let room = resolve_room(&state, &membership.room_id).await?;
    let summary = state
        .db
        .get_summary(&room.room_id)
        .await
        .map_err(internal_error)?;
    Ok(Json(summary))
}

async fn resolve_room(state: &AppState, room_id: &str) -> Result<Room, (StatusCode, String)> {
    state
        .db
        .get_room(room_id)
        .await
        .map_err(internal_error)?
        .ok_or((StatusCode::NOT_FOUND, format!("room not found: {room_id}")))
}

fn cli_join_command(api_url: &str, app_url: &str, room_id: &str) -> String {
    let api_url = trim_url(api_url);
    let app_url = trim_url(app_url);
    if api_url == DEFAULT_PUBLIC_API_URL && app_url == DEFAULT_PUBLIC_APP_URL {
        return format!("supermanager join {room_id}");
    }

    format!("supermanager join {room_id} --server \"{api_url}\" --app-url \"{app_url}\"")
}

fn dashboard_url(app_url: &str, room_id: &str) -> String {
    format!("{}/r/{room_id}", trim_url(app_url))
}

fn trim_url(url: &str) -> &str {
    url.trim_end_matches('/')
}

fn internal_error(error: anyhow::Error) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}

fn service_unavailable_error(error: anyhow::Error) -> (StatusCode, String) {
    (StatusCode::SERVICE_UNAVAILABLE, error.to_string())
}
