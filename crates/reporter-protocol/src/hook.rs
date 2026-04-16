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
