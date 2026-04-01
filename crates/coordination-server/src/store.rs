use std::{fs, path::Path};

use anyhow::{Context, Result};
use reporter_protocol::StoredProgressNote;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub fn initialize(data_dir: &Path) -> Result<()> {
    fs::create_dir_all(data_dir.join("notes"))
        .with_context(|| format!("failed to create {}", data_dir.join("notes").display()))?;
    Ok(())
}

pub fn persist_note(data_dir: &Path, stored: &StoredProgressNote) -> Result<()> {
    let notes_dir = data_dir.join("notes");
    let path = notes_dir.join(format!(
        "{}-{}.json",
        safe_fs_segment(&stored.note.employee_name),
        stored.note_id
    ));
    fs::write(&path, serde_json::to_vec_pretty(stored)?)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub fn read_all_notes(data_dir: &Path) -> Result<Vec<StoredProgressNote>> {
    let notes_dir = data_dir.join("notes");
    let mut notes: Vec<StoredProgressNote> = Vec::new();

    for entry in fs::read_dir(&notes_dir)
        .with_context(|| format!("failed to read {}", notes_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "json") {
            let bytes =
                fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
            let note: StoredProgressNote = serde_json::from_slice(&bytes)
                .with_context(|| format!("invalid note in {}", path.display()))?;
            notes.push(note);
        }
    }

    notes.sort_by(|a, b| b.received_at.cmp(&a.received_at));
    Ok(notes)
}

pub fn read_notes_after(data_dir: &Path, note_id: &str) -> Result<Vec<StoredProgressNote>> {
    let mut notes = read_all_notes(data_dir)?;
    notes.reverse();

    let mut found = false;
    let mut replay = Vec::new();
    for note in notes {
        if found {
            replay.push(note);
            continue;
        }

        if note.note_id.to_string() == note_id {
            found = true;
        }
    }

    Ok(replay)
}

pub fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| OffsetDateTime::now_utc().unix_timestamp().to_string())
}

fn safe_fs_segment(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => ch,
            _ => '_',
        })
        .collect();

    if cleaned.is_empty() {
        "unknown".to_owned()
    } else {
        cleaned
    }
}
