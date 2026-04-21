use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../../packages/common/http-types/")]
pub struct ActivityUpdate {
    pub created_at: String,
    pub statement_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../../packages/common/http-types/")]
pub struct ActivityUpdatesResponse {
    pub updates: Vec<ActivityUpdate>,
}
