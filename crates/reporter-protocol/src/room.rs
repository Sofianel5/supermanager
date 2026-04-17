use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room {
    pub room_id: String,
    pub name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../packages/common/api-protocol/")]
pub struct CreateRoomRequest {
    pub name: String,
    #[serde(default)]
    pub organization_slug: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../packages/common/api-protocol/")]
pub struct CreateRoomResponse {
    pub room_id: String,
    pub dashboard_url: String,
    pub join_command: String,
    pub organization_slug: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../packages/common/api-protocol/")]
pub struct RoomMetadataResponse {
    pub room_id: String,
    pub name: String,
    pub created_at: String,
    pub organization_slug: String,
    pub join_command: String,
}
