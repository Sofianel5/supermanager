use anyhow::{Context, Result};
use rand::Rng;
use reporter_protocol::{HookTurnReport, Room, RoomSnapshot, StoredHookEvent};
use serde_json::Value;
use sqlx::{
    PgPool, Row,
    migrate::Migrator,
    postgres::{PgPoolOptions, PgRow},
    types::Json,
};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

const ROOM_CODE_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
const ROOM_CODE_LENGTH: usize = 6;

static MIGRATOR: Migrator = sqlx::migrate!();

#[derive(Clone)]
pub struct Db {
    pool: PgPool,
}

impl Db {
    pub async fn connect(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await
            .context("failed to connect to PostgreSQL")?;

        MIGRATOR
            .run(&pool)
            .await
            .context("failed to run PostgreSQL migrations")?;

        Ok(Self { pool })
    }

    pub async fn ping(&self) -> Result<()> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .context("failed database health check")?;
        Ok(())
    }

    pub async fn create_room(&self, name: &str) -> Result<Room> {
        for _ in 0..10 {
            let room_id = {
                let mut rng = rand::rng();
                generate_room_code(&mut rng)
            };

            let insert = sqlx::query(
                "INSERT INTO rooms (room_id, name)
                 VALUES ($1, $2)
                 RETURNING created_at",
            )
            .bind(&room_id)
            .bind(name)
            .fetch_one(&self.pool)
            .await;

            match insert {
                Ok(row) => {
                    let created_at: OffsetDateTime = row
                        .try_get("created_at")
                        .context("failed to decode room created_at")?;
                    return Ok(Room {
                        room_id,
                        name: name.to_owned(),
                        created_at: format_rfc3339(created_at),
                    });
                }
                Err(error) if is_unique_violation(&error) => continue,
                Err(error) => {
                    return Err(error).context("failed to insert room into PostgreSQL");
                }
            }
        }

        anyhow::bail!("failed to generate unique room code after 10 attempts")
    }

    pub async fn get_room(&self, room_id: &str) -> Result<Option<Room>> {
        let row = sqlx::query(
            "SELECT room_id, name, created_at
             FROM rooms
             WHERE room_id = $1",
        )
        .bind(normalize_room_id(room_id))
        .fetch_optional(&self.pool)
        .await
        .with_context(|| format!("failed to fetch room {room_id}"))?;

        row.map(map_room).transpose()
    }

    pub async fn insert_hook_event(
        &self,
        room_id: &str,
        report: &HookTurnReport,
    ) -> Result<StoredHookEvent> {
        let room_id = normalize_room_id(room_id);
        let event_id = Uuid::new_v4();
        let row = sqlx::query(
            "INSERT INTO hook_events (
                event_id,
                room_id,
                employee_name,
                client,
                repo_root,
                branch,
                payload_json
             )
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             RETURNING seq, received_at",
        )
        .bind(event_id)
        .bind(&room_id)
        .bind(&report.employee_name)
        .bind(&report.client)
        .bind(&report.repo_root)
        .bind(report.branch.as_deref())
        .bind(Json(report.payload.clone()))
        .fetch_one(&self.pool)
        .await
        .with_context(|| format!("failed to insert hook event into room {room_id}"))?;
        let seq: i64 = row
            .try_get("seq")
            .context("failed to decode hook event seq")?;
        let received_at: OffsetDateTime = row
            .try_get("received_at")
            .context("failed to decode hook event received_at")?;

        Ok(StoredHookEvent {
            seq,
            event_id,
            received_at: format_rfc3339(received_at),
            employee_name: report.employee_name.clone(),
            client: report.client.clone(),
            repo_root: report.repo_root.clone(),
            branch: report.branch.clone(),
            payload: report.payload.clone(),
        })
    }

    pub async fn get_hook_events(
        &self,
        room_id: &str,
        before: Option<i64>,
        after: Option<i64>,
        limit: Option<i64>,
    ) -> Result<Vec<StoredHookEvent>> {
        let rows = sqlx::query(
            "SELECT
                seq,
                event_id,
                employee_name,
                client,
                repo_root,
                branch,
                payload_json,
                received_at
             FROM hook_events
             WHERE room_id = $1
               AND ($2::bigint IS NULL OR seq < $2)
               AND ($3::bigint IS NULL OR seq > $3)
             ORDER BY seq DESC
             LIMIT $4",
        )
        .bind(normalize_room_id(room_id))
        .bind(before)
        .bind(after)
        .bind(limit.unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .with_context(|| format!("failed to fetch hook events for room {room_id}"))?;

        rows.into_iter().map(map_stored_hook_event).collect()
    }

    pub async fn get_summary(&self, room_id: &str) -> Result<RoomSnapshot> {
        let row = sqlx::query(
            "SELECT content_json
             FROM summaries
             WHERE room_id = $1",
        )
        .bind(normalize_room_id(room_id))
        .fetch_optional(&self.pool)
        .await
        .with_context(|| format!("failed to fetch summary for room {room_id}"))?;

        Ok(match row {
            Some(row) => {
                row.try_get::<Json<RoomSnapshot>, _>("content_json")
                    .context("failed to decode room summary JSON")?
                    .0
            }
            None => RoomSnapshot::default(),
        })
    }

    pub async fn get_summary_thread_id(&self, room_id: &str) -> Result<Option<String>> {
        let row = sqlx::query(
            "SELECT thread_id
             FROM summaries
             WHERE room_id = $1",
        )
        .bind(normalize_room_id(room_id))
        .fetch_optional(&self.pool)
        .await
        .with_context(|| format!("failed to fetch summary thread id for room {room_id}"))?;

        row.map(|row| row.try_get("thread_id"))
            .transpose()
            .context("failed to decode summary thread id")
    }

    pub async fn get_summary_status(&self, room_id: &str) -> Result<String> {
        let row = sqlx::query(
            "SELECT status
             FROM summaries
             WHERE room_id = $1",
        )
        .bind(normalize_room_id(room_id))
        .fetch_optional(&self.pool)
        .await
        .with_context(|| format!("failed to fetch summary status for room {room_id}"))?;

        Ok(row
            .map(|row| row.try_get("status"))
            .transpose()
            .context("failed to decode summary status")?
            .unwrap_or_else(|| "ready".to_owned()))
    }

    pub async fn set_summary_status(&self, room_id: &str, status: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO summaries (room_id, content_json, thread_id, status, updated_at)
             VALUES ($1, $2, NULL, $3, NOW())
             ON CONFLICT(room_id) DO UPDATE SET
                status = EXCLUDED.status,
                updated_at = EXCLUDED.updated_at",
        )
        .bind(normalize_room_id(room_id))
        .bind(Json(RoomSnapshot::default()))
        .bind(status)
        .execute(&self.pool)
        .await
        .with_context(|| format!("failed to persist summary status for room {room_id}"))?;

        Ok(())
    }
    pub async fn set_summary(&self, room_id: &str, content: &RoomSnapshot) -> Result<()> {
        sqlx::query(
            "INSERT INTO summaries (room_id, content_json, thread_id, status, updated_at)
             VALUES ($1, $2, NULL, 'ready', NOW())
             ON CONFLICT(room_id) DO UPDATE SET
                content_json = EXCLUDED.content_json,
                status = 'ready',
                updated_at = EXCLUDED.updated_at",
        )
        .bind(normalize_room_id(room_id))
        .bind(Json(content.clone()))
        .execute(&self.pool)
        .await
        .with_context(|| format!("failed to persist summary for room {room_id}"))?;

        Ok(())
    }

    pub async fn set_summary_thread_id(&self, room_id: &str, thread_id: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO summaries (room_id, content_json, thread_id, status, updated_at)
             VALUES ($1, $2, $3, 'ready', NOW())
             ON CONFLICT(room_id) DO UPDATE SET
                thread_id = EXCLUDED.thread_id,
                updated_at = EXCLUDED.updated_at",
        )
        .bind(normalize_room_id(room_id))
        .bind(Json(RoomSnapshot::default()))
        .bind(thread_id)
        .execute(&self.pool)
        .await
        .with_context(|| format!("failed to persist summary thread id for room {room_id}"))?;

        Ok(())
    }
}

