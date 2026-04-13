use anyhow::{Context, Result};
use reporter_protocol::{HookTurnReport, StoredHookEvent};
use serde_json::Value;
use sqlx::{Row, postgres::PgRow, types::Json};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::util::format_rfc3339;

use super::{Db, normalize_room_id};

impl Db {
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

#[cfg(test)]
mod tests {
    use crate::store::test_support::TestDb;
    use reporter_protocol::HookTurnReport;

    #[tokio::test]
    async fn hook_events_round_trip_with_paging() {
        let Some(test_db) = TestDb::new().await else {
            eprintln!("skipping PostgreSQL test: TEST_DATABASE_URL is not set");
            return;
        };

        let room = test_db
            .db
            .create_room("Hook Events", "org_test", "user_test")
            .await
            .unwrap();
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
