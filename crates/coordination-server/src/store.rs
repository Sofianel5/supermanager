use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result};
use rand::Rng;
use reporter_protocol::{HookTurnReport, Room, RoomSnapshot, StoredHookEvent};
use rusqlite::{Connection, params};
use serde_json::Value;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

const ROOM_CODE_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
const ROOM_CODE_LENGTH: usize = 6;

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
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS summaries (
                room_id      TEXT PRIMARY KEY REFERENCES rooms(room_id),
                content_json TEXT NOT NULL,
                thread_id    TEXT,
                status       TEXT NOT NULL DEFAULT 'ready',
                updated_at   TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS hook_events (
                seq            INTEGER PRIMARY KEY AUTOINCREMENT,
                event_id       TEXT NOT NULL UNIQUE,
                room_id        TEXT NOT NULL REFERENCES rooms(room_id),
                employee_name  TEXT NOT NULL,
                client         TEXT NOT NULL,
                repo_root      TEXT NOT NULL,
                branch         TEXT,
                payload_json   TEXT,
                received_at    TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_hook_events_room_seq
                ON hook_events(room_id, seq);",
        )?;

        if table_has_column(&conn, "rooms", "secret")? {
            migrate_rooms_table_without_secret(&conn)?;
        }

        // ── Migrations for existing DBs ────────────────────────
        migrate_summaries_table(&conn)?;

        let has_payload_json: bool = conn
            .prepare("SELECT sql FROM sqlite_master WHERE type='table' AND name='hook_events'")?
            .query_row([], |row| row.get::<_, String>(0))
            .map(|sql| sql.contains("payload_json"))
            .unwrap_or(false);
        if !has_payload_json {
            conn.execute_batch("ALTER TABLE hook_events ADD COLUMN payload_json TEXT;")?;
        }

        let has_seq: bool = conn
            .prepare("SELECT sql FROM sqlite_master WHERE type='table' AND name='hook_events'")?
            .query_row([], |row| row.get::<_, String>(0))
            .map(|sql| sql.contains("seq INTEGER PRIMARY KEY"))
            .unwrap_or(false);
        if !has_seq {
            conn.execute_batch(
                "CREATE TABLE hook_events_new (
                    seq            INTEGER PRIMARY KEY AUTOINCREMENT,
                    event_id       TEXT NOT NULL UNIQUE,
                    room_id        TEXT NOT NULL REFERENCES rooms(room_id),
                    employee_name  TEXT NOT NULL,
                    client         TEXT NOT NULL,
                    repo_root      TEXT NOT NULL,
                    branch         TEXT,
                    payload_json   TEXT,
                    received_at    TEXT NOT NULL
                 );
                 INSERT INTO hook_events_new
                     (event_id, room_id, employee_name, client, repo_root, branch, payload_json, received_at)
                 SELECT event_id, room_id, employee_name, client, repo_root, branch, payload_json, received_at
                 FROM hook_events
                 ORDER BY received_at ASC;
                 DROP INDEX IF EXISTS idx_hook_events_room_received;
                 DROP TABLE hook_events;
                 ALTER TABLE hook_events_new RENAME TO hook_events;
                 CREATE INDEX IF NOT EXISTS idx_hook_events_room_seq
                     ON hook_events(room_id, seq);",
            )?;
        }

        conn.execute_batch(
            "DROP INDEX IF EXISTS idx_tasks_room;
             DROP TABLE IF EXISTS tasks;",
        )?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    // ── Rooms ───────────────────────────────────────────────

    /// Create a new room with an auto-generated 6-character code.
    pub fn create_room(&self, name: &str) -> Result<Room> {
        let conn = self.conn.lock().unwrap();
        let mut rng = rand::rng();
        let created_at = now_rfc3339();

        // Try generating a unique room code (retry on collision).
        let room_id = {
            let max_attempts = 10;
            let mut code = None;
            for _ in 0..max_attempts {
                let candidate = generate_room_code(&mut rng);
                let exists: bool = conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM rooms WHERE room_id = ?1 COLLATE NOCASE)",
                    params![candidate],
                    |row| row.get(0),
                )?;
                if !exists {
                    code = Some(candidate);
                    break;
                }
            }
            code.ok_or_else(|| {
                anyhow::anyhow!("failed to generate unique room code after {max_attempts} attempts")
            })?
        };

        conn.execute(
            "INSERT INTO rooms (room_id, name, created_at)
             VALUES (?1, ?2, ?3)",
            params![room_id, name, created_at],
        )?;

        Ok(Room {
            room_id,
            name: name.to_owned(),
            created_at,
        })
    }

    /// Retrieve a room by its code, or `None` if it does not exist.
    pub fn get_room(&self, room_id: &str) -> Result<Option<Room>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT room_id, name, created_at FROM rooms WHERE room_id = ?1 COLLATE NOCASE",
        )?;
        let mut rows = stmt.query_map(params![room_id], |row| {
            Ok(Room {
                room_id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
            })
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
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
                event_id, room_id, employee_name, client, repo_root, branch, payload_json,
                received_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                event_id.to_string(),
                room_id,
                report.employee_name,
                report.client,
                report.repo_root,
                report.branch,
                payload_json,
                received_at,
            ],
        )
        .with_context(|| format!("failed to insert hook event into room {room_id}"))?;

        let seq = conn.last_insert_rowid();

        Ok(StoredHookEvent {
            seq,
            event_id,
            received_at,
            employee_name: report.employee_name.clone(),
            client: report.client.clone(),
            repo_root: report.repo_root.clone(),
            branch: report.branch.clone(),
            payload: report.payload.clone(),
        })
    }

    /// Fetch hook events for a room, ordered newest-first (seq DESC).
    ///
    /// `before`/`after` are exclusive seq bounds; pass `None` to leave that
    /// side unbounded. `limit` caps the returned page; `None` = unbounded.
    pub fn get_hook_events(
        &self,
        room_id: &str,
        before: Option<i64>,
        after: Option<i64>,
        limit: Option<i64>,
    ) -> Result<Vec<StoredHookEvent>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT seq, event_id, employee_name, client, repo_root, branch, payload_json, received_at
             FROM hook_events
             WHERE room_id = ?1
               AND (?2 IS NULL OR seq < ?2)
               AND (?3 IS NULL OR seq > ?3)
             ORDER BY seq DESC
             LIMIT ?4",
        )?;

        let events = stmt
            .query_map(
                params![room_id, before, after, limit.unwrap_or(-1)],
                map_stored_hook_event,
            )?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(events)
    }

    // ── Summaries ───────────────────────────────────────────

    /// Get the manager summary for a room.
    pub fn get_summary(&self, room_id: &str) -> Result<RoomSnapshot> {
        let conn = self.conn.lock().unwrap();
        let result: Option<String> = conn
            .query_row(
                "SELECT content_json FROM summaries WHERE room_id = ?1",
                params![room_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(result
            .as_deref()
            .and_then(|raw| serde_json::from_str::<RoomSnapshot>(raw).ok())
            .unwrap_or_default())
    }

    pub fn get_summary_thread_id(&self, room_id: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let result = conn
            .query_row(
                "SELECT thread_id FROM summaries WHERE room_id = ?1",
                params![room_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?;
        Ok(result.flatten())
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
            "INSERT INTO summaries (room_id, content_json, thread_id, status, updated_at)
             VALUES (?1, ?2, NULL, ?3, ?4)
             ON CONFLICT(room_id) DO UPDATE SET
                status     = excluded.status,
                updated_at = excluded.updated_at",
            params![room_id, empty_room_summary_json(), status, now_rfc3339()],
        )?;
        Ok(())
    }

    /// Create or replace the manager summary for a room.
    pub fn set_summary(&self, room_id: &str, content: &RoomSnapshot) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let content_json =
            serde_json::to_string(content).context("failed to serialize room summary")?;
        conn.execute(
            "INSERT INTO summaries (room_id, content_json, thread_id, status, updated_at)
             VALUES (?1, ?2, NULL, 'ready', ?3)
             ON CONFLICT(room_id) DO UPDATE SET
                content_json = excluded.content_json,
                status       = 'ready',
                updated_at   = excluded.updated_at",
            params![room_id, content_json, now_rfc3339()],
        )?;
        Ok(())
    }

    pub fn set_summary_thread_id(&self, room_id: &str, thread_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO summaries (room_id, content_json, thread_id, status, updated_at)
             VALUES (?1, ?2, ?3, 'ready', ?4)
             ON CONFLICT(room_id) DO UPDATE SET
                thread_id   = excluded.thread_id,
                updated_at  = excluded.updated_at",
            params![room_id, empty_room_summary_json(), thread_id, now_rfc3339()],
        )?;
        Ok(())
    }
}

