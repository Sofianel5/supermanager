use std::collections::HashSet;

use anyhow::{Context, Result};
use reporter_protocol::{
    EmployeeSnapshot, OrganizationSnapshot, RoomBlufSnapshot, RoomSnapshot, StoredHookEvent,
    SummaryStatus,
};
use serde_json::Value;
use sqlx::{
    PgPool, Row,
    postgres::PgPoolOptions,
    types::{Json, time::OffsetDateTime},
};
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

use crate::{
    event::{OrganizationHeartbeatEvent, OrganizationHeartbeatRoom},
    tools::SummaryTool,
};

#[derive(Clone)]
pub(crate) struct SummaryDb {
    pool: PgPool,
}

pub(crate) struct SummaryRoomRecord {
    pub(crate) room_id: String,
    pub(crate) name: String,
}

pub(crate) struct OrganizationSummaryClaim {
    pub(crate) previous_summary_updated_at: Option<String>,
}

pub(crate) struct RoomSummaryClaim {
    pub(crate) last_processed_seq: i64,
}

pub(crate) struct ToolExecutionResult {
    pub(crate) success: bool,
    pub(crate) message: String,
}

pub(crate) struct OrganizationSummaryQueryOptions {
    pub(crate) after_received_at: Option<String>,
    pub(crate) before_received_at: Option<String>,
    pub(crate) limit: Option<i64>,
}

pub(crate) struct RoomSummaryQueryOptions {
    pub(crate) after_seq: Option<i64>,
    pub(crate) limit: Option<i64>,
}

impl SummaryDb {
    pub(crate) async fn connect(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await
            .context("failed to connect to PostgreSQL")?;

        sqlx::query("SELECT 1")
            .execute(&pool)
            .await
            .context("failed to verify PostgreSQL connection")?;

        Ok(Self { pool })
    }

    pub(crate) async fn close(&self) {
        self.pool.close().await;
    }

    pub(crate) async fn reset_generating_organization_summaries(&self) -> Result<()> {
        sqlx::query(
            "UPDATE organization_summaries SET status = 'error' WHERE status = 'generating'",
        )
        .execute(&self.pool)
        .await
        .context("failed to reset generating organization summaries")?;
        Ok(())
    }

    pub(crate) async fn reset_generating_room_summaries(&self) -> Result<()> {
        sqlx::query("UPDATE room_summaries SET status = 'error' WHERE status = 'generating'")
            .execute(&self.pool)
            .await
            .context("failed to reset generating room summaries")?;
        Ok(())
    }

