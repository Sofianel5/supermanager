use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../packages/common/api-protocol/")]
pub struct IngestResponse {
    #[ts(type = "string")]
    pub event_id: Uuid,
    pub received_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../packages/common/api-protocol/")]
pub struct FeedResponse {
    pub events: Vec<StoredHookEvent>,
    pub total_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../packages/common/api-protocol/")]
pub struct HookTurnReport {
    pub client: String,
    pub repo_root: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[ts(type = "unknown")]
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../packages/common/api-protocol/")]
pub struct StoredHookEvent {
    #[ts(type = "number")]
    pub seq: i64,
    #[ts(type = "string")]
    pub event_id: Uuid,
    pub received_at: String,
    pub employee_user_id: String,
    pub employee_name: String,
    pub client: String,
    pub repo_root: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[ts(type = "unknown")]
    pub payload: Value,
}