// ── Helpers ─────────────────────────────────────────────────

fn map_stored_hook_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredHookEvent> {
    let event_id_str: String = row.get(1)?;
    let payload = row
        .get::<_, Option<String>>(6)?
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .unwrap_or(Value::Null);

    Ok(StoredHookEvent {
        seq: row.get(0)?,
        event_id: Uuid::parse_str(&event_id_str).unwrap_or_else(|_| Uuid::nil()),
        received_at: row.get(7)?,
        employee_name: row.get(2)?,
        client: row.get(3)?,
        repo_root: row.get(4)?,
        branch: row.get(5)?,
        payload,
    })
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

fn table_has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn migrate_rooms_table_without_secret(conn: &Connection) -> Result<()> {
    conn.execute_batch("PRAGMA foreign_keys=OFF;")?;

    let migration_result = conn.execute_batch(
        "DROP TABLE IF EXISTS rooms_new;
         CREATE TABLE rooms_new (
            room_id    TEXT PRIMARY KEY,
            name       TEXT NOT NULL,
            created_at TEXT NOT NULL
         );
         INSERT INTO rooms_new (room_id, name, created_at)
            SELECT room_id, name, created_at FROM rooms;
         DROP TABLE rooms;
         ALTER TABLE rooms_new RENAME TO rooms;",
    );

    let restore_result = conn.execute_batch("PRAGMA foreign_keys=ON;");

    migration_result?;
    restore_result?;
    ensure_no_foreign_key_violations(conn)?;
    Ok(())
}

fn migrate_summaries_table(conn: &Connection) -> Result<()> {
    let has_content_json = table_has_column(conn, "summaries", "content_json")?;
    let has_content_markdown = table_has_column(conn, "summaries", "content_markdown")?;
    let has_thread_id = table_has_column(conn, "summaries", "thread_id")?;
    if has_content_json && !has_content_markdown {
        if !has_thread_id {
            conn.execute_batch("ALTER TABLE summaries ADD COLUMN thread_id TEXT;")?;
        }
        return Ok(());
    }

    let has_status = table_has_column(conn, "summaries", "status")?;
    let empty_summary = empty_room_summary_json();

    conn.execute_batch("PRAGMA foreign_keys=OFF;")?;

    let migration_result = (|| -> Result<()> {
        conn.execute_batch(
            "ALTER TABLE summaries RENAME TO summaries_old;
             CREATE TABLE summaries (
                room_id      TEXT PRIMARY KEY REFERENCES rooms(room_id),
                content_json TEXT NOT NULL,
                thread_id    TEXT,
                status       TEXT NOT NULL DEFAULT 'ready',
                updated_at   TEXT NOT NULL
             );",
        )?;

        if has_status {
            conn.execute(
                "INSERT INTO summaries (room_id, content_json, status, updated_at)
                 SELECT room_id, ?1, status, updated_at FROM summaries_old",
                params![empty_summary],
            )?;
        } else {
            conn.execute(
                "INSERT INTO summaries (room_id, content_json, status, updated_at)
                 SELECT room_id, ?1, 'ready', updated_at FROM summaries_old",
                params![empty_summary],
            )?;
        }

        conn.execute_batch("DROP TABLE summaries_old;")?;
        Ok(())
    })();

    let restore_result = conn.execute_batch("PRAGMA foreign_keys=ON;");

    migration_result?;
    restore_result?;
    ensure_no_foreign_key_violations(conn)?;
    Ok(())
}

fn ensure_no_foreign_key_violations(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare("PRAGMA foreign_key_check")?;
    let mut rows = stmt.query([])?;

    if let Some(row) = rows.next()? {
        let table: String = row.get(0)?;
        let row_id: i64 = row.get(1)?;
        let parent: String = row.get(2)?;
        let fk_index: i64 = row.get(3)?;
        anyhow::bail!(
            "foreign key violation after rooms migration: table={table} row_id={row_id} parent={parent} fk_index={fk_index}"
        );
    }

    Ok(())
}

fn generate_room_code(rng: &mut impl Rng) -> String {
    (0..ROOM_CODE_LENGTH)
        .map(|_| {
            let idx = rng.random_range(0..ROOM_CODE_ALPHABET.len());
            ROOM_CODE_ALPHABET[idx] as char
        })
        .collect()
}

pub fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| OffsetDateTime::now_utc().unix_timestamp().to_string())
}