    pub(crate) async fn list_organizations_with_rooms(&self) -> Result<Vec<String>> {
        sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT organization_id FROM rooms ORDER BY organization_id ASC",
        )
        .fetch_all(&self.pool)
        .await
        .context("failed to list organizations with rooms")
    }

    pub(crate) async fn try_start_organization_summary(
        &self,
        organization_id: &str,
    ) -> Result<Option<OrganizationSummaryClaim>> {
        let row = sqlx::query(
            r#"
            INSERT INTO organization_summaries (organization_id, content_json, status, updated_at)
            VALUES ($1, $2, 'generating', TO_TIMESTAMP(0))
            ON CONFLICT(organization_id) DO UPDATE SET
              status = 'generating'
            WHERE organization_summaries.status <> 'generating'
            RETURNING updated_at
            "#,
        )
        .bind(organization_id)
        .bind(Json(stored_organization_snapshot(
            OrganizationSnapshot::default(),
        )))
        .fetch_optional(&self.pool)
        .await
        .with_context(|| format!("failed to claim organization summary for {organization_id}"))?;

        row.map(|row| {
            Ok(OrganizationSummaryClaim {
                previous_summary_updated_at: row
                    .try_get::<Option<OffsetDateTime>, _>("updated_at")
                    .context("failed to decode organization summary updated_at")?
                    .map(format_timestamp)
                    .transpose()?,
            })
        })
        .transpose()
    }

    pub(crate) async fn set_organization_summary_status(
        &self,
        organization_id: &str,
        status: SummaryStatus,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO organization_summaries (organization_id, content_json, status, updated_at)
            VALUES ($1, $2, $3, TO_TIMESTAMP(0))
            ON CONFLICT(organization_id) DO UPDATE SET
              status = EXCLUDED.status
            "#,
        )
        .bind(organization_id)
        .bind(Json(stored_organization_snapshot(
            OrganizationSnapshot::default(),
        )))
        .bind(status.as_db_str())
        .execute(&self.pool)
        .await
        .with_context(|| {
            format!("failed to persist organization summary status for {organization_id}")
        })?;
        Ok(())
    }

    pub(crate) async fn set_organization_summary_updated_at(
        &self,
        organization_id: &str,
        updated_at: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO organization_summaries (
              organization_id,
              content_json,
              updated_at
            )
            VALUES ($1, $2, $3::timestamptz)
            ON CONFLICT(organization_id) DO UPDATE SET
              updated_at = GREATEST(
                organization_summaries.updated_at,
                EXCLUDED.updated_at
              )
            "#,
        )
        .bind(organization_id)
        .bind(Json(stored_organization_snapshot(
            OrganizationSnapshot::default(),
        )))
        .bind(updated_at)
        .execute(&self.pool)
        .await
        .with_context(|| {
            format!("failed to persist organization summary updated_at for {organization_id}")
        })?;
        Ok(())
    }

    pub(crate) async fn list_rooms_for_summary(
        &self,
        organization_id: &str,
    ) -> Result<Vec<OrganizationHeartbeatRoom>> {
        let rows = sqlx::query(
            r#"
            SELECT room_id, name
            FROM rooms
            WHERE organization_id = $1
            ORDER BY created_at DESC, room_id DESC
            "#,
        )
        .bind(organization_id)
        .fetch_all(&self.pool)
        .await
        .with_context(|| format!("failed to list rooms for organization {organization_id}"))?;

        rows.into_iter()
            .map(|row| {
                Ok(OrganizationHeartbeatRoom {
                    room_id: row.try_get("room_id").context("failed to decode room_id")?,
                    name: row.try_get("name").context("failed to decode room name")?,
                })
            })
            .collect()
    }

    pub(crate) async fn query_organization_events_for_summary(
        &self,
        organization_id: &str,
        options: OrganizationSummaryQueryOptions,
    ) -> Result<Vec<OrganizationHeartbeatEvent>> {
        let rows = sqlx::query(
            r#"
            SELECT
              h.seq,
              h.event_id,
              h.room_id,
              r.name AS room_name,
              h.employee_user_id,
              h.employee_name,
              h.client,
              h.repo_root,
              h.branch,
              h.payload_json,
              h.received_at
            FROM hook_events AS h
            INNER JOIN rooms AS r ON r.room_id = h.room_id
            WHERE r.organization_id = $1
              AND ($2::timestamptz IS NULL OR h.received_at > $2::timestamptz)
              AND ($3::timestamptz IS NULL OR h.received_at <= $3::timestamptz)
            ORDER BY h.received_at ASC, h.seq ASC
            LIMIT COALESCE($4, 9223372036854775807)
            "#,
        )
        .bind(organization_id)
        .bind(options.after_received_at)
        .bind(options.before_received_at)
        .bind(options.limit)
        .fetch_all(&self.pool)
        .await
        .with_context(|| {
            format!("failed to query organization summary events for {organization_id}")
        })?;

        rows.into_iter()
            .map(|row| {
                Ok(OrganizationHeartbeatEvent {
                    room_id: row.try_get("room_id").context("failed to decode room_id")?,
                    room_name: row
                        .try_get("room_name")
                        .context("failed to decode room_name")?,
                    event: map_stored_hook_event(&row)?,
                })
            })
            .collect()
    }

    pub(crate) async fn get_organization_summary(
        &self,
        organization_id: &str,
    ) -> Result<OrganizationSnapshot> {
        let (stored, rooms) = tokio::try_join!(
            self.get_stored_organization_summary(organization_id),
            self.list_room_blufs_for_organization(organization_id),
        )?;
        Ok(OrganizationSnapshot { rooms, ..stored })
    }

    pub(crate) async fn list_room_ids_for_organization(
        &self,
        organization_id: &str,
    ) -> Result<Vec<String>> {
        sqlx::query_scalar::<_, String>(
            r#"
            SELECT room_id
            FROM rooms
            WHERE organization_id = $1
            ORDER BY created_at DESC, room_id DESC
            "#,
        )
        .bind(organization_id)
        .fetch_all(&self.pool)
        .await
        .with_context(|| format!("failed to list room ids for organization {organization_id}"))
    }

    pub(crate) async fn set_organization_summary(
        &self,
        organization_id: &str,
        content: &OrganizationSnapshot,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO organization_summaries (organization_id, content_json, status, updated_at)
            VALUES ($1, $2, 'ready', TO_TIMESTAMP(0))
            ON CONFLICT(organization_id) DO UPDATE SET
              content_json = EXCLUDED.content_json
            "#,
        )
        .bind(organization_id)
        .bind(Json(stored_organization_snapshot(content.clone())))
        .execute(&self.pool)
        .await
        .with_context(|| format!("failed to persist organization summary for {organization_id}"))?;
        Ok(())
    }

    pub(crate) async fn list_rooms_needing_summary(
        &self,
        limit: i64,
    ) -> Result<Vec<SummaryRoomRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT
              rooms.room_id,
              rooms.name
            FROM rooms
            INNER JOIN hook_events ON hook_events.room_id = rooms.room_id
            LEFT JOIN room_summaries ON room_summaries.room_id = rooms.room_id
            GROUP BY rooms.room_id, rooms.name, room_summaries.last_processed_seq
            HAVING MAX(hook_events.seq) > COALESCE(room_summaries.last_processed_seq, 0)
            ORDER BY MAX(hook_events.received_at) ASC, rooms.room_id ASC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .context("failed to list rooms needing summary")?;

        rows.into_iter()
            .map(|row| {
                Ok(SummaryRoomRecord {
                    room_id: row.try_get("room_id").context("failed to decode room_id")?,
                    name: row.try_get("name").context("failed to decode room name")?,
                })
            })
            .collect()
    }

    pub(crate) async fn try_start_room_summary(
        &self,
        room_id: &str,
    ) -> Result<Option<RoomSummaryClaim>> {
        let normalized_room_id = normalize_room_id(room_id);
        let row = sqlx::query(
            r#"
            INSERT INTO room_summaries (
              room_id,
              content_json,
              status,
              updated_at,
              last_processed_seq
            )
            VALUES ($1, $2, 'generating', TO_TIMESTAMP(0), 0)
            ON CONFLICT(room_id) DO UPDATE SET
              status = 'generating'
            WHERE room_summaries.status <> 'generating'
            RETURNING last_processed_seq
            "#,
        )
        .bind(&normalized_room_id)
        .bind(Json(normalize_room_snapshot(RoomSnapshot::default())))
        .fetch_optional(&self.pool)
        .await
        .with_context(|| format!("failed to claim room summary for {normalized_room_id}"))?;

        row.map(|row| {
            Ok(RoomSummaryClaim {
                last_processed_seq: row
                    .try_get::<Option<i64>, _>("last_processed_seq")
                    .context("failed to decode room summary last_processed_seq")?
                    .unwrap_or(0),
            })
        })
        .transpose()
    }

    pub(crate) async fn query_room_events_for_summary(
        &self,
        room_id: &str,
        options: RoomSummaryQueryOptions,
    ) -> Result<Vec<StoredHookEvent>> {
        let normalized_room_id = normalize_room_id(room_id);
        let rows = sqlx::query(
            r#"
            SELECT
              seq,
              event_id,
              employee_user_id,
              employee_name,
              client,
              repo_root,
              branch,
              payload_json,
              received_at
            FROM hook_events
            WHERE room_id = $1
              AND ($2::bigint IS NULL OR seq > $2)
            ORDER BY seq ASC
            LIMIT COALESCE($3, 9223372036854775807)
            "#,
        )
        .bind(&normalized_room_id)
        .bind(options.after_seq)
        .bind(options.limit)
        .fetch_all(&self.pool)
        .await
        .with_context(|| format!("failed to query room summary events for {normalized_room_id}"))?;

        rows.into_iter()
            .map(|row| map_stored_hook_event(&row))
            .collect()
    }

    pub(crate) async fn set_room_summary_status(
        &self,
        room_id: &str,
        status: SummaryStatus,
    ) -> Result<()> {
        let normalized_room_id = normalize_room_id(room_id);
        sqlx::query(
            r#"
            INSERT INTO room_summaries (
              room_id,
              content_json,
              status,
              updated_at,
              last_processed_seq
            )
            VALUES ($1, $2, $3, TO_TIMESTAMP(0), 0)
            ON CONFLICT(room_id) DO UPDATE SET
              status = EXCLUDED.status
            "#,
        )
        .bind(&normalized_room_id)
        .bind(Json(normalize_room_snapshot(RoomSnapshot::default())))
        .bind(status.as_db_str())
        .execute(&self.pool)
        .await
        .with_context(|| {
            format!("failed to persist room summary status for {normalized_room_id}")
        })?;
        Ok(())
    }

    pub(crate) async fn set_room_summary_last_processed_seq(
        &self,
        room_id: &str,
        last_processed_seq: i64,
    ) -> Result<()> {
        let normalized_room_id = normalize_room_id(room_id);
        sqlx::query(
            r#"
            INSERT INTO room_summaries (
              room_id,
              content_json,
              status,
              updated_at,
              last_processed_seq
            )
            VALUES ($1, $2, 'ready', TO_TIMESTAMP(0), $3)
            ON CONFLICT(room_id) DO UPDATE SET
              last_processed_seq = GREATEST(
                room_summaries.last_processed_seq,
                EXCLUDED.last_processed_seq
              )
            "#,
        )
        .bind(&normalized_room_id)
        .bind(Json(normalize_room_snapshot(RoomSnapshot::default())))
        .bind(last_processed_seq)
        .execute(&self.pool)
        .await
        .with_context(|| {
            format!("failed to persist last_processed_seq for room {normalized_room_id}")
        })?;
        Ok(())
    }

    pub(crate) async fn get_room_summary(&self, room_id: &str) -> Result<RoomSnapshot> {
        let normalized_room_id = normalize_room_id(room_id);
        let row = sqlx::query("SELECT content_json FROM room_summaries WHERE room_id = $1")
            .bind(&normalized_room_id)
            .fetch_optional(&self.pool)
            .await
            .with_context(|| format!("failed to fetch room summary for {normalized_room_id}"))?;

        Ok(row
            .map(|row| {
                row.try_get::<Option<Json<RoomSnapshot>>, _>("content_json")
                    .context("failed to decode room summary content_json")
            })
            .transpose()?
            .flatten()
            .map(|json| normalize_room_snapshot(json.0))
            .unwrap_or_default())
    }

    pub(crate) async fn set_room_summary(
        &self,
        room_id: &str,
        content: &RoomSnapshot,
    ) -> Result<()> {
        let normalized_room_id = normalize_room_id(room_id);
        sqlx::query(
            r#"
            INSERT INTO room_summaries (
              room_id,
              content_json,
              status,
              updated_at,
              last_processed_seq
            )
            VALUES ($1, $2, 'ready', NOW(), 0)
            ON CONFLICT(room_id) DO UPDATE SET
              content_json = EXCLUDED.content_json,
              updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(&normalized_room_id)
        .bind(Json(normalize_room_snapshot(content.clone())))
        .execute(&self.pool)
        .await
        .with_context(|| format!("failed to persist room summary for {normalized_room_id}"))?;
        Ok(())
    }

    pub(crate) async fn execute_room_tool_call(
        &self,
        room_id: &str,
        tool: SummaryTool,
    ) -> Result<ToolExecutionResult> {
        match tool {
            SummaryTool::RoomGetSnapshot => Ok(ToolExecutionResult {
                success: true,
                message: serde_json::to_string_pretty(&self.get_room_summary(room_id).await?)
                    .context("failed to serialize room snapshot")?,
            }),
            SummaryTool::SetRoomBluf { markdown } => {
                let normalized_room_id = normalize_room_id(room_id);
                let mut snapshot = self.get_room_summary(&normalized_room_id).await?;
                snapshot.bluf_markdown = markdown.trim().to_owned();
                self.set_room_summary(&normalized_room_id, &snapshot)
                    .await?;
                Ok(ToolExecutionResult {
                    success: true,
                    message: format!("updated room BLUF for {normalized_room_id}"),
                })
            }
            SummaryTool::SetRoomDetailedSummary { markdown } => {
                let normalized_room_id = normalize_room_id(room_id);
                let mut snapshot = self.get_room_summary(&normalized_room_id).await?;
                snapshot.detailed_summary_markdown = markdown.trim().to_owned();
                self.set_room_summary(&normalized_room_id, &snapshot)
                    .await?;
                Ok(ToolExecutionResult {
                    success: true,
                    message: format!("updated room detailed summary for {normalized_room_id}"),
                })
            }
            SummaryTool::SetEmployeeBluf {
                employee_user_id,
                employee_name,
                markdown,
                ..
            } => {
                let normalized_room_id = normalize_room_id(room_id);
                let mut snapshot = self.get_room_summary(&normalized_room_id).await?;
                upsert_employee_bluf(
                    &mut snapshot.employees,
                    employee_user_id.trim(),
                    employee_name.trim(),
                    vec![normalized_room_id.clone()],
                    markdown.trim(),
                    now_rfc3339()?,
                );
                self.set_room_summary(&normalized_room_id, &snapshot)
                    .await?;
                Ok(ToolExecutionResult {
                    success: true,
                    message: format!(
                        "updated employee BLUF for {} in {normalized_room_id}",
                        employee_name.trim()
                    ),
                })
            }
            SummaryTool::RemoveEmployeeBluf {
                employee_user_id,
                employee_name,
            } => {
                let normalized_room_id = normalize_room_id(room_id);
                let mut snapshot = self.get_room_summary(&normalized_room_id).await?;
                let result = remove_employee_bluf(
                    &mut snapshot,
                    employee_user_id.trim(),
                    employee_name.trim(),
                );
                if result.changed {
                    self.set_room_summary(&normalized_room_id, &snapshot)
                        .await?;
                }
                Ok(ToolExecutionResult {
                    success: true,
                    message: result.message,
                })
            }
            _ => Ok(ToolExecutionResult {
                success: false,
                message: "tool is not available for room summaries".to_owned(),
            }),
        }
    }

    pub(crate) async fn execute_organization_tool_call(
        &self,
        organization_id: &str,
        tool: SummaryTool,
    ) -> Result<ToolExecutionResult> {
        match tool {
            SummaryTool::OrganizationGetSnapshot => Ok(ToolExecutionResult {
                success: true,
                message: serde_json::to_string_pretty(
                    &self.get_organization_summary(organization_id).await?,
                )
                .context("failed to serialize organization snapshot")?,
            }),
            SummaryTool::SetOrgBluf { markdown } => {
                let mut snapshot = self.get_organization_summary(organization_id).await?;
                snapshot.bluf_markdown = markdown.trim().to_owned();
                self.set_organization_summary(organization_id, &snapshot)
                    .await?;
                Ok(ToolExecutionResult {
                    success: true,
                    message: "updated organization BLUF".to_owned(),
                })
            }
            SummaryTool::SetEmployeeBluf {
                employee_user_id,
                employee_name,
                room_ids,
                markdown,
            } => {
                let requested_room_ids = normalize_room_ids(room_ids);
                let known_room_ids = self
                    .list_room_ids_for_organization(organization_id)
                    .await?
                    .into_iter()
                    .collect::<HashSet<_>>();
                let valid_room_ids = requested_room_ids
                    .into_iter()
                    .filter(|room_id| known_room_ids.contains(room_id))
                    .collect::<Vec<_>>();

                if valid_room_ids.is_empty() {
                    return Ok(ToolExecutionResult {
                        success: false,
                        message:
                            "room_ids must include at least one valid room for the organization"
                                .to_owned(),
                    });
                }

                let mut snapshot = self.get_organization_summary(organization_id).await?;
                upsert_employee_bluf(
                    &mut snapshot.employees,
                    employee_user_id.trim(),
                    employee_name.trim(),
                    valid_room_ids,
                    markdown.trim(),
                    now_rfc3339()?,
                );
                self.set_organization_summary(organization_id, &snapshot)
                    .await?;
                Ok(ToolExecutionResult {
                    success: true,
                    message: format!("updated employee BLUF for {}", employee_name.trim()),
                })
            }
            SummaryTool::RemoveEmployeeBluf {
                employee_user_id,
                employee_name,
            } => {
                let mut snapshot = self.get_organization_summary(organization_id).await?;
                let result = remove_employee_bluf(
                    &mut snapshot,
                    employee_user_id.trim(),
                    employee_name.trim(),
                );
                if result.changed {
                    self.set_organization_summary(organization_id, &snapshot)
                        .await?;
                }
                Ok(ToolExecutionResult {
                    success: true,
                    message: result.message,
                })
            }
            _ => Ok(ToolExecutionResult {
                success: false,
                message: "tool is not available for organization summaries".to_owned(),
            }),
        }
    }

    async fn get_stored_organization_summary(
        &self,
        organization_id: &str,
    ) -> Result<OrganizationSnapshot> {
        let row = sqlx::query(
            "SELECT content_json FROM organization_summaries WHERE organization_id = $1",
        )
        .bind(organization_id)
        .fetch_optional(&self.pool)
        .await
        .with_context(|| {
            format!("failed to fetch stored organization summary for {organization_id}")
        })?;

        Ok(row
            .map(|row| {
                row.try_get::<Option<Json<OrganizationSnapshot>>, _>("content_json")
                    .context("failed to decode organization summary content_json")
            })
            .transpose()?
            .flatten()
            .map(|json| normalize_organization_snapshot(json.0))
            .unwrap_or_default())
    }

    async fn list_room_blufs_for_organization(
        &self,
        organization_id: &str,
    ) -> Result<Vec<RoomBlufSnapshot>> {
        let rows = sqlx::query(
            r#"
            SELECT
              rooms.room_id,
              room_summaries.content_json,
              room_summaries.updated_at
            FROM rooms
            LEFT JOIN room_summaries ON room_summaries.room_id = rooms.room_id
            WHERE rooms.organization_id = $1
            ORDER BY rooms.created_at DESC, rooms.room_id DESC
            "#,
        )
        .bind(organization_id)
        .fetch_all(&self.pool)
        .await
        .with_context(|| format!("failed to list room BLUFs for organization {organization_id}"))?;

        rows.into_iter()
            .map(|row| {
                let room_id: String = row.try_get("room_id").context("failed to decode room_id")?;
                let snapshot = row
                    .try_get::<Option<Json<RoomSnapshot>>, _>("content_json")
                    .context("failed to decode room_summaries.content_json")?
                    .map(|json| normalize_room_snapshot(json.0))
                    .unwrap_or_default();
                let updated_at = row
                    .try_get::<Option<OffsetDateTime>, _>("updated_at")
                    .context("failed to decode room_summaries.updated_at")?
                    .map(format_timestamp)
                    .transpose()?;
                Ok(RoomBlufSnapshot {
                    room_id: normalize_room_id(&room_id),
                    bluf_markdown: snapshot.bluf_markdown,
                    last_update_at: updated_at.unwrap_or_default(),
                })
            })
            .collect()
    }
}