pub fn normalize_room_id(room_id: &str) -> String {
    room_id.trim().to_ascii_uppercase()
}

fn map_room(row: PgRow) -> Result<Room> {
    Ok(Room {
        room_id: row.try_get("room_id")?,
        name: row.try_get("name")?,
        created_at: format_rfc3339(row.try_get("created_at")?),
    })
}

fn map_stored_hook_event(row: PgRow) -> Result<StoredHookEvent> {
    Ok(StoredHookEvent {
        seq: row.try_get("seq")?,
        event_id: row.try_get("event_id")?,
        received_at: format_rfc3339(row.try_get("received_at")?),
        employee_name: row.try_get("employee_name")?,
        client: row.try_get("client")?,
        repo_root: row.try_get("repo_root")?,
        branch: row.try_get("branch")?,
        payload: row.try_get::<Json<Value>, _>("payload_json")?.0,
    })
}

fn generate_room_code(rng: &mut impl Rng) -> String {
    (0..ROOM_CODE_LENGTH)
        .map(|_| {
            let index = rng.random_range(0..ROOM_CODE_ALPHABET.len());
            ROOM_CODE_ALPHABET[index] as char
        })
        .collect()
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    matches!(
        error,
        sqlx::Error::Database(db_error)
            if db_error.code().as_deref() == Some("23505")
    )
}

