use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result};
use rand::Rng;
use reporter_protocol::{ProgressNote, Room, StoredProgressNote};
use rusqlite::{Connection, params};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

// ── Slug generation word lists ──────────────────────────────

const ADJECTIVES: &[&str] = &[
    "bright", "calm", "cool", "dark", "fast", "bold", "keen", "warm", "wild", "free", "swift",
    "brave", "quiet", "sharp", "clear", "fresh", "grand", "prime", "true", "fair",
];

const NOUNS: &[&str] = &[
    "fox", "owl", "bear", "wolf", "hawk", "deer", "lynx", "crow", "dove", "hare", "lion", "seal",
    "wren", "orca", "puma", "swan", "moth", "frog", "newt", "mink",
];

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
                status           TEXT NOT NULL DEFAULT 'ready',
                updated_at       TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS tasks (
                task_id    TEXT PRIMARY KEY,
                room_id    TEXT NOT NULL REFERENCES rooms(room_id),
                title      TEXT NOT NULL,
                status     TEXT NOT NULL DEFAULT 'todo',
                assignee   TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_notes_room_received
                ON notes(room_id, received_at);
            CREATE INDEX IF NOT EXISTS idx_tasks_room
                ON tasks(room_id);",
        )?;

        // ── Migrations for existing DBs ────────────────────────
        // Add status column to summaries if missing
        let has_status: bool = conn
            .prepare("SELECT sql FROM sqlite_master WHERE type='table' AND name='summaries'")?
            .query_row([], |row| row.get::<_, String>(0))
            .map(|sql| sql.contains("status"))
            .unwrap_or(false);
        if !has_status {
            conn.execute_batch(
                "ALTER TABLE summaries ADD COLUMN status TEXT NOT NULL DEFAULT 'ready';",
            )?;
        }

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
        let mut stmt =
            conn.prepare("SELECT room_id, name, secret, created_at FROM rooms WHERE room_id = ?1")?;
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

    // ── Notes ───────────────────────────────────────────────

    /// Insert a progress note into a room.
    pub fn insert_note(&self, room_id: &str, note: &ProgressNote) -> Result<StoredProgressNote> {
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

    /// Get the received_at timestamp of a person's most recent note in a room.
    pub fn get_last_update_time(
        &self,
        room_id: &str,
        employee_name: &str,
    ) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let result: Option<String> = conn
            .query_row(
                "SELECT received_at FROM notes
                 WHERE room_id = ?1 AND employee_name = ?2
                 ORDER BY received_at DESC LIMIT 1",
                params![room_id, employee_name],
                |row| row.get(0),
            )
            .optional()?;
        Ok(result)
    }

    /// Get notes from OTHER people since a person's second-to-last update.
    /// (Second-to-last because the most recent is the one just submitted.)
    pub fn get_updates_from_others(
        &self,
        room_id: &str,
        employee_name: &str,
        limit: u32,
    ) -> Result<Vec<StoredProgressNote>> {
        let conn = self.conn.lock().unwrap();
        // Get second-to-last update time for this person
        let cutoff: Option<String> = conn
            .query_row(
                "SELECT received_at FROM notes
                 WHERE room_id = ?1 AND employee_name = ?2
                 ORDER BY received_at DESC LIMIT 1 OFFSET 1",
                params![room_id, employee_name],
                |row| row.get(0),
            )
            .optional()?;

        let (sql, param_values): (String, Vec<String>) = if let Some(ref t) = cutoff {
            (
                format!(
                    "SELECT note_id, employee_name, repo, branch, progress_text, received_at
                     FROM notes WHERE room_id = ?1 AND employee_name != ?2 AND received_at > ?3
                     ORDER BY received_at DESC LIMIT ?4"
                ),
                vec![
                    room_id.to_string(),
                    employee_name.to_string(),
                    t.clone(),
                    limit.to_string(),
                ],
            )
        } else {
            (
                format!(
                    "SELECT note_id, employee_name, repo, branch, progress_text, received_at
                     FROM notes WHERE room_id = ?1 AND employee_name != ?2
                     ORDER BY received_at DESC LIMIT ?3"
                ),
                vec![
                    room_id.to_string(),
                    employee_name.to_string(),
                    limit.to_string(),
                ],
            )
        };

        let param_refs: Vec<&dyn rusqlite::types::ToSql> = param_values
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();
        let mut stmt = conn.prepare(&sql)?;
        let notes = stmt
            .query_map(rusqlite::params_from_iter(param_refs.iter()), |row| {
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

    /// Get notes with optional filters: time cutoff, employee name, limit.
    pub fn get_notes_filtered(
        &self,
        room_id: &str,
        after_time: Option<&str>,
        employee_name: Option<&str>,
        branch: Option<&str>,
        limit: u32,
    ) -> Result<Vec<StoredProgressNote>> {
        let conn = self.conn.lock().unwrap();

        let mut conditions = vec!["room_id = ?1".to_string()];
        let mut param_values: Vec<String> = vec![room_id.to_string()];
        let mut idx = 2u32;

        if let Some(time) = after_time {
            conditions.push(format!("received_at > ?{idx}"));
            param_values.push(time.to_string());
            idx += 1;
        }

        if let Some(name) = employee_name {
            conditions.push(format!("employee_name = ?{idx}"));
            param_values.push(name.to_string());
            idx += 1;
        }

        if let Some(b) = branch {
            conditions.push(format!("branch = ?{idx}"));
            param_values.push(b.to_string());
            idx += 1;
        }

        let where_clause = conditions.join(" AND ");
        let sql = format!(
            "SELECT note_id, employee_name, repo, branch, progress_text, received_at
             FROM notes WHERE {where_clause}
             ORDER BY received_at DESC LIMIT ?{idx}"
        );
        param_values.push(limit.to_string());

        let param_refs: Vec<&dyn rusqlite::types::ToSql> = param_values
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();

        let mut stmt = conn.prepare(&sql)?;
        let notes = stmt
            .query_map(rusqlite::params_from_iter(param_refs.iter()), |row| {
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

    /// Get the summary generation status for a room.
    pub fn get_summary_status(&self, room_id: &str) -> Result<String> {
        let conn = self.conn.lock().unwrap();
        let result: Option<String> = conn
            .query_row(
                "SELECT status FROM summaries WHERE room_id = ?1",
                params![room_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(result.unwrap_or_else(|| "ready".to_owned()))
    }

    /// Set just the summary status (generating/ready/error).
    pub fn set_summary_status(&self, room_id: &str, status: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO summaries (room_id, content_markdown, status, updated_at)
             VALUES (?1, '', ?2, ?3)
             ON CONFLICT(room_id) DO UPDATE SET
                status     = excluded.status,
                updated_at = excluded.updated_at",
            params![room_id, status, now_rfc3339()],
        )?;
        Ok(())
    }

    /// Create or replace the manager summary for a room.
    pub fn set_summary(&self, room_id: &str, content: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO summaries (room_id, content_markdown, status, updated_at)
             VALUES (?1, ?2, 'ready', ?3)
             ON CONFLICT(room_id) DO UPDATE SET
                content_markdown = excluded.content_markdown,
                status           = 'ready',
                updated_at       = excluded.updated_at",
            params![room_id, content, now_rfc3339()],
        )?;
        Ok(())
    }
    // ── Tasks ───────────────────────────────────────────────

    pub fn create_task(
        &self,
        room_id: &str,
        title: &str,
        assignee: Option<&str>,
    ) -> Result<String> {
        let conn = self.conn.lock().unwrap();
        let task_id = Uuid::new_v4().to_string();
        let now = now_rfc3339();
        conn.execute(
            "INSERT INTO tasks (task_id, room_id, title, status, assignee, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'todo', ?4, ?5, ?5)",
            params![task_id, room_id, title, assignee, now],
        )?;
        Ok(task_id)
    }

    pub fn get_tasks(&self, room_id: &str, include_done: bool) -> Result<Vec<serde_json::Value>> {
        let conn = self.conn.lock().unwrap();
        let sql = if include_done {
            "SELECT task_id, title, status, assignee, created_at, updated_at
             FROM tasks WHERE room_id = ?1 ORDER BY created_at"
        } else {
            "SELECT task_id, title, status, assignee, created_at, updated_at
             FROM tasks WHERE room_id = ?1 AND status != 'done' ORDER BY created_at"
        };
        let mut stmt = conn.prepare(sql)?;
        let tasks = stmt
            .query_map(params![room_id], |row| {
                Ok(serde_json::json!({
                    "task_id": row.get::<_, String>(0)?,
                    "title": row.get::<_, String>(1)?,
                    "status": row.get::<_, String>(2)?,
                    "assignee": row.get::<_, Option<String>>(3)?,
                    "created_at": row.get::<_, String>(4)?,
                    "updated_at": row.get::<_, String>(5)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(tasks)
    }

    pub fn update_task(
        &self,
        room_id: &str,
        task_id: &str,
        title: Option<&str>,
        status: Option<&str>,
        assignee: Option<&str>,
    ) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let mut sets = vec!["updated_at = ?3".to_string()];
        let mut param_values: Vec<String> =
            vec![task_id.to_string(), room_id.to_string(), now_rfc3339()];
        let mut idx = 4u32;

        if let Some(t) = title {
            sets.push(format!("title = ?{idx}"));
            param_values.push(t.to_string());
            idx += 1;
        }
        if let Some(s) = status {
            sets.push(format!("status = ?{idx}"));
            param_values.push(s.to_string());
            idx += 1;
        }
        if let Some(a) = assignee {
            sets.push(format!("assignee = ?{idx}"));
            param_values.push(a.to_string());
        }

        let sql = format!(
            "UPDATE tasks SET {} WHERE task_id = ?1 AND room_id = ?2",
            sets.join(", ")
        );
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = param_values
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();
        let changed = conn.execute(&sql, rusqlite::params_from_iter(param_refs.iter()))?;
        Ok(changed > 0)
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