struct RemoveEmployeeResult {
    changed: bool,
    message: String,
}

fn map_stored_hook_event(row: &sqlx::postgres::PgRow) -> Result<StoredHookEvent> {
    Ok(StoredHookEvent {
        seq: row.try_get("seq").context("failed to decode seq")?,
        event_id: row
            .try_get::<Uuid, _>("event_id")
            .context("failed to decode event_id")?,
        received_at: format_timestamp(
            row.try_get::<OffsetDateTime, _>("received_at")
                .context("failed to decode received_at")?,
        )?,
        employee_user_id: row
            .try_get("employee_user_id")
            .context("failed to decode employee_user_id")?,
        employee_name: row
            .try_get("employee_name")
            .context("failed to decode employee_name")?,
        client: row.try_get("client").context("failed to decode client")?,
        repo_root: row
            .try_get("repo_root")
            .context("failed to decode repo_root")?,
        branch: row.try_get("branch").context("failed to decode branch")?,
        payload: row
            .try_get::<Value, _>("payload_json")
            .context("failed to decode payload_json")?,
    })
}

fn normalize_room_id(room_id: &str) -> String {
    room_id.trim().to_ascii_uppercase()
}

fn normalize_room_ids(room_ids: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for room_id in room_ids {
        let room_id = normalize_room_id(&room_id);
        if room_id.is_empty() || !seen.insert(room_id.clone()) {
            continue;
        }
        normalized.push(room_id);
    }
    normalized
}

