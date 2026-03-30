use std::fmt;

use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Host {
    Claude,
    Codex,
}

impl Host {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }
}

impl fmt::Display for Host {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum NoteKind {
    Intent,
    Progress,
}

impl NoteKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Intent => "intent",
            Self::Progress => "progress",
        }
    }
}

impl fmt::Display for NoteKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressNote {
    pub employee_name: String,
    pub host: Host,
    pub kind: NoteKind,
    pub workspace: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReportState {
    pub updated_at: String,
    pub notes_ingested: usize,
    pub markdown: String,
}

impl ReportState {
    pub fn empty() -> Self {
        Self {
            updated_at: String::new(),
            notes_ingested: 0,
            markdown: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestResponse {
    pub note_id: Uuid,
    pub updated_at: String,
    pub notes_ingested: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentReportResponse {
    pub updated_at: String,
    pub notes_ingested: usize,
    pub markdown: String,
}
