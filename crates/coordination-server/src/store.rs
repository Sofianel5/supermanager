use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result, bail};
use rand::Rng;
use reporter_protocol::{ProgressNote, Room, StoredProgressNote};
use rusqlite::{Connection, params};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

// ── Slug generation word lists ──────────────────────────────

const ADJECTIVES: &[&str] = &[
    "bright", "calm", "cool", "dark", "fast", "bold", "keen", "warm", "wild", "free",
    "swift", "brave", "quiet", "sharp", "clear", "fresh", "grand", "prime", "true", "fair",
];

const NOUNS: &[&str] = &[
    "fox", "owl", "bear", "wolf", "hawk", "deer", "lynx", "crow", "dove", "hare",
    "lion", "seal", "wren", "orca", "puma", "swan", "moth", "frog", "newt", "mink",
];

/// The default room used for backwards-compatible (non-room) API routes.
pub const LOCAL_ROOM_ID: &str = "__local";

// ── Database wrapper ────────────────────────────────────────

pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    /// Open (or create) the SQLite database at `path` and run migrations.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open database at {}", path.display()))?;

        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS rooms (
                room_id    TEXT PRIMARY KEY,
                name       TEXT NOT NULL,
                secret     TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS notes (
                note_id       TEXT PRIMARY KEY,
                room_id       TEXT NOT NULL REFERENCES rooms(room_id),
                employee_name TEXT NOT NULL,
                repo          TEXT NOT NULL,
                branch        TEXT,
                progress_text TEXT NOT NULL,
                received_at   TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS summaries (
                room_id          TEXT PRIMARY KEY REFERENCES rooms(room_id),
                content_markdown TEXT NOT NULL,
                updated_at       TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_notes_room_received
                ON notes(room_id, received_at);",
        )?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    // ── Rooms ───────────────────────────────────────────────

    /// Create a new room with an auto-generated slug and secret.
    pub fn create_room(&self, name: &str) -> Result<Room> {
        let conn = self.conn.lock().unwrap();
        let mut rng = rand::rng();
        let secret = generate_secret(&mut rng);
        let created_at = now_rfc3339();

        // Try generating a unique slug (retry on collision).
        let room_id = loop {
            let slug = generate_slug(&mut rng);
            let exists: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM rooms WHERE room_id = ?1)",
                params![slug],
                |row| row.get(0),
            )?;
            if !exists {
                break slug;
            }
        };

        conn.execute(
            "INSERT INTO rooms (room_id, name, secret, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![room_id, name, secret, created_at],
        )?;

        Ok(Room {
            room_id,
            name: name.to_owned(),
            secret,
            created_at,
        })
    }

    /// Retrieve a room by its slug, or `None` if it does not exist.
    pub fn get_room(&self, room_id: &str) -> Result<Option<Room>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT room_id, name, secret, created_at FROM rooms WHERE room_id = ?1",
        )?;
        let mut rows = stmt.query_map(params![room_id], |row| {
            Ok(Room {
                room_id: row.get(0)?,
                name: row.get(1)?,
                secret: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// Return `true` if the supplied secret matches the room's secret.
    pub fn verify_room_secret(&self, room_id: &str, secret: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let result: Option<String> = conn
            .query_row(
                "SELECT secret FROM rooms WHERE room_id = ?1",
                params![room_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(result.as_deref() == Some(secret))
    }

    /// Delete a room and all associated notes/summaries. Returns `true` if the
    /// room existed.
    pub fn delete_room(&self, room_id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM summaries WHERE room_id = ?1", params![room_id])?;
        conn.execute("DELETE FROM notes WHERE room_id = ?1", params![room_id])?;
        let affected = conn.execute("DELETE FROM rooms WHERE room_id = ?1", params![room_id])?;
        Ok(affected > 0)
    }

    /// Ensure the local default room exists (idempotent).
    pub fn ensure_local_room(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO rooms (room_id, name, secret, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![LOCAL_ROOM_ID, "Local", generate_secret(&mut rand::rng()), now_rfc3339()],
        )?;
        Ok(())
    }

    // ── Notes ───────────────────────────────────────────────

    /// Insert a progress note into a room.
    pub fn insert_note(
        &self,
        room_id: &str,
        note: &ProgressNote,
    ) -> Result<StoredProgressNote> {
        let conn = self.conn.lock().unwrap();
        let note_id = Uuid::new_v4();
        let received_at = now_rfc3339();

        conn.execute(
            "INSERT INTO notes (note_id, room_id, employee_name, repo, branch, progress_text, received_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                note_id.to_string(),
                room_id,
                note.employee_name,
                note.repo,
                note.branch,
                note.progress_text,
                received_at,
            ],
        )
        .with_context(|| format!("failed to insert note into room {room_id}"))?;

        Ok(StoredProgressNote {
            note_id,
            received_at,
            note: note.clone(),
        })
    }

    /// Get all notes in a room, newest first.
    pub fn get_notes(&self, room_id: &str) -> Result<Vec<StoredProgressNote>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT note_id, employee_name, repo, branch, progress_text, received_at
             FROM notes WHERE room_id = ?1
             ORDER BY received_at DESC",
        )?;

        let notes = stmt
            .query_map(params![room_id], |row| {
                let note_id_str: String = row.get(0)?;
                Ok(StoredProgressNote {
                    note_id: Uuid::parse_str(&note_id_str).unwrap_or_else(|_| Uuid::nil()),
                    received_at: row.get(5)?,
                    note: ProgressNote {
                        employee_name: row.get(1)?,
                        repo: row.get(2)?,
                        branch: row.get(3)?,
                        progress_text: row.get(4)?,
                    },
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(notes)
    }

    /// Get all notes received after the note with the given ID (chronological
    /// order, oldest first). Used for SSE replay.
    pub fn get_notes_after(
        &self,
        room_id: &str,
        after_note_id: &str,
    ) -> Result<Vec<StoredProgressNote>> {
        let conn = self.conn.lock().unwrap();

        // Find the received_at of the anchor note.
        let anchor_time: Option<String> = conn
            .query_row(
                "SELECT received_at FROM notes WHERE note_id = ?1 AND room_id = ?2",
                params![after_note_id, room_id],
                |row| row.get(0),
            )
            .optional()?;

        let anchor_time = match anchor_time {
            Some(t) => t,
            None => return Ok(Vec::new()),
        };

        let mut stmt = conn.prepare(
            "SELECT note_id, employee_name, repo, branch, progress_text, received_at
             FROM notes
             WHERE room_id = ?1 AND received_at > ?2
             ORDER BY received_at ASC",
        )?;

        let notes = stmt
            .query_map(params![room_id, anchor_time], |row| {
                let note_id_str: String = row.get(0)?;
                Ok(StoredProgressNote {
                    note_id: Uuid::parse_str(&note_id_str).unwrap_or_else(|_| Uuid::nil()),
                    received_at: row.get(5)?,
                    note: ProgressNote {
                        employee_name: row.get(1)?,
                        repo: row.get(2)?,
                        branch: row.get(3)?,
                        progress_text: row.get(4)?,
                    },
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(notes)
    }

    // ── Summaries ───────────────────────────────────────────

    /// Get the manager summary for a room.
    pub fn get_summary(&self, room_id: &str) -> Result<String> {
        let conn = self.conn.lock().unwrap();
        let result: Option<String> = conn
            .query_row(
                "SELECT content_markdown FROM summaries WHERE room_id = ?1",
                params![room_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(result.unwrap_or_default())
    }

    /// Create or replace the manager summary for a room.
    pub fn set_summary(&self, room_id: &str, content: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO summaries (room_id, content_markdown, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(room_id) DO UPDATE SET
                content_markdown = excluded.content_markdown,
                updated_at       = excluded.updated_at",
            params![room_id, content, now_rfc3339()],
        )?;
        Ok(())
    }
}

// ── Helpers ─────────────────────────────────────────────────

/// Extension trait so we can use `.optional()` on rusqlite single-row queries.
trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

fn generate_slug(rng: &mut impl Rng) -> String {
    let adj = ADJECTIVES[rng.random_range(0..ADJECTIVES.len())];
    let noun = NOUNS[rng.random_range(0..NOUNS.len())];
    let num: u32 = rng.random_range(1..100);
    format!("{adj}-{noun}-{num}")
}

fn generate_secret(rng: &mut impl Rng) -> String {
    let bytes: [u8; 16] = rng.random();
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("sm_sec_{hex}")
}

pub fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| OffsetDateTime::now_utc().unix_timestamp().to_string())
}
