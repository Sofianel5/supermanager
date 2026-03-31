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

impl StoredProgressNote {
    pub fn new(note: ProgressNote, received_at: String) -> Self {
        Self {
            note_id: Uuid::new_v4(),
            received_at,
            note,
        }
    }
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
