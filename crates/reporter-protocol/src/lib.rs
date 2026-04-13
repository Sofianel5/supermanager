use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestResponse {
    pub event_id: Uuid,
    pub received_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../web/src/generated/")]
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
    #[serde(default)]
    pub organization_slug: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRoomResponse {
    pub room_id: String,
    pub dashboard_url: String,
    pub join_command: String,
    pub organization_slug: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../web/src/generated/")]
pub struct RoomMetadataResponse {
    pub room_id: String,
    pub name: String,
    pub created_at: String,
    pub organization_slug: String,
    pub join_command: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../../web/src/generated/")]
pub struct RoomSnapshot {
    #[serde(default)]
    pub bluf_markdown: String,
    #[serde(default)]
    pub overview_markdown: String,
    #[serde(default)]
    pub employees: Vec<EmployeeSnapshot>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../../web/src/generated/")]
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

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../web/src/generated/")]
pub struct StoredHookEvent {
    #[ts(type = "number")]
    pub seq: i64,
    #[ts(type = "string")]
    pub event_id: Uuid,
    pub received_at: String,
    pub employee_name: String,
    pub client: String,
    pub repo_root: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[ts(type = "unknown")]
    pub payload: Value,
}