fn normalize_employee_snapshot(snapshot: EmployeeSnapshot) -> EmployeeSnapshot {
    EmployeeSnapshot {
        employee_user_id: snapshot.employee_user_id.trim().to_owned(),
        employee_name: snapshot.employee_name,
        room_ids: normalize_room_ids(snapshot.room_ids),
        bluf_markdown: snapshot.bluf_markdown,
        last_update_at: snapshot.last_update_at,
    }
}

fn normalize_room_snapshot(snapshot: RoomSnapshot) -> RoomSnapshot {
    RoomSnapshot {
        bluf_markdown: snapshot.bluf_markdown,
        detailed_summary_markdown: snapshot.detailed_summary_markdown,
        employees: snapshot
            .employees
            .into_iter()
            .map(normalize_employee_snapshot)
            .collect(),
    }
}

fn normalize_organization_snapshot(snapshot: OrganizationSnapshot) -> OrganizationSnapshot {
    OrganizationSnapshot {
        bluf_markdown: snapshot.bluf_markdown,
        rooms: snapshot.rooms,
        employees: snapshot
            .employees
            .into_iter()
            .map(normalize_employee_snapshot)
            .collect(),
    }
}

fn stored_organization_snapshot(snapshot: OrganizationSnapshot) -> OrganizationSnapshot {
    let mut snapshot = normalize_organization_snapshot(snapshot);
    snapshot.rooms.clear();
    snapshot
}

