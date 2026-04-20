use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../../packages/common/summary-protocol/")]
pub struct RoomSnapshot {
    #[serde(default)]
    pub bluf_markdown: String,
    #[serde(default)]
    pub detailed_summary_markdown: String,
    #[serde(default)]
    pub employees: Vec<EmployeeSnapshot>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../../packages/common/summary-protocol/")]
pub struct EmployeeSnapshot {
    #[serde(default)]
    pub employee_user_id: String,
    pub employee_name: String,
    #[serde(default)]
    pub room_ids: Vec<String>,
    #[serde(default)]
    pub bluf_markdown: String,
    #[serde(default)]
    pub last_update_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../../packages/common/summary-protocol/")]
pub struct RoomBlufSnapshot {
    pub room_id: String,
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
    pub rooms: Vec<RoomBlufSnapshot>,
    #[serde(default)]
    pub employees: Vec<EmployeeSnapshot>,
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
