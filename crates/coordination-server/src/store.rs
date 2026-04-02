use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result};
use rand::Rng;
use reporter_protocol::{HookTurnReport, Room, StoredHookEvent};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
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

            CREATE TABLE IF NOT EXISTS hook_events (
                event_id       TEXT PRIMARY KEY,
                room_id        TEXT NOT NULL REFERENCES rooms(room_id),
                employee_name  TEXT NOT NULL,
                client         TEXT NOT NULL,
                event_name     TEXT NOT NULL,
                session_id     TEXT NOT NULL,
                turn_id        TEXT,
                repo_root      TEXT NOT NULL,
                cwd            TEXT,
                branch         TEXT,
                content        TEXT NOT NULL,
                payload_json   TEXT,
                received_at    TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_tasks_room
                ON tasks(room_id);
            CREATE INDEX IF NOT EXISTS idx_hook_events_room_received
                ON hook_events(room_id, received_at);",
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

        let has_payload_json: bool = conn
            .prepare("SELECT sql FROM sqlite_master WHERE type='table' AND name='hook_events'")?
            .query_row([], |row| row.get::<_, String>(0))
            .map(|sql| sql.contains("payload_json"))
            .unwrap_or(false);
        if !has_payload_json {
            conn.execute_batch("ALTER TABLE hook_events ADD COLUMN payload_json TEXT;")?;
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

    /// Insert a raw hook turn event into a room.
    pub fn insert_hook_event(
        &self,
        room_id: &str,
        report: &HookTurnReport,
    ) -> Result<StoredHookEvent> {
        let conn = self.conn.lock().unwrap();
        let event_id = Uuid::new_v4();
        let received_at = now_rfc3339();
        let payload_json =
            serde_json::to_string(&report.payload).context("failed to serialize hook payload")?;

        conn.execute(
            "INSERT INTO hook_events (
                event_id, room_id, employee_name, client, event_name, session_id, turn_id,
                repo_root, cwd, branch, content, payload_json, received_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                event_id.to_string(),
                room_id,
                report.employee_name,
                report.client,
                "",
                "",
                Option::<String>::None,
                report.repo_root,
                report.branch,
                Option::<String>::None,
                payload_json.clone(),
                payload_json,
                received_at,
            ],
        )
        .with_context(|| format!("failed to insert hook event into room {room_id}"))?;

        Ok(StoredHookEvent {
            event_id,
            received_at,
            employee_name: report.employee_name.clone(),
            client: report.client.clone(),
            repo_root: report.repo_root.clone(),
            branch: report.branch.clone(),
            payload: report.payload.clone(),
        })
    }

    pub fn get_hook_events(&self, room_id: &str) -> Result<Vec<StoredHookEvent>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT event_id, employee_name, client, repo_root, branch, payload_json,
                    event_name, session_id, turn_id, cwd, content, received_at
             FROM hook_events WHERE room_id = ?1
             ORDER BY received_at DESC",
        )?;

        let events = stmt
            .query_map(params![room_id], map_stored_hook_event)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(events)
    }

    pub fn get_hook_events_after(
        &self,
        room_id: &str,
        after_event_id: &str,
    ) -> Result<Vec<StoredHookEvent>> {
        let conn = self.conn.lock().unwrap();

        let anchor_time: Option<String> = conn
            .query_row(
                "SELECT received_at FROM hook_events WHERE event_id = ?1 AND room_id = ?2",
                params![after_event_id, room_id],
                |row| row.get(0),
            )
            .optional()?;

        let anchor_time = match anchor_time {
            Some(t) => t,
            None => return Ok(Vec::new()),
        };

        let mut stmt = conn.prepare(
            "SELECT event_id, employee_name, client, repo_root, branch, payload_json,
                    event_name, session_id, turn_id, cwd, content, received_at
             FROM hook_events
             WHERE room_id = ?1 AND received_at > ?2
             ORDER BY received_at ASC",
        )?;

        let events = stmt
            .query_map(params![room_id, anchor_time], map_stored_hook_event)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(events)
    }

    pub fn get_last_hook_event_time(
        &self,
        room_id: &str,
        employee_name: &str,
    ) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let result: Option<String> = conn
            .query_row(
                "SELECT received_at FROM hook_events
                 WHERE room_id = ?1 AND employee_name = ?2
                 ORDER BY received_at DESC LIMIT 1",
                params![room_id, employee_name],
                |row| row.get(0),
            )
            .optional()?;
        Ok(result)
    }

    pub fn get_hook_events_filtered(
        &self,
        room_id: &str,
        after_time: Option<&str>,
        employee_name: Option<&str>,
        branch: Option<&str>,
        limit: u32,
    ) -> Result<Vec<StoredHookEvent>> {
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
            "SELECT event_id, employee_name, client, repo_root, branch, payload_json,
                    event_name, session_id, turn_id, cwd, content, received_at
             FROM hook_events WHERE {where_clause}
             ORDER BY received_at DESC LIMIT ?{idx}"
        );
        param_values.push(limit.to_string());

        let param_refs: Vec<&dyn rusqlite::types::ToSql> = param_values
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();

        let mut stmt = conn.prepare(&sql)?;
        let events = stmt
            .query_map(
                rusqlite::params_from_iter(param_refs.iter()),
                map_stored_hook_event,
            )?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(events)
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
}

// ── Helpers ─────────────────────────────────────────────────

fn map_stored_hook_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredHookEvent> {
    let event_id_str: String = row.get(0)?;
    let payload_json: Option<String> = row.get(5)?;
    let payload = parse_or_reconstruct_payload(
        payload_json.as_deref(),
        row.get::<_, String>(6)?.as_str(),
        row.get::<_, String>(7)?.as_str(),
        row.get::<_, Option<String>>(8)?,
        row.get::<_, Option<String>>(9)?,
        row.get::<_, String>(10)?.as_str(),
    );

    Ok(StoredHookEvent {
        event_id: Uuid::parse_str(&event_id_str).unwrap_or_else(|_| Uuid::nil()),
        received_at: row.get(11)?,
        employee_name: row.get(1)?,
        client: row.get(2)?,
        repo_root: row.get(3)?,
        branch: row.get(4)?,
        payload,
    })
}

fn parse_or_reconstruct_payload(
    payload_json: Option<&str>,
    event_name: &str,
    session_id: &str,
    turn_id: Option<String>,
    cwd: Option<String>,
    content: &str,
) -> Value {
    if let Some(raw) = payload_json {
        if let Ok(value) = serde_json::from_str::<Value>(raw) {
            return value;
        }
    }

    let hook_event_name = match event_name {
        "user_prompt_submit" => "UserPromptSubmit",
        "stop" => "Stop",
        other if !other.is_empty() => other,
        _ => "Unknown",
    };

    let mut payload = json!({
        "hook_event_name": hook_event_name,
    });

    if !session_id.is_empty() {
        payload["session_id"] = Value::String(session_id.to_owned());
    }
    if let Some(turn_id) = turn_id {
        payload["turn_id"] = Value::String(turn_id);
    }
    if let Some(cwd) = cwd {
        payload["cwd"] = Value::String(cwd);
    }
    if !content.is_empty() {
        let key = if event_name == "user_prompt_submit" {
            "prompt"
        } else if event_name == "stop" {
            "last_assistant_message"
        } else {
            "content"
        };
        payload[key] = Value::String(content.to_owned());
    }

    payload
}

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
