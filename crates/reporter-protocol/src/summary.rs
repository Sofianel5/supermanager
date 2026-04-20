use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../../packages/common/summary-protocol/")]
pub struct ProjectSnapshot {
    #[serde(default)]
    pub bluf_markdown: String,
    #[serde(default)]
    pub detailed_summary_markdown: String,
    #[serde(default)]
    pub members: Vec<MemberSnapshot>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../../packages/common/summary-protocol/")]
pub struct MemberSnapshot {
    pub member_user_id: String,
    pub member_name: String,
    #[serde(default)]
    pub project_ids: Vec<String>,
    #[serde(default)]
    pub bluf_markdown: String,
    #[serde(default)]
    pub last_update_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../../packages/common/summary-protocol/")]
pub struct ProjectBlufSnapshot {
    pub project_id: String,
    #[serde(default)]
    pub bluf_markdown: String,
    #[serde(default)]
    pub last_update_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../../packages/common/summary-protocol/")]
pub struct OrganizationSnapshot {
    #[serde(default)]
    pub bluf_markdown: String,
    #[serde(default)]
    pub projects: Vec<ProjectBlufSnapshot>,
    #[serde(default)]
    pub members: Vec<MemberSnapshot>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../packages/common/summary-protocol/")]
pub enum SummaryStatus {
    Generating,
    Ready,
    Error,
}

impl SummaryStatus {
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::Generating => "generating",
            Self::Ready => "ready",
            Self::Error => "error",
        }
    }
}
