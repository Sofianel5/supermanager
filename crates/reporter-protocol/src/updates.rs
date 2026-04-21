use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../packages/common/updates-protocol/")]
pub enum UpdateScope {
    Organization,
    Project,
    Member,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../packages/common/updates-protocol/")]
pub struct Update {
    #[ts(type = "number")]
    pub seq: i64,
    #[ts(type = "string")]
    pub update_id: Uuid,
    pub organization_id: String,
    pub scope: UpdateScope,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub member_user_id: Option<String>,
    pub body_text: String,
    pub source_workflow_kind: String,
    pub created_at: String,
}