pub fn now_rfc3339() -> String {
    format_rfc3339(OffsetDateTime::now_utc())
}

fn format_rfc3339(value: OffsetDateTime) -> String {
    value.format(&Rfc3339).unwrap()
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    use sqlx::{Connection, PgConnection};
    use url::Url;

    pub(crate) struct TestDb {
        pub(crate) db: Db,
        admin_database_url: String,
        database_name: String,
    }

    impl TestDb {
        pub(crate) async fn new() -> Option<Self> {
            let admin_database_url = std::env::var("TEST_DATABASE_URL")
                .ok()
                .or_else(|| std::env::var("DATABASE_URL").ok())?;

            let database_name = format!("supermanager_test_{}", Uuid::new_v4().simple());
            let mut admin = PgConnection::connect(&admin_database_url).await.unwrap();
            sqlx::query(&format!(r#"CREATE DATABASE "{database_name}""#))
                .execute(&mut admin)
                .await
                .unwrap();
            drop(admin);

            let database_url = database_url_for_test(&admin_database_url, &database_name).unwrap();
            let db = Db::connect(&database_url).await.unwrap();

            Some(Self {
                db,
                admin_database_url,
                database_name,
            })
        }

        pub(crate) async fn cleanup(self) {
            self.db.pool.close().await;

            let mut admin = PgConnection::connect(&self.admin_database_url)
                .await
                .unwrap();
            sqlx::query(
                "SELECT pg_terminate_backend(pid)
                 FROM pg_stat_activity
                 WHERE datname = $1
                   AND pid <> pg_backend_pid()",
            )
            .bind(&self.database_name)
            .execute(&mut admin)
            .await
            .unwrap();
            sqlx::query(&format!(
                r#"DROP DATABASE IF EXISTS "{0}""#,
                self.database_name
            ))
            .execute(&mut admin)
            .await
            .unwrap();
        }
    }

    fn database_url_for_test(admin_database_url: &str, database_name: &str) -> Result<String> {
        let mut url = Url::parse(admin_database_url)?;
        url.set_path(&format!("/{database_name}"));
        Ok(url.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::test_support::TestDb;

    #[tokio::test]
    async fn room_lookup_normalizes_case() {
        let Some(test_db) = TestDb::new().await else {
            eprintln!("skipping PostgreSQL test: TEST_DATABASE_URL is not set");
            return;
        };

        let room = test_db.db.create_room("Case Test").await.unwrap();
        let fetched = test_db
            .db
            .get_room(&room.room_id.to_ascii_lowercase())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(fetched.room_id, room.room_id);
        test_db.cleanup().await;
    }

    #[tokio::test]
    async fn summary_defaults_to_empty_ready_snapshot() {
        let Some(test_db) = TestDb::new().await else {
            eprintln!("skipping PostgreSQL test: TEST_DATABASE_URL is not set");
            return;
        };

        let room = test_db.db.create_room("Summary Default").await.unwrap();

        assert_eq!(
            test_db.db.get_summary(&room.room_id).await.unwrap(),
            RoomSnapshot::default()
        );
        assert_eq!(
            test_db.db.get_summary_status(&room.room_id).await.unwrap(),
            "ready"
        );

        test_db.cleanup().await;
    }

    #[tokio::test]
    async fn summary_thread_id_survives_status_updates() {
        let Some(test_db) = TestDb::new().await else {
            eprintln!("skipping PostgreSQL test: TEST_DATABASE_URL is not set");
            return;
        };

        let room = test_db.db.create_room("Summary Thread").await.unwrap();
        test_db
            .db
            .set_summary_thread_id(&room.room_id, "thread_123")
            .await
            .unwrap();
        test_db
            .db
            .set_summary_status(&room.room_id, "generating")
            .await
            .unwrap();

        assert_eq!(
            test_db
                .db
                .get_summary_thread_id(&room.room_id)
                .await
                .unwrap(),
            Some("thread_123".to_owned())
        );
        assert_eq!(
            test_db.db.get_summary_status(&room.room_id).await.unwrap(),
            "generating"
        );

        test_db.cleanup().await;
    }

    #[tokio::test]
    async fn hook_events_round_trip_with_paging() {
        let Some(test_db) = TestDb::new().await else {
            eprintln!("skipping PostgreSQL test: TEST_DATABASE_URL is not set");
            return;
        };

        let room = test_db.db.create_room("Hook Events").await.unwrap();
        let first = test_db
            .db
            .insert_hook_event(
                &room.room_id,
                &HookTurnReport {
                    employee_name: "Alice".to_owned(),
                    client: "codex".to_owned(),
                    repo_root: "/tmp/repo".to_owned(),
                    branch: Some("main".to_owned()),
                    payload: serde_json::json!({ "message": "first" }),
                },
            )
            .await
            .unwrap();
        let second = test_db
            .db
            .insert_hook_event(
                &room.room_id,
                &HookTurnReport {
                    employee_name: "Bob".to_owned(),
                    client: "claude".to_owned(),
                    repo_root: "/tmp/repo".to_owned(),
                    branch: Some("feature/aws".to_owned()),
                    payload: serde_json::json!({ "message": "second" }),
                },
            )
            .await
            .unwrap();

        let newest = test_db
            .db
            .get_hook_events(&room.room_id, None, None, Some(1))
            .await
            .unwrap();
        let older = test_db
            .db
            .get_hook_events(&room.room_id, Some(second.seq), None, Some(10))
            .await
            .unwrap();
        let after_first = test_db
            .db
            .get_hook_events(&room.room_id, None, Some(first.seq), Some(10))
            .await
            .unwrap();

        assert_eq!(newest.len(), 1);
        assert_eq!(newest[0].event_id, second.event_id);
        assert_eq!(older.len(), 1);
        assert_eq!(older[0].event_id, first.event_id);
        assert_eq!(after_first.len(), 1);
        assert_eq!(after_first[0].event_id, second.event_id);

        test_db.cleanup().await;
    }
}
