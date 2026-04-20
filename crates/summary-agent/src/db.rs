use std::{
    collections::HashSet,
    path::{Component, Path},
};

use anyhow::{Context, Result};
use reporter_protocol::{
    MemberSnapshot, OrganizationSnapshot, ProjectBlufSnapshot, ProjectSnapshot, StoredHookEvent,
    SummaryStatus,
};
use serde::Serialize;
use serde_json::Value;
use sqlx::{
    PgPool, Row,
    postgres::PgPoolOptions,
    types::{Json, time::OffsetDateTime},
};
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

use crate::{
    event::{OrganizationEvent, OrganizationProject, OrganizationTranscript},
    tools::SummaryTool,
    workflow::WorkflowKind,
};

#[derive(Clone)]
pub(crate) struct SummaryDb {
    pool: PgPool,
}

pub(crate) struct SummaryProjectRecord {
    pub(crate) project_id: String,
    pub(crate) name: String,
}

pub(crate) struct OrganizationSummaryClaim {
    pub(crate) previous_summary_updated_at: Option<String>,
}

pub(crate) struct OrganizationWorkflowClaim {
    pub(crate) previous_processed_received_at: Option<String>,
}

pub(crate) struct ProjectSummaryClaim {
    pub(crate) last_processed_seq: i64,
}

#[derive(Serialize)]
struct OrganizationWorkflowDocumentRecord {
    path: String,
    content: String,
    updated_at: String,
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

pub(crate) type OrganizationWorkflowQueryOptions = OrganizationSummaryQueryOptions;

pub(crate) struct ProjectSummaryQueryOptions {
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

    pub(crate) async fn reset_generating_project_summaries(&self) -> Result<()> {
        sqlx::query("UPDATE project_summaries SET status = 'error' WHERE status = 'generating'")
            .execute(&self.pool)
            .await
            .context("failed to reset generating project summaries")?;
        Ok(())
    }

    pub(crate) async fn reset_generating_organization_workflows(&self) -> Result<()> {
        sqlx::query(
            "UPDATE organization_workflows SET status = 'error' WHERE status = 'generating'",
        )
        .execute(&self.pool)
        .await
        .context("failed to reset generating organization workflows")?;
        Ok(())
    }

    pub(crate) async fn list_organizations_with_projects(&self) -> Result<Vec<String>> {
        sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT organization_id FROM projects ORDER BY organization_id ASC",
        )
        .fetch_all(&self.pool)
        .await
        .context("failed to list organizations with projects")
    }

