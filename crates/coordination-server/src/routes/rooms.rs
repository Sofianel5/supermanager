use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use reporter_protocol::{
    CreateRoomRequest, CreateRoomResponse, FeedResponse, HookTurnReport, IngestResponse,
    RoomMetadataResponse, RoomSnapshot,
};
use serde::Deserialize;

use crate::auth;
use crate::state::{AppState, HookFeedEvent};
use crate::store::RoomRecord;
use crate::util::{internal_error, service_unavailable_error, trim_url};

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
    let room_id = room.room_id.clone();

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

pub(crate) async fn resolve_room(
    state: &AppState,
    room_id: &str,
) -> Result<RoomRecord, (StatusCode, String)> {
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
