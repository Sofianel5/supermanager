use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestResponse {
    pub event_id: Uuid,
    pub received_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedResponse {
    pub events: Vec<StoredHookEvent>,
}

// ── Room types ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room {
    pub room_id: String,
    pub name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRoomRequest {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRoomResponse {
    pub room_id: String,
    pub dashboard_url: String,
    pub join_command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomMetadataResponse {
    pub room_id: String,
    pub name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoomSnapshot {
    #[serde(default)]
    pub bluf_markdown: String,
    #[serde(default)]
    pub overview_markdown: String,
    #[serde(default)]
    pub employees: Vec<EmployeeSnapshot>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmployeeSnapshot {
    pub employee_name: String,
    #[serde(default)]
    pub content_markdown: String,
    #[serde(default)]
    pub last_update_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookTurnReport {
    pub employee_name: String,
    pub client: String,
    pub repo_root: String,
    #[serde(default)]
    pub branch: Option<String>,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredHookEvent {
    pub event_id: Uuid,
    pub received_at: String,
    pub employee_name: String,
    pub client: String,
    pub repo_root: String,
    #[serde(default)]
    pub branch: Option<String>,
    pub payload: Value,
}