    pub(crate) async fn list_organizations_with_transcripts(&self) -> Result<Vec<String>> {
        sqlx::query_scalar::<_, String>(
            r#"
            SELECT DISTINCT projects.organization_id
            FROM hook_event_transcripts
            INNER JOIN hook_events ON hook_events.event_id = hook_event_transcripts.event_id
            INNER JOIN projects ON projects.project_id = hook_events.project_id
            ORDER BY projects.organization_id ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("failed to list organizations with transcripts")
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

    pub(crate) async fn try_start_organization_workflow(
        &self,
        organization_id: &str,
        workflow_kind: WorkflowKind,
    ) -> Result<Option<OrganizationWorkflowClaim>> {
        let row = sqlx::query(
            r#"
            INSERT INTO organization_workflows (
              organization_id,
              workflow_kind,
              status,
              updated_at,
              last_processed_received_at
            )
            VALUES ($1, $2, 'generating', TO_TIMESTAMP(0), TO_TIMESTAMP(0))
            ON CONFLICT(organization_id, workflow_kind) DO UPDATE SET
              status = 'generating'
            WHERE organization_workflows.status <> 'generating'
            RETURNING last_processed_received_at
            "#,
        )
        .bind(organization_id)
        .bind(workflow_kind.as_str())
        .fetch_optional(&self.pool)
        .await
        .with_context(|| {
            format!(
                "failed to claim organization workflow {} for {organization_id}",
                workflow_kind.as_str()
            )
        })?;

        row.map(|row| {
            Ok(OrganizationWorkflowClaim {
                previous_processed_received_at: row
                    .try_get::<Option<OffsetDateTime>, _>("last_processed_received_at")
                    .context("failed to decode last_processed_received_at")?
                    .map(format_timestamp)
                    .transpose()?
                    .and_then(|value| {
                        if value == "1970-01-01T00:00:00Z" {
                            None
                        } else {
                            Some(value)
                        }
                    }),
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

    pub(crate) async fn set_organization_workflow_status(
        &self,
        organization_id: &str,
        workflow_kind: WorkflowKind,
        status: SummaryStatus,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO organization_workflows (
              organization_id,
              workflow_kind,
              status,
              updated_at,
              last_processed_received_at
            )
            VALUES ($1, $2, $3, TO_TIMESTAMP(0), TO_TIMESTAMP(0))
            ON CONFLICT(organization_id, workflow_kind) DO UPDATE SET
              status = EXCLUDED.status
            "#,
        )
        .bind(organization_id)
        .bind(workflow_kind.as_str())
        .bind(status.as_db_str())
        .execute(&self.pool)
        .await
        .with_context(|| {
            format!(
                "failed to persist organization workflow status {} for {organization_id}",
                workflow_kind.as_str()
            )
        })?;
        Ok(())
    }

    pub(crate) async fn set_organization_workflow_updated_at(
        &self,
        organization_id: &str,
        workflow_kind: WorkflowKind,
        updated_at: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO organization_workflows (
              organization_id,
              workflow_kind,
              status,
              updated_at,
              last_processed_received_at
            )
            VALUES ($1, $2, 'ready', NOW(), $3::timestamptz)
            ON CONFLICT(organization_id, workflow_kind) DO UPDATE SET
              updated_at = EXCLUDED.updated_at,
              last_processed_received_at = GREATEST(
                organization_workflows.last_processed_received_at,
                EXCLUDED.last_processed_received_at
              )
            "#,
        )
        .bind(organization_id)
        .bind(workflow_kind.as_str())
        .bind(updated_at)
        .execute(&self.pool)
        .await
        .with_context(|| {
            format!(
                "failed to persist organization workflow updated_at {} for {organization_id}",
                workflow_kind.as_str()
            )
        })?;
        Ok(())
    }

    async fn list_organization_workflow_documents(
        &self,
        organization_id: &str,
        workflow_kind: WorkflowKind,
    ) -> Result<Vec<OrganizationWorkflowDocumentRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT document_path, content_text, updated_at
            FROM organization_workflow_documents
            WHERE organization_id = $1
              AND workflow_kind = $2
            ORDER BY document_path ASC
            "#,
        )
        .bind(organization_id)
        .bind(workflow_kind.as_str())
        .fetch_all(&self.pool)
        .await
        .with_context(|| {
            format!(
                "failed to list organization workflow documents {} for {organization_id}",
                workflow_kind.as_str()
            )
        })?;

        rows.into_iter()
            .map(|row| {
                Ok(OrganizationWorkflowDocumentRecord {
                    path: row
                        .try_get("document_path")
                        .context("failed to decode workflow document path")?,
                    content: row
                        .try_get("content_text")
                        .context("failed to decode workflow document content")?,
                    updated_at: row
                        .try_get::<OffsetDateTime, _>("updated_at")
                        .context("failed to decode workflow document updated_at")
                        .and_then(format_timestamp)?,
                })
            })
            .collect()
    }

    async fn upsert_organization_workflow_document(
        &self,
        organization_id: &str,
        workflow_kind: WorkflowKind,
        path: &str,
        content: &str,
    ) -> Result<()> {
        let normalized_path = normalize_workflow_document_path(workflow_kind, path)?;

        sqlx::query(
            r#"
            INSERT INTO organization_workflow_documents (
              organization_id,
              workflow_kind,
              document_path,
              content_text
            )
            VALUES ($1, $2, $3, $4)
            ON CONFLICT(organization_id, workflow_kind, document_path) DO UPDATE SET
              content_text = EXCLUDED.content_text,
              updated_at = NOW()
            "#,
        )
        .bind(organization_id)
        .bind(workflow_kind.as_str())
        .bind(&normalized_path)
        .bind(content)
        .execute(&self.pool)
        .await
        .with_context(|| {
            format!(
                "failed to upsert organization workflow document {}:{} for {organization_id}",
                workflow_kind.as_str(),
                normalized_path
            )
        })?;

        Ok(())
    }

    async fn delete_organization_workflow_document(
        &self,
        organization_id: &str,
        workflow_kind: WorkflowKind,
        path: &str,
    ) -> Result<bool> {
        let normalized_path = normalize_workflow_document_path(workflow_kind, path)?;

        let result = sqlx::query(
            r#"
            DELETE FROM organization_workflow_documents
            WHERE organization_id = $1
              AND workflow_kind = $2
              AND document_path = $3
            "#,
        )
        .bind(organization_id)
        .bind(workflow_kind.as_str())
        .bind(&normalized_path)
        .execute(&self.pool)
        .await
        .with_context(|| {
            format!(
                "failed to delete organization workflow document {}:{} for {organization_id}",
                workflow_kind.as_str(),
                normalized_path
            )
        })?;

        Ok(result.rows_affected() > 0)
    }

    pub(crate) async fn list_projects_for_summary(
        &self,
        organization_id: &str,
    ) -> Result<Vec<OrganizationProject>> {
        let rows = sqlx::query(
            r#"
            SELECT project_id, name
            FROM projects
            WHERE organization_id = $1
            ORDER BY created_at DESC, project_id DESC
            "#,
        )
        .bind(organization_id)
        .fetch_all(&self.pool)
        .await
        .with_context(|| format!("failed to list projects for organization {organization_id}"))?;

        rows.into_iter()
            .map(|row| {
                Ok(OrganizationProject {
                    project_id: row
                        .try_get("project_id")
                        .context("failed to decode project_id")?,
                    name: row
                        .try_get("name")
                        .context("failed to decode project name")?,
                })
            })
            .collect()
    }

    pub(crate) async fn query_organization_events_for_summary(
        &self,
        organization_id: &str,
        options: OrganizationSummaryQueryOptions,
    ) -> Result<Vec<OrganizationEvent>> {
        let rows = sqlx::query(
            r#"
            SELECT
              h.seq,
              h.event_id,
              h.project_id,
              r.name AS project_name,
              h.member_user_id,
              h.member_name,
              h.client,
              h.repo_root,
              h.branch,
              h.payload_json,
              h.received_at
            FROM hook_events AS h
            INNER JOIN projects AS r ON r.project_id = h.project_id
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
                Ok(OrganizationEvent {
                    project_id: row
                        .try_get("project_id")
                        .context("failed to decode project_id")?,
                    project_name: row
                        .try_get("project_name")
                        .context("failed to decode project_name")?,
                    event: map_stored_hook_event(&row)?,
                })
            })
            .collect()
    }

    pub(crate) async fn query_organization_transcripts_for_workflow(
        &self,
        organization_id: &str,
        options: OrganizationWorkflowQueryOptions,
    ) -> Result<Vec<OrganizationTranscript>> {
        let rows = sqlx::query(
            r#"
            SELECT
              h.seq,
              h.event_id,
              h.project_id,
              r.name AS project_name,
              h.member_user_id,
              h.member_name,
              h.client,
              h.repo_root,
              h.branch,
              h.payload_json,
              h.received_at,
              t.transcript_path,
              t.content_text,
              t.truncated
            FROM hook_event_transcripts AS t
            INNER JOIN hook_events AS h ON h.event_id = t.event_id
            INNER JOIN projects AS r ON r.project_id = h.project_id
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
            format!("failed to query organization transcripts for workflow {organization_id}")
        })?;

        rows.into_iter()
            .map(|row| {
                Ok(OrganizationTranscript {
                    project_id: row
                        .try_get("project_id")
                        .context("failed to decode project_id")?,
                    project_name: row
                        .try_get("project_name")
                        .context("failed to decode project_name")?,
                    transcript_path: row
                        .try_get("transcript_path")
                        .context("failed to decode transcript_path")?,
                    transcript_text: row
                        .try_get("content_text")
                        .context("failed to decode content_text")?,
                    transcript_truncated: row
                        .try_get("truncated")
                        .context("failed to decode transcript truncated flag")?,
                    event: map_stored_hook_event(&row)?,
                })
            })
            .collect()
    }

    pub(crate) async fn get_organization_summary(
        &self,
        organization_id: &str,
    ) -> Result<OrganizationSnapshot> {
        let (stored, projects) = tokio::try_join!(
            self.get_stored_organization_summary(organization_id),
            self.list_project_blufs_for_organization(organization_id),
        )?;
        Ok(OrganizationSnapshot { projects, ..stored })
    }

    pub(crate) async fn list_project_ids_for_organization(
        &self,
        organization_id: &str,
    ) -> Result<Vec<String>> {
        sqlx::query_scalar::<_, String>(
            r#"
            SELECT project_id
            FROM projects
            WHERE organization_id = $1
            ORDER BY created_at DESC, project_id DESC
            "#,
        )
        .bind(organization_id)
        .fetch_all(&self.pool)
        .await
        .with_context(|| format!("failed to list project ids for organization {organization_id}"))
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

    pub(crate) async fn list_projects_needing_summary(
        &self,
        limit: i64,
    ) -> Result<Vec<SummaryProjectRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT
              projects.project_id,
              projects.name
            FROM projects
            INNER JOIN hook_events ON hook_events.project_id = projects.project_id
            LEFT JOIN project_summaries ON project_summaries.project_id = projects.project_id
            GROUP BY projects.project_id, projects.name, project_summaries.last_processed_seq
            HAVING MAX(hook_events.seq) > COALESCE(project_summaries.last_processed_seq, 0)
            ORDER BY MAX(hook_events.received_at) ASC, projects.project_id ASC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .context("failed to list projects needing summary")?;

        rows.into_iter()
            .map(|row| {
                Ok(SummaryProjectRecord {
                    project_id: row
                        .try_get("project_id")
                        .context("failed to decode project_id")?,
                    name: row
                        .try_get("name")
                        .context("failed to decode project name")?,
                })
            })
            .collect()
    }

    pub(crate) async fn try_start_project_summary(
        &self,
        project_id: &str,
    ) -> Result<Option<ProjectSummaryClaim>> {
        let normalized_project_id = normalize_project_id(project_id);
        let row = sqlx::query(
            r#"
            INSERT INTO project_summaries (
              project_id,
              content_json,
              status,
              updated_at,
              last_processed_seq
            )
            VALUES ($1, $2, 'generating', TO_TIMESTAMP(0), 0)
            ON CONFLICT(project_id) DO UPDATE SET
              status = 'generating'
            WHERE project_summaries.status <> 'generating'
            RETURNING last_processed_seq
            "#,
        )
        .bind(&normalized_project_id)
        .bind(Json(normalize_project_snapshot(ProjectSnapshot::default())))
        .fetch_optional(&self.pool)
        .await
        .with_context(|| format!("failed to claim project summary for {normalized_project_id}"))?;

        row.map(|row| {
            Ok(ProjectSummaryClaim {
                last_processed_seq: row
                    .try_get::<Option<i64>, _>("last_processed_seq")
                    .context("failed to decode project summary last_processed_seq")?
                    .unwrap_or(0),
            })
        })
        .transpose()
    }

    pub(crate) async fn query_project_events_for_summary(
        &self,
        project_id: &str,
        options: ProjectSummaryQueryOptions,
    ) -> Result<Vec<StoredHookEvent>> {
        let normalized_project_id = normalize_project_id(project_id);
        let rows = sqlx::query(
            r#"
            SELECT
              seq,
              event_id,
              member_user_id,
              member_name,
              client,
              repo_root,
              branch,
              payload_json,
              received_at
            FROM hook_events
            WHERE project_id = $1
              AND ($2::bigint IS NULL OR seq > $2)
            ORDER BY seq ASC
            LIMIT COALESCE($3, 9223372036854775807)
            "#,
        )
        .bind(&normalized_project_id)
        .bind(options.after_seq)
        .bind(options.limit)
        .fetch_all(&self.pool)
        .await
        .with_context(|| {
            format!("failed to query project summary events for {normalized_project_id}")
        })?;

        rows.into_iter()
            .map(|row| map_stored_hook_event(&row))
            .collect()
    }

    pub(crate) async fn set_project_summary_status(
        &self,
        project_id: &str,
        status: SummaryStatus,
    ) -> Result<()> {
        let normalized_project_id = normalize_project_id(project_id);
        sqlx::query(
            r#"
            INSERT INTO project_summaries (
              project_id,
              content_json,
              status,
              updated_at,
              last_processed_seq
            )
            VALUES ($1, $2, $3, TO_TIMESTAMP(0), 0)
            ON CONFLICT(project_id) DO UPDATE SET
              status = EXCLUDED.status
            "#,
        )
        .bind(&normalized_project_id)
        .bind(Json(normalize_project_snapshot(ProjectSnapshot::default())))
        .bind(status.as_db_str())
        .execute(&self.pool)
        .await
        .with_context(|| {
            format!("failed to persist project summary status for {normalized_project_id}")
        })?;
        Ok(())
    }

    pub(crate) async fn set_project_summary_last_processed_seq(
        &self,
        project_id: &str,
        last_processed_seq: i64,
    ) -> Result<()> {
        let normalized_project_id = normalize_project_id(project_id);
        sqlx::query(
            r#"
            INSERT INTO project_summaries (
              project_id,
              content_json,
              status,
              updated_at,
              last_processed_seq
            )
            VALUES ($1, $2, 'ready', TO_TIMESTAMP(0), $3)
            ON CONFLICT(project_id) DO UPDATE SET
              last_processed_seq = GREATEST(
                project_summaries.last_processed_seq,
                EXCLUDED.last_processed_seq
              )
            "#,
        )
        .bind(&normalized_project_id)
        .bind(Json(normalize_project_snapshot(ProjectSnapshot::default())))
        .bind(last_processed_seq)
        .execute(&self.pool)
        .await
        .with_context(|| {
            format!("failed to persist last_processed_seq for project {normalized_project_id}")
        })?;
        Ok(())
    }

    pub(crate) async fn get_project_summary(&self, project_id: &str) -> Result<ProjectSnapshot> {
        let normalized_project_id = normalize_project_id(project_id);
        let row = sqlx::query("SELECT content_json FROM project_summaries WHERE project_id = $1")
            .bind(&normalized_project_id)
            .fetch_optional(&self.pool)
            .await
            .with_context(|| {
                format!("failed to fetch project summary for {normalized_project_id}")
            })?;

        Ok(row
            .map(|row| {
                row.try_get::<Option<Json<ProjectSnapshot>>, _>("content_json")
                    .context("failed to decode project summary content_json")
            })
            .transpose()?
            .flatten()
            .map(|json| normalize_project_snapshot(json.0))
            .unwrap_or_default())
    }

    pub(crate) async fn set_project_summary(
        &self,
        project_id: &str,
        content: &ProjectSnapshot,
    ) -> Result<()> {
        let normalized_project_id = normalize_project_id(project_id);
        sqlx::query(
            r#"
            INSERT INTO project_summaries (
              project_id,
              content_json,
              status,
              updated_at,
              last_processed_seq
            )
            VALUES ($1, $2, 'ready', NOW(), 0)
            ON CONFLICT(project_id) DO UPDATE SET
              content_json = EXCLUDED.content_json,
              updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(&normalized_project_id)
        .bind(Json(normalize_project_snapshot(content.clone())))
        .execute(&self.pool)
        .await
        .with_context(|| {
            format!("failed to persist project summary for {normalized_project_id}")
        })?;
        Ok(())
    }

    pub(crate) async fn execute_project_tool_call(
        &self,
        project_id: &str,
        tool: SummaryTool,
    ) -> Result<ToolExecutionResult> {
        match tool {
            SummaryTool::ProjectGetSnapshot => Ok(ToolExecutionResult {
                success: true,
                message: serde_json::to_string_pretty(&self.get_project_summary(project_id).await?)
                    .context("failed to serialize project snapshot")?,
            }),
            SummaryTool::SetProjectBluf { markdown } => {
                let normalized_project_id = normalize_project_id(project_id);
                let mut snapshot = self.get_project_summary(&normalized_project_id).await?;
                snapshot.bluf_markdown = markdown.trim().to_owned();
                self.set_project_summary(&normalized_project_id, &snapshot)
                    .await?;
                Ok(ToolExecutionResult {
                    success: true,
                    message: format!("updated project BLUF for {normalized_project_id}"),
                })
            }
            SummaryTool::SetProjectDetailedSummary { markdown } => {
                let normalized_project_id = normalize_project_id(project_id);
                let mut snapshot = self.get_project_summary(&normalized_project_id).await?;
                snapshot.detailed_summary_markdown = markdown.trim().to_owned();
                self.set_project_summary(&normalized_project_id, &snapshot)
                    .await?;
                Ok(ToolExecutionResult {
                    success: true,
                    message: format!(
                        "updated project detailed summary for {normalized_project_id}"
                    ),
                })
            }
            SummaryTool::SetMemberBluf {
                member_user_id,
                member_name,
                markdown,
                ..
            } => {
                let normalized_project_id = normalize_project_id(project_id);
                let mut snapshot = self.get_project_summary(&normalized_project_id).await?;
                upsert_member_bluf(
                    &mut snapshot.members,
                    member_user_id.trim(),
                    member_name.trim(),
                    vec![normalized_project_id.clone()],
                    markdown.trim(),
                    now_rfc3339()?,
                );
                self.set_project_summary(&normalized_project_id, &snapshot)
                    .await?;
                Ok(ToolExecutionResult {
                    success: true,
                    message: format!(
                        "updated member BLUF for {} in {normalized_project_id}",
                        member_name.trim()
                    ),
                })
            }
            SummaryTool::RemoveMemberBluf {
                member_user_id,
                member_name,
            } => {
                let normalized_project_id = normalize_project_id(project_id);
                let mut snapshot = self.get_project_summary(&normalized_project_id).await?;
                let result =
                    remove_member_bluf(&mut snapshot, member_user_id.trim(), member_name.trim());
                if result.changed {
                    self.set_project_summary(&normalized_project_id, &snapshot)
                        .await?;
                }
                Ok(ToolExecutionResult {
                    success: true,
                    message: result.message,
                })
            }
            _ => Ok(ToolExecutionResult {
                success: false,
                message: "tool is not available for project summaries".to_owned(),
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
            SummaryTool::SetMemberBluf {
                member_user_id,
                member_name,
                project_ids,
                markdown,
            } => {
                let requested_project_ids = normalize_project_ids(project_ids);
                let known_project_ids = self
                    .list_project_ids_for_organization(organization_id)
                    .await?
                    .into_iter()
                    .collect::<HashSet<_>>();
                let valid_project_ids = requested_project_ids
                    .into_iter()
                    .filter(|project_id| known_project_ids.contains(project_id))
                    .collect::<Vec<_>>();

                if valid_project_ids.is_empty() {
                    return Ok(ToolExecutionResult {
                        success: false,
                        message:
                            "project_ids must include at least one valid project for the organization"
                                .to_owned(),
                    });
                }

                let mut snapshot = self.get_organization_summary(organization_id).await?;
                upsert_member_bluf(
                    &mut snapshot.members,
                    member_user_id.trim(),
                    member_name.trim(),
                    valid_project_ids,
                    markdown.trim(),
                    now_rfc3339()?,
                );
                self.set_organization_summary(organization_id, &snapshot)
                    .await?;
                Ok(ToolExecutionResult {
                    success: true,
                    message: format!("updated member BLUF for {}", member_name.trim()),
                })
            }
            SummaryTool::RemoveMemberBluf {
                member_user_id,
                member_name,
            } => {
                let mut snapshot = self.get_organization_summary(organization_id).await?;
                let result =
                    remove_member_bluf(&mut snapshot, member_user_id.trim(), member_name.trim());
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

    pub(crate) async fn execute_organization_workflow_tool_call(
        &self,
        organization_id: &str,
        workflow_kind: WorkflowKind,
        tool: SummaryTool,
    ) -> Result<ToolExecutionResult> {
        match tool {
            SummaryTool::OrganizationWorkflowGetSnapshot => {
                let documents = self
                    .list_organization_workflow_documents(organization_id, workflow_kind)
                    .await?;
                Ok(ToolExecutionResult {
                    success: true,
                    message: serde_json::to_string_pretty(&serde_json::json!({
                        "workflow_kind": workflow_kind.as_str(),
                        "path_root": workflow_document_root_label(workflow_kind),
                        "files": documents,
                    }))
                    .context("failed to serialize organization workflow snapshot")?,
                })
            }
            SummaryTool::UpsertOrganizationWorkflowFile { path, content } => {
                let normalized_path = normalize_workflow_document_path(workflow_kind, &path)?;
                self.upsert_organization_workflow_document(
                    organization_id,
                    workflow_kind,
                    &normalized_path,
                    &content,
                )
                .await?;

                Ok(ToolExecutionResult {
                    success: true,
                    message: format!("upserted {}", normalized_path),
                })
            }
            SummaryTool::DeleteOrganizationWorkflowFile { path } => {
                let normalized_path = normalize_workflow_document_path(workflow_kind, &path)?;
                let deleted = self
                    .delete_organization_workflow_document(
                        organization_id,
                        workflow_kind,
                        &normalized_path,
                    )
                    .await?;

                Ok(ToolExecutionResult {
                    success: true,
                    message: if deleted {
                        format!("deleted {}", normalized_path)
                    } else {
                        format!("already absent: {}", normalized_path)
                    },
                })
            }
            _ => Ok(ToolExecutionResult {
                success: false,
                message: "tool is not available for this organization workflow".to_owned(),
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

    async fn list_project_blufs_for_organization(
        &self,
        organization_id: &str,
    ) -> Result<Vec<ProjectBlufSnapshot>> {
        let rows = sqlx::query(
            r#"
            SELECT
              projects.project_id,
              project_summaries.content_json,
              project_summaries.updated_at
            FROM projects
            LEFT JOIN project_summaries ON project_summaries.project_id = projects.project_id
            WHERE projects.organization_id = $1
            ORDER BY projects.created_at DESC, projects.project_id DESC
            "#,
        )
        .bind(organization_id)
        .fetch_all(&self.pool)
        .await
        .with_context(|| {
            format!("failed to list project BLUFs for organization {organization_id}")
        })?;

        rows.into_iter()
            .map(|row| {
                let project_id: String = row
                    .try_get("project_id")
                    .context("failed to decode project_id")?;
                let snapshot = row
                    .try_get::<Option<Json<ProjectSnapshot>>, _>("content_json")
                    .context("failed to decode project_summaries.content_json")?
                    .map(|json| normalize_project_snapshot(json.0))
                    .unwrap_or_default();
                let updated_at = row
                    .try_get::<Option<OffsetDateTime>, _>("updated_at")
                    .context("failed to decode project_summaries.updated_at")?
                    .map(format_timestamp)
                    .transpose()?;
                Ok(ProjectBlufSnapshot {
                    project_id: normalize_project_id(&project_id),
                    bluf_markdown: snapshot.bluf_markdown,
                    last_update_at: updated_at.unwrap_or_default(),
                })
            })
            .collect()
    }
}

struct RemoveMemberResult {
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
        member_user_id: row
            .try_get("member_user_id")
            .context("failed to decode member_user_id")?,
        member_name: row
            .try_get("member_name")
            .context("failed to decode member_name")?,
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

fn normalize_project_id(project_id: &str) -> String {
    project_id.trim().to_ascii_uppercase()
}

fn normalize_project_ids(project_ids: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for project_id in project_ids {
        let project_id = normalize_project_id(&project_id);
        if project_id.is_empty() || !seen.insert(project_id.clone()) {
            continue;
        }
        normalized.push(project_id);
    }
    normalized
}

fn normalize_member_snapshot(snapshot: MemberSnapshot) -> MemberSnapshot {
    MemberSnapshot {
        member_user_id: snapshot.member_user_id.trim().to_owned(),
        member_name: snapshot.member_name,
        project_ids: normalize_project_ids(snapshot.project_ids),
        bluf_markdown: snapshot.bluf_markdown,
        last_update_at: snapshot.last_update_at,
    }
}

fn normalize_project_snapshot(snapshot: ProjectSnapshot) -> ProjectSnapshot {
    ProjectSnapshot {
        bluf_markdown: snapshot.bluf_markdown,
        detailed_summary_markdown: snapshot.detailed_summary_markdown,
        members: snapshot
            .members
            .into_iter()
            .map(normalize_member_snapshot)
            .collect(),
    }
}

fn normalize_organization_snapshot(snapshot: OrganizationSnapshot) -> OrganizationSnapshot {
    OrganizationSnapshot {
        bluf_markdown: snapshot.bluf_markdown,
        projects: snapshot.projects,
        members: snapshot
            .members
            .into_iter()
            .map(normalize_member_snapshot)
            .collect(),
    }
}

fn stored_organization_snapshot(snapshot: OrganizationSnapshot) -> OrganizationSnapshot {
    let mut snapshot = normalize_organization_snapshot(snapshot);
    snapshot.projects.clear();
    snapshot
}

fn upsert_member_bluf(
    members: &mut Vec<MemberSnapshot>,
    member_user_id: &str,
    member_name: &str,
    project_ids: Vec<String>,
    markdown: &str,
    updated_at: String,
) {
    let normalized_member_user_id = member_user_id.trim();
    if normalized_member_user_id.is_empty() {
        return;
    }
    if let Some(existing) = members
        .iter_mut()
        .find(|member| member.member_user_id == normalized_member_user_id)
    {
        existing.member_user_id = normalized_member_user_id.to_owned();
        existing.member_name = member_name.to_owned();
        existing.project_ids = project_ids;
        existing.bluf_markdown = markdown.to_owned();
        existing.last_update_at = updated_at;
        return;
    }

    members.push(MemberSnapshot {
        member_user_id: normalized_member_user_id.to_owned(),
        member_name: member_name.to_owned(),
        project_ids,
        bluf_markdown: markdown.to_owned(),
        last_update_at: updated_at,
    });
}

fn remove_member_bluf<T>(
    snapshot: &mut T,
    member_user_id: &str,
    member_name: &str,
) -> RemoveMemberResult
where
    T: MemberSnapshotContainer,
{
    let normalized_member_user_id = member_user_id.trim();
    if normalized_member_user_id.is_empty() {
        return RemoveMemberResult {
            changed: false,
            message: format!("member BLUF already absent for {member_name}"),
        };
    }
    let members = snapshot.members_mut();
    let before = members.len();
    members.retain(|member| member.member_user_id != normalized_member_user_id);

    let changed = members.len() != before;
    RemoveMemberResult {
        changed,
        message: if changed {
            format!("removed member BLUF for {member_name}")
        } else {
            format!("member BLUF already absent for {member_name}")
        },
    }
}

trait MemberSnapshotContainer {
    fn members_mut(&mut self) -> &mut Vec<MemberSnapshot>;
}

impl MemberSnapshotContainer for ProjectSnapshot {
    fn members_mut(&mut self) -> &mut Vec<MemberSnapshot> {
        &mut self.members
    }
}

impl MemberSnapshotContainer for OrganizationSnapshot {
    fn members_mut(&mut self) -> &mut Vec<MemberSnapshot> {
        &mut self.members
    }
}

fn workflow_document_root_label(workflow_kind: WorkflowKind) -> &'static str {
    match workflow_kind {
        WorkflowKind::OrganizationMemories => "memories",
        WorkflowKind::OrganizationSkills => ".codex/skills",
        WorkflowKind::OrganizationSummary | WorkflowKind::ProjectSummary => {
            panic!("workflow does not use organization workflow documents")
        }
    }
}

fn normalize_workflow_document_path(workflow_kind: WorkflowKind, path: &str) -> Result<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        anyhow::bail!("path must be a non-empty relative path");
    }
    if trimmed.contains('\\') {
        anyhow::bail!("path must use '/' separators");
    }

    let mut normalized_components = Vec::new();
    for component in Path::new(trimmed).components() {
        match component {
            Component::Normal(segment) => {
                let segment = segment.to_str().context("path must be valid UTF-8")?.trim();
                if segment.is_empty() {
                    anyhow::bail!("path contains an empty segment");
                }
                normalized_components.push(segment.to_owned());
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                anyhow::bail!("path must stay within the organization workflow root");
            }
        }
    }

    if normalized_components.is_empty() {
        anyhow::bail!("path must contain at least one normal path segment");
    }

    if workflow_kind == WorkflowKind::OrganizationSkills && normalized_components.len() < 2 {
        anyhow::bail!("organization skill files must live under <skill-name>/...");
    }

    Ok(normalized_components.join("/"))
}

fn format_timestamp(timestamp: OffsetDateTime) -> Result<String> {
    timestamp
        .format(&Rfc3339)
        .context("failed to format timestamp as RFC3339")
}

pub(crate) fn now_rfc3339() -> Result<String> {
    format_timestamp(OffsetDateTime::now_utc())
}

#[cfg(test)]
mod tests {
    use super::normalize_workflow_document_path;
    use crate::workflow::WorkflowKind;

    #[test]
    fn memory_document_paths_are_normalized() {
        let path =
            normalize_workflow_document_path(WorkflowKind::OrganizationMemories, "./MEMORY.md")
                .unwrap();

        assert_eq!(path, "MEMORY.md");
    }

    #[test]
    fn workflow_document_paths_reject_parent_segments() {
        let error =
            normalize_workflow_document_path(WorkflowKind::OrganizationMemories, "../MEMORY.md")
                .unwrap_err();

        assert!(error.to_string().contains("workflow root"));
    }

    #[test]
    fn skill_documents_require_skill_subdirectories() {
        let error = normalize_workflow_document_path(WorkflowKind::OrganizationSkills, "SKILL.md")
            .unwrap_err();

        assert!(error.to_string().contains("<skill-name>"));
    }
}