fn upsert_employee_bluf(
    employees: &mut Vec<EmployeeSnapshot>,
    employee_user_id: &str,
    employee_name: &str,
    room_ids: Vec<String>,
    markdown: &str,
    updated_at: String,
) {
    let normalized_employee_user_id = employee_user_id.trim();
    if normalized_employee_user_id.is_empty() {
        return;
    }
    if let Some(existing) = employees
        .iter_mut()
        .find(|employee| employee_snapshot_matches(employee, normalized_employee_user_id))
    {
        existing.employee_user_id = normalized_employee_user_id.to_owned();
        existing.employee_name = employee_name.to_owned();
        existing.room_ids = room_ids;
        existing.bluf_markdown = markdown.to_owned();
        existing.last_update_at = updated_at;
        return;
    }

    employees.push(EmployeeSnapshot {
        employee_user_id: normalized_employee_user_id.to_owned(),
        employee_name: employee_name.to_owned(),
        room_ids,
        bluf_markdown: markdown.to_owned(),
        last_update_at: updated_at,
    });
}

fn remove_employee_bluf<T>(
    snapshot: &mut T,
    employee_user_id: &str,
    employee_name: &str,
) -> RemoveEmployeeResult
where
    T: EmployeeSnapshotContainer,
{
    let normalized_employee_user_id = employee_user_id.trim();
    if normalized_employee_user_id.is_empty() {
        return RemoveEmployeeResult {
            changed: false,
            message: format!("employee BLUF already absent for {employee_name}"),
        };
    }
    let employees = snapshot.employees_mut();
    let before = employees.len();
    employees.retain(|employee| !employee_snapshot_matches(employee, normalized_employee_user_id));

    let changed = employees.len() != before;
    RemoveEmployeeResult {
        changed,
        message: if changed {
            format!("removed employee BLUF for {employee_name}")
        } else {
            format!("employee BLUF already absent for {employee_name}")
        },
    }
}

fn employee_snapshot_matches(employee: &EmployeeSnapshot, employee_user_id: &str) -> bool {
    employee.employee_user_id == employee_user_id
}

trait EmployeeSnapshotContainer {
    fn employees_mut(&mut self) -> &mut Vec<EmployeeSnapshot>;
}

impl EmployeeSnapshotContainer for RoomSnapshot {
    fn employees_mut(&mut self) -> &mut Vec<EmployeeSnapshot> {
        &mut self.employees
    }
}

impl EmployeeSnapshotContainer for OrganizationSnapshot {
    fn employees_mut(&mut self) -> &mut Vec<EmployeeSnapshot> {
        &mut self.employees
    }
}

fn format_timestamp(timestamp: OffsetDateTime) -> Result<String> {
    timestamp
        .format(&Rfc3339)
        .context("failed to format timestamp as RFC3339")
}

fn now_rfc3339() -> Result<String> {
    format_timestamp(OffsetDateTime::now_utc())
}
