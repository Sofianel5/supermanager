use anyhow::{Context, Result};
use reporter_protocol::RoomSnapshot;
use sqlx::{Row, types::Json};

use super::{Db, normalize_room_id};

impl Db {
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

#[cfg(test)]
mod tests {
    use crate::store::Db;
    use crate::store::test_support::TestDb;
    use reporter_protocol::{Room, RoomSnapshot};

    async fn create_test_room(db: &Db, name: &str) -> Room {
        db.create_room(name, "org_test", "user_test").await.unwrap()
    }

    #[tokio::test]
    async fn summary_defaults_to_empty_ready_snapshot() {
        let Some(test_db) = TestDb::new().await else {
            eprintln!("skipping PostgreSQL test: TEST_DATABASE_URL is not set");
            return;
        };

        let room = create_test_room(&test_db.db, "Summary Default").await;

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

        let room = create_test_room(&test_db.db, "Summary Thread").await;
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
}
