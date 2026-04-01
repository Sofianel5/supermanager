use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressNote {
    pub employee_name: String,
    pub repo: String,
    #[serde(default)]
    pub branch: Option<String>,
    pub progress_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredProgressNote {
    pub note_id: Uuid,
    pub received_at: String,
    #[serde(flatten)]
    pub note: ProgressNote,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestResponse {
    pub note_id: Uuid,
    pub received_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedResponse {
    pub notes: Vec<StoredProgressNote>,
}

// ── Room types ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room {
    pub room_id: String,
    pub name: String,
    pub secret: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRoomRequest {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRoomResponse {
    pub room_id: String,
    pub secret: String,
    pub dashboard_url: String,
    pub join_command: String,
}