fn empty_room_summary_json() -> String {
    serde_json::to_string(&RoomSnapshot::default()).unwrap_or_else(|_| {
        "{\"bluf_markdown\":\"\",\"overview_markdown\":\"\",\"employees\":[]}".to_owned()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::TempDir;

    #[test]
    fn summary_defaults_to_empty_payload() {
        let tempdir = TempDir::new().unwrap();
        let db = Db::open(&tempdir.path().join("summary-default.sqlite")).unwrap();
        let room = db.create_room("Test Room").unwrap();

        assert_eq!(
            db.get_summary(&room.room_id).unwrap(),
            RoomSnapshot::default()
        );
        assert_eq!(db.get_summary_status(&room.room_id).unwrap(), "ready");
    }

    #[test]
    fn set_summary_status_persists_empty_json_summary() {
        let tempdir = TempDir::new().unwrap();
        let db = Db::open(&tempdir.path().join("summary-status.sqlite")).unwrap();
        let room = db.create_room("Test Room").unwrap();

        db.set_summary_status(&room.room_id, "generating").unwrap();

        assert_eq!(
            db.get_summary(&room.room_id).unwrap(),
            RoomSnapshot::default()
        );
        assert_eq!(db.get_summary_status(&room.room_id).unwrap(), "generating");
    }

    #[test]
    fn summary_thread_id_round_trips_and_survives_status_updates() {
        let tempdir = TempDir::new().unwrap();
        let db = Db::open(&tempdir.path().join("summary-thread.sqlite")).unwrap();
        let room = db.create_room("Test Room").unwrap();

        db.set_summary_thread_id(&room.room_id, "thread_123")
            .unwrap();
        db.set_summary_status(&room.room_id, "generating").unwrap();

        assert_eq!(
            db.get_summary_thread_id(&room.room_id).unwrap(),
            Some("thread_123".to_owned())
        );
        assert_eq!(db.get_summary_status(&room.room_id).unwrap(), "generating");
    }

    #[test]
    fn open_migrates_markdown_summaries_to_empty_json_payload() {
        let tempdir = TempDir::new().unwrap();
        let db_path = tempdir.path().join("summary-migration.sqlite");

        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "PRAGMA foreign_keys=ON;
             CREATE TABLE rooms (
                room_id    TEXT PRIMARY KEY,
                name       TEXT NOT NULL,
                created_at TEXT NOT NULL
             );
             CREATE TABLE summaries (
                room_id          TEXT PRIMARY KEY REFERENCES rooms(room_id),
                content_markdown TEXT NOT NULL,
                status           TEXT NOT NULL DEFAULT 'ready',
                updated_at       TEXT NOT NULL
             );
             INSERT INTO rooms (room_id, name, created_at)
                VALUES ('ROOM01', 'Migrated Room', '2026-04-01T09:00:00Z');
             INSERT INTO summaries (room_id, content_markdown, status, updated_at)
                VALUES ('ROOM01', '# legacy markdown', 'ready', '2026-04-01T10:00:00Z');",
        )
        .unwrap();
        drop(conn);

        let db = Db::open(&db_path).unwrap();

        assert_eq!(db.get_summary("ROOM01").unwrap(), RoomSnapshot::default());
        assert_eq!(db.get_summary_status("ROOM01").unwrap(), "ready");

        let conn = Connection::open(&db_path).unwrap();
        assert!(table_has_column(&conn, "summaries", "content_json").unwrap());
        assert!(table_has_column(&conn, "summaries", "thread_id").unwrap());
        assert!(!table_has_column(&conn, "summaries", "content_markdown").unwrap());
    }
}
