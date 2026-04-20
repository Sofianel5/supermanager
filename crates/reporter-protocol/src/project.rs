use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub project_id: String,
    pub name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../packages/common/api-protocol/")]
pub struct CreateProjectRequest {
    pub name: String,
    #[serde(default)]
    pub organization_slug: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../packages/common/api-protocol/")]
pub struct CreateProjectResponse {
    pub project_id: String,
    pub dashboard_url: String,
    pub join_command: String,
    pub organization_slug: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../packages/common/api-protocol/")]
pub struct ProjectMetadataResponse {
    pub project_id: String,
    pub name: String,
    pub created_at: String,
    pub organization_slug: String,
    pub join_command: String,
}
