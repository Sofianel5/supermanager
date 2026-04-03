mod sse;
pub mod summarize;

use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use reporter_protocol::{
    CreateRoomRequest, CreateRoomResponse, FeedResponse, HookTurnReport, IngestResponse,
    RoomMetadataResponse, Room, RoomSnapshot, StoredHookEvent,
};
use tokio::sync::broadcast;

use crate::store::Db;
use summarize::{SummaryStatusEvent, spawn_auto_summarize};

pub use sse::stream_feed;

const DEFAULT_PUBLIC_API_URL: &str = "https://supermanager.fly.dev";
const DEFAULT_PUBLIC_APP_URL: &str = "https://supermanager.dev";

// ── Shared state ────────────────────────────────────────────

#[derive(Clone)]
pub struct HookFeedEvent {
    pub room_id: String,
    pub event: StoredHookEvent,
}

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Db>,
    pub hook_events: broadcast::Sender<HookFeedEvent>,
    pub summary_events: broadcast::Sender<SummaryStatusEvent>,
    pub public_api_url: String,
    pub public_app_url: String,
    pub http: reqwest::Client,
    pub openai_api_key: Option<String>,
}

// ── Health ──────────────────────────────────────────────────

pub async fn health() -> &'static str {
    "ok"
}

// ── Room management ─────────────────────────────────────────

pub async fn create_room(
    State(state): State<AppState>,
    Json(req): Json<CreateRoomRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let room = state.db.create_room(&req.name).map_err(internal_error)?;
    let resp = CreateRoomResponse {
        dashboard_url: dashboard_url(&state.public_app_url, &room.room_id),
        join_command: cli_join_command(&state.public_api_url, &state.public_app_url, &room.room_id),
        room_id: room.room_id,
    };
    Ok((StatusCode::CREATED, Json(resp)))
}

// ── Room-scoped routes ──────────────────────────────────────

pub async fn ingest_hook_turn(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Json(report): Json<HookTurnReport>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let room = resolve_room(&state, &room_id)?;
    let room_id = room.room_id;

    let stored = state
        .db
        .insert_hook_event(&room_id, &report)
        .map_err(internal_error)?;
    let event_id = stored.event_id;
    let received_at = stored.received_at.clone();

    let _ = state.hook_events.send(HookFeedEvent {
        room_id: room_id.clone(),
        event: stored,
    });

    spawn_auto_summarize(&state, &room_id);

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
    Path(room_id): Path<String>,
) -> Result<Json<RoomMetadataResponse>, (StatusCode, String)> {
    let room = resolve_room(&state, &room_id)?;
    Ok(Json(RoomMetadataResponse {
        room_id: room.room_id,
        name: room.name,
        created_at: room.created_at,
    }))
}

pub async fn get_feed(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<FeedResponse>, (StatusCode, String)> {
    let room = resolve_room(&state, &room_id)?;
    let room_id = room.room_id;
    let events = state.db.get_hook_events(&room_id).map_err(internal_error)?;
    Ok(Json(FeedResponse { events }))
}

pub async fn get_manager_summary(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<RoomSnapshot>, (StatusCode, String)> {
    let room = resolve_room(&state, &room_id)?;
    let room_id = room.room_id;
    let summary = state.db.get_summary(&room_id).map_err(internal_error)?;
    Ok(Json(summary))
}

// ── Helpers ─────────────────────────────────────────────────

fn resolve_room(state: &AppState, room_id: &str) -> Result<Room, (StatusCode, String)> {
    state
        .db
        .get_room(room_id)
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

// ── Tests ──────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use axum::body::to_bytes;
    use reporter_protocol::EmployeeSnapshot;
    use tempfile::TempDir;

    #[test]
    fn cli_join_command_includes_app_url() {
        let command = cli_join_command(
            "https://api.example.com/",
            "https://app.example.com/",
            "bright-fox-1",
        );

        assert_eq!(
            command,
            "supermanager join bright-fox-1 --server \"https://api.example.com\" --app-url \"https://app.example.com\""
        );
    }

    #[test]
    fn cli_join_command_uses_short_form_for_default_deployment() {
        let command = cli_join_command(
            "https://supermanager.fly.dev/",
            "https://supermanager.dev/",
            "ABC123",
        );

        assert_eq!(command, "supermanager join ABC123");
    }

    #[test]
    fn dashboard_url_is_room_path() {
        let url = dashboard_url("https://app.example.com/", "bright-fox-1");

        assert_eq!(url, "https://app.example.com/r/bright-fox-1");
    }

    #[tokio::test]
    async fn get_manager_summary_returns_json_payload() {
        let tempdir = TempDir::new().unwrap();
        let db = Arc::new(Db::open(&tempdir.path().join("api-summary.sqlite")).unwrap());
        let room = db.create_room("Summary Room").unwrap();
        let summary = RoomSnapshot {
            bluf_markdown: "- Top line".to_owned(),
            overview_markdown: "Detailed overview".to_owned(),
            employees: vec![EmployeeSnapshot {
                employee_name: "Alice".to_owned(),
                content_markdown: "- On track".to_owned(),
                last_update_at: "2026-04-02T12:00:00Z".to_owned(),
            }],
        };
        db.set_summary(&room.room_id, &summary).unwrap();

        let (hook_events, _) = broadcast::channel(8);
        let (summary_events, _) = broadcast::channel(8);
        let state = AppState {
            db,
            hook_events,
            summary_events,
            public_api_url: DEFAULT_PUBLIC_API_URL.to_owned(),
            public_app_url: DEFAULT_PUBLIC_APP_URL.to_owned(),
            http: reqwest::Client::new(),
            openai_api_key: None,
        };

        let response = get_manager_summary(State(state), Path(room.room_id.clone()))
            .await
            .unwrap()
            .into_response();

        assert_eq!(
            response
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok()),
            Some("application/json")
        );

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let returned_summary: RoomSnapshot = serde_json::from_slice(&body).unwrap();
        assert_eq!(returned_summary, summary);
    }
}
