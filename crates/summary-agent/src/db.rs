use std::collections::HashSet;

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
    workflow::{WorkflowKind, WorkflowScope},
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

pub(crate) struct WorkflowClaim {
    pub(crate) previous_processed_received_at: Option<String>,
}

pub(crate) struct ProjectSummaryClaim {
    pub(crate) last_processed_seq: i64,
}

#[derive(Serialize)]
struct RawMemoryEntry {
    session_id: String,
    content: String,
    updated_at: String,
}

#[derive(Serialize)]
struct SkillEntry {
    name: String,
    body: String,
    updated_at: String,
}

#[derive(Serialize)]
struct ProjectMemorySnapshotView {
    handbook: String,
    memory_summary: String,
    raw: Vec<RawMemoryEntry>,
}

#[derive(Serialize)]
struct OrganizationMemorySnapshotView {
    handbook: String,
    memory_summary: String,
    projects: Vec<OrganizationProjectMemoryView>,
}

#[derive(Serialize)]
struct OrganizationProjectMemoryView {
    project_id: String,
    handbook: String,
    memory_summary: String,
    updated_at: String,
}

#[derive(Serialize)]
struct OrganizationSkillsSnapshotView {
    skills: Vec<SkillEntry>,
    projects: Vec<OrganizationProjectSkillsView>,
}

#[derive(Serialize)]
struct OrganizationProjectSkillsView {
    project_id: String,
    skills: Vec<SkillEntry>,
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

    pub(crate) async fn reset_generating_project_workflows(&self) -> Result<()> {
        sqlx::query("UPDATE project_workflows SET status = 'error' WHERE status = 'generating'")
            .execute(&self.pool)
            .await
            .context("failed to reset generating project workflows")?;
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

    pub(crate) async fn list_projects_with_transcripts(&self) -> Result<Vec<OrganizationProject>> {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT projects.project_id, projects.name
            FROM projects
            INNER JOIN hook_event_transcripts
              ON hook_event_transcripts.project_id = projects.project_id
            ORDER BY projects.project_id ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("failed to list projects with transcripts")?;

        rows.into_iter().map(decode_organization_project).collect()
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

    pub(crate) async fn try_start_workflow(
        &self,
        subject_id: &str,
        workflow_kind: WorkflowKind,
    ) -> Result<Option<WorkflowClaim>> {
        let (sql, normalized_id) = match workflow_kind.scope() {
            WorkflowScope::Organization => (
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
                subject_id.to_owned(),
            ),
            WorkflowScope::Project => (
                r#"
                INSERT INTO project_workflows (
                  project_id,
                  workflow_kind,
                  status,
                  updated_at,
                  last_processed_received_at
                )
                VALUES ($1, $2, 'generating', TO_TIMESTAMP(0), TO_TIMESTAMP(0))
                ON CONFLICT(project_id, workflow_kind) DO UPDATE SET
                  status = 'generating'
                WHERE project_workflows.status <> 'generating'
                RETURNING last_processed_received_at
                "#,
                normalize_project_id(subject_id),
            ),
        };

        let row = sqlx::query(sql)
            .bind(&normalized_id)
            .bind(workflow_kind.as_str())
            .fetch_optional(&self.pool)
            .await
            .with_context(|| {
                format!(
                    "failed to claim workflow {} for {normalized_id}",
                    workflow_kind.as_str()
                )
            })?;

        row.map(|row| {
            Ok(WorkflowClaim {
                previous_processed_received_at: decode_last_processed_received_at(&row)?,
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

    pub(crate) async fn set_workflow_status(
        &self,
        subject_id: &str,
        workflow_kind: WorkflowKind,
        status: SummaryStatus,
    ) -> Result<()> {
        let (sql, normalized_id) = match workflow_kind.scope() {
            WorkflowScope::Organization => (
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
                subject_id.to_owned(),
            ),
            WorkflowScope::Project => (
                r#"
                INSERT INTO project_workflows (
                  project_id,
                  workflow_kind,
                  status,
                  updated_at,
                  last_processed_received_at
                )
                VALUES ($1, $2, $3, TO_TIMESTAMP(0), TO_TIMESTAMP(0))
                ON CONFLICT(project_id, workflow_kind) DO UPDATE SET
                  status = EXCLUDED.status
                "#,
                normalize_project_id(subject_id),
            ),
        };

        sqlx::query(sql)
            .bind(&normalized_id)
            .bind(workflow_kind.as_str())
            .bind(status.as_db_str())
            .execute(&self.pool)
            .await
            .with_context(|| {
                format!(
                    "failed to persist workflow status {} for {normalized_id}",
                    workflow_kind.as_str()
                )
            })?;
        Ok(())
    }

    pub(crate) async fn set_workflow_updated_at(
        &self,
        subject_id: &str,
        workflow_kind: WorkflowKind,
        updated_at: &str,
    ) -> Result<()> {
        let (sql, normalized_id) = match workflow_kind.scope() {
            WorkflowScope::Organization => (
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
                subject_id.to_owned(),
            ),
            WorkflowScope::Project => (
                r#"
                INSERT INTO project_workflows (
                  project_id,
                  workflow_kind,
                  status,
                  updated_at,
                  last_processed_received_at
                )
                VALUES ($1, $2, 'ready', NOW(), $3::timestamptz)
                ON CONFLICT(project_id, workflow_kind) DO UPDATE SET
                  updated_at = EXCLUDED.updated_at,
                  last_processed_received_at = GREATEST(
                    project_workflows.last_processed_received_at,
                    EXCLUDED.last_processed_received_at
                  )
                "#,
                normalize_project_id(subject_id),
            ),
        };

        sqlx::query(sql)
            .bind(&normalized_id)
            .bind(workflow_kind.as_str())
            .bind(updated_at)
            .execute(&self.pool)
            .await
            .with_context(|| {
                format!(
                    "failed to persist workflow updated_at {} for {normalized_id}",
                    workflow_kind.as_str()
                )
            })?;
        Ok(())
    }

    async fn get_project_memory_snapshot(
        &self,
        project_id: &str,
    ) -> Result<ProjectMemorySnapshotView> {
        let (memory, raw) = tokio::try_join!(
            self.load_project_memory_row(project_id),
            self.list_project_memory_raw(project_id),
        )?;
        Ok(ProjectMemorySnapshotView {
            handbook: memory.0,
            memory_summary: memory.1,
            raw,
        })
    }

    async fn load_project_memory_row(&self, project_id: &str) -> Result<(String, String)> {
        let row = sqlx::query(
            r#"
            SELECT handbook_text, summary_text
            FROM project_memory
            WHERE project_id = $1
            "#,
        )
        .bind(project_id)
        .fetch_optional(&self.pool)
        .await
        .with_context(|| format!("failed to fetch project memory for {project_id}"))?;

        Ok(match row {
            Some(row) => (
                row.try_get("handbook_text")
                    .context("failed to decode project_memory.handbook_text")?,
                row.try_get("summary_text")
                    .context("failed to decode project_memory.summary_text")?,
            ),
            None => (String::new(), String::new()),
        })
    }

    async fn list_project_memory_raw(&self, project_id: &str) -> Result<Vec<RawMemoryEntry>> {
        let rows = sqlx::query(
            r#"
            SELECT session_id, content_text, updated_at
            FROM project_memory_raw
            WHERE project_id = $1
            ORDER BY updated_at ASC, session_id ASC
            "#,
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .with_context(|| format!("failed to list project_memory_raw for {project_id}"))?;

        rows.into_iter()
            .map(|row| {
                Ok(RawMemoryEntry {
                    session_id: row
                        .try_get("session_id")
                        .context("failed to decode project_memory_raw.session_id")?,
                    content: row
                        .try_get("content_text")
                        .context("failed to decode project_memory_raw.content_text")?,
                    updated_at: row
                        .try_get::<OffsetDateTime, _>("updated_at")
                        .context("failed to decode project_memory_raw.updated_at")
                        .and_then(format_timestamp)?,
                })
            })
            .collect()
    }

    async fn stage_raw_project_memory(
        &self,
        project_id: &str,
        session_id: &str,
        content: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO project_memory_raw (project_id, session_id, content_text)
            VALUES ($1, $2, $3)
            ON CONFLICT(project_id, session_id) DO UPDATE SET
              content_text = EXCLUDED.content_text,
              updated_at = NOW()
            "#,
        )
        .bind(project_id)
        .bind(session_id)
        .bind(content)
        .execute(&self.pool)
        .await
        .with_context(|| {
            format!("failed to stage raw project memory {session_id} for {project_id}")
        })?;
        Ok(())
    }

    async fn delete_raw_project_memory(
        &self,
        project_id: &str,
        session_id: &str,
    ) -> Result<bool> {
        let result = sqlx::query(
            r#"
            DELETE FROM project_memory_raw
            WHERE project_id = $1 AND session_id = $2
            "#,
        )
        .bind(project_id)
        .bind(session_id)
        .execute(&self.pool)
        .await
        .with_context(|| {
            format!("failed to delete raw project memory {session_id} for {project_id}")
        })?;
        Ok(result.rows_affected() > 0)
    }

    async fn set_project_handbook(&self, project_id: &str, handbook: &str) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO project_memory (project_id, handbook_text, summary_text, updated_at)
            VALUES ($1, $2, '', NOW())
            ON CONFLICT(project_id) DO UPDATE SET
              handbook_text = EXCLUDED.handbook_text,
              updated_at = NOW()
            "#,
        )
        .bind(project_id)
        .bind(handbook)
        .execute(&self.pool)
        .await
        .with_context(|| format!("failed to persist project handbook for {project_id}"))?;
        Ok(())
    }

    async fn set_project_memory_summary(&self, project_id: &str, summary: &str) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO project_memory (project_id, handbook_text, summary_text, updated_at)
            VALUES ($1, '', $2, NOW())
            ON CONFLICT(project_id) DO UPDATE SET
              summary_text = EXCLUDED.summary_text,
              updated_at = NOW()
            "#,
        )
        .bind(project_id)
        .bind(summary)
        .execute(&self.pool)
        .await
        .with_context(|| format!("failed to persist project memory summary for {project_id}"))?;
        Ok(())
    }

    async fn list_project_skills(&self, project_id: &str) -> Result<Vec<SkillEntry>> {
        let rows = sqlx::query(
            r#"
            SELECT skill_name, content_text, updated_at
            FROM project_skills
            WHERE project_id = $1
            ORDER BY skill_name ASC
            "#,
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .with_context(|| format!("failed to list project_skills for {project_id}"))?;

        rows.into_iter().map(decode_skill_row).collect()
    }

    async fn upsert_project_skill(
        &self,
        project_id: &str,
        skill_name: &str,
        body: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO project_skills (project_id, skill_name, content_text)
            VALUES ($1, $2, $3)
            ON CONFLICT(project_id, skill_name) DO UPDATE SET
              content_text = EXCLUDED.content_text,
              updated_at = NOW()
            "#,
        )
        .bind(project_id)
        .bind(skill_name)
        .bind(body)
        .execute(&self.pool)
        .await
        .with_context(|| format!("failed to upsert project skill {skill_name} for {project_id}"))?;
        Ok(())
    }

    async fn delete_project_skill(&self, project_id: &str, skill_name: &str) -> Result<bool> {
        let result = sqlx::query(
            r#"
            DELETE FROM project_skills
            WHERE project_id = $1 AND skill_name = $2
            "#,
        )
        .bind(project_id)
        .bind(skill_name)
        .execute(&self.pool)
        .await
        .with_context(|| format!("failed to delete project skill {skill_name} for {project_id}"))?;
        Ok(result.rows_affected() > 0)
    }

    async fn get_organization_memory_snapshot(
        &self,
        organization_id: &str,
    ) -> Result<OrganizationMemorySnapshotView> {
        let (memory, projects) = tokio::try_join!(
            self.load_organization_memory_row(organization_id),
            self.list_project_memories_for_organization(organization_id),
        )?;
        Ok(OrganizationMemorySnapshotView {
            handbook: memory.0,
            memory_summary: memory.1,
            projects,
        })
    }

    async fn load_organization_memory_row(
        &self,
        organization_id: &str,
    ) -> Result<(String, String)> {
        let row = sqlx::query(
            r#"
            SELECT handbook_text, summary_text
            FROM organization_memory
            WHERE organization_id = $1
            "#,
        )
        .bind(organization_id)
        .fetch_optional(&self.pool)
        .await
        .with_context(|| format!("failed to fetch organization memory for {organization_id}"))?;

        Ok(match row {
            Some(row) => (
                row.try_get("handbook_text")
                    .context("failed to decode organization_memory.handbook_text")?,
                row.try_get("summary_text")
                    .context("failed to decode organization_memory.summary_text")?,
            ),
            None => (String::new(), String::new()),
        })
    }

    async fn list_project_memories_for_organization(
        &self,
        organization_id: &str,
    ) -> Result<Vec<OrganizationProjectMemoryView>> {
        let rows = sqlx::query(
            r#"
            SELECT
              p.project_id,
              COALESCE(m.handbook_text, '') AS handbook_text,
              COALESCE(m.summary_text, '') AS summary_text,
              m.updated_at
            FROM projects AS p
            LEFT JOIN project_memory AS m ON m.project_id = p.project_id
            WHERE p.organization_id = $1
              AND (m.handbook_text IS NOT NULL AND m.handbook_text <> ''
                   OR m.summary_text IS NOT NULL AND m.summary_text <> '')
            ORDER BY p.project_id ASC
            "#,
        )
        .bind(organization_id)
        .fetch_all(&self.pool)
        .await
        .with_context(|| {
            format!("failed to list per-project memory for organization {organization_id}")
        })?;

        rows.into_iter()
            .map(|row| {
                let updated_at = row
                    .try_get::<Option<OffsetDateTime>, _>("updated_at")
                    .context("failed to decode project_memory.updated_at")?
                    .map(format_timestamp)
                    .transpose()?
                    .unwrap_or_default();
                Ok(OrganizationProjectMemoryView {
                    project_id: row
                        .try_get("project_id")
                        .context("failed to decode project_id")?,
                    handbook: row
                        .try_get("handbook_text")
                        .context("failed to decode project_memory.handbook_text")?,
                    memory_summary: row
                        .try_get("summary_text")
                        .context("failed to decode project_memory.summary_text")?,
                    updated_at,
                })
            })
            .collect()
    }

    async fn set_organization_handbook(
        &self,
        organization_id: &str,
        handbook: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO organization_memory (organization_id, handbook_text, summary_text, updated_at)
            VALUES ($1, $2, '', NOW())
            ON CONFLICT(organization_id) DO UPDATE SET
              handbook_text = EXCLUDED.handbook_text,
              updated_at = NOW()
            "#,
        )
        .bind(organization_id)
        .bind(handbook)
        .execute(&self.pool)
        .await
        .with_context(|| {
            format!("failed to persist organization handbook for {organization_id}")
        })?;
        Ok(())
    }

    async fn set_organization_memory_summary(
        &self,
        organization_id: &str,
        summary: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO organization_memory (organization_id, handbook_text, summary_text, updated_at)
            VALUES ($1, '', $2, NOW())
            ON CONFLICT(organization_id) DO UPDATE SET
              summary_text = EXCLUDED.summary_text,
              updated_at = NOW()
            "#,
        )
        .bind(organization_id)
        .bind(summary)
        .execute(&self.pool)
        .await
        .with_context(|| {
            format!("failed to persist organization memory summary for {organization_id}")
        })?;
        Ok(())
    }

    async fn get_organization_skills_snapshot(
        &self,
        organization_id: &str,
    ) -> Result<OrganizationSkillsSnapshotView> {
        let (skills, projects) = tokio::try_join!(
            self.list_organization_skills(organization_id),
            self.list_project_skills_for_organization(organization_id),
        )?;
        Ok(OrganizationSkillsSnapshotView { skills, projects })
    }

    async fn list_organization_skills(&self, organization_id: &str) -> Result<Vec<SkillEntry>> {
        let rows = sqlx::query(
            r#"
            SELECT skill_name, content_text, updated_at
            FROM organization_skills
            WHERE organization_id = $1
            ORDER BY skill_name ASC
            "#,
        )
        .bind(organization_id)
        .fetch_all(&self.pool)
        .await
        .with_context(|| format!("failed to list organization_skills for {organization_id}"))?;

        rows.into_iter().map(decode_skill_row).collect()
    }

    async fn list_project_skills_for_organization(
        &self,
        organization_id: &str,
    ) -> Result<Vec<OrganizationProjectSkillsView>> {
        let rows = sqlx::query(
            r#"
            SELECT
              s.project_id,
              s.skill_name,
              s.content_text,
              s.updated_at
            FROM project_skills AS s
            INNER JOIN projects AS p ON p.project_id = s.project_id
            WHERE p.organization_id = $1
            ORDER BY s.project_id ASC, s.skill_name ASC
            "#,
        )
        .bind(organization_id)
        .fetch_all(&self.pool)
        .await
        .with_context(|| {
            format!("failed to list per-project skills for organization {organization_id}")
        })?;

        let mut grouped: Vec<OrganizationProjectSkillsView> = Vec::new();
        for row in rows {
            let project_id: String = row
                .try_get("project_id")
                .context("failed to decode project_id")?;
            let skill = SkillEntry {
                name: row
                    .try_get("skill_name")
                    .context("failed to decode project_skills.skill_name")?,
                body: row
                    .try_get("content_text")
                    .context("failed to decode project_skills.content_text")?,
                updated_at: row
                    .try_get::<OffsetDateTime, _>("updated_at")
                    .context("failed to decode project_skills.updated_at")
                    .and_then(format_timestamp)?,
            };
            if let Some(last) = grouped.last_mut()
                && last.project_id == project_id
            {
                last.skills.push(skill);
            } else {
                grouped.push(OrganizationProjectSkillsView {
                    project_id,
                    skills: vec![skill],
                });
            }
        }
        Ok(grouped)
    }

    async fn upsert_organization_skill(
        &self,
        organization_id: &str,
        skill_name: &str,
        body: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO organization_skills (organization_id, skill_name, content_text)
            VALUES ($1, $2, $3)
            ON CONFLICT(organization_id, skill_name) DO UPDATE SET
              content_text = EXCLUDED.content_text,
              updated_at = NOW()
            "#,
        )
        .bind(organization_id)
        .bind(skill_name)
        .bind(body)
        .execute(&self.pool)
        .await
        .with_context(|| {
            format!("failed to upsert organization skill {skill_name} for {organization_id}")
        })?;
        Ok(())
    }

    async fn delete_organization_skill(
        &self,
        organization_id: &str,
        skill_name: &str,
    ) -> Result<bool> {
        let result = sqlx::query(
            r#"
            DELETE FROM organization_skills
            WHERE organization_id = $1 AND skill_name = $2
            "#,
        )
        .bind(organization_id)
        .bind(skill_name)
        .execute(&self.pool)
        .await
        .with_context(|| {
            format!("failed to delete organization skill {skill_name} for {organization_id}")
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

        rows.into_iter().map(decode_organization_project).collect()
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

    pub(crate) async fn query_project_transcripts_for_workflow(
        &self,
        project_id: &str,
        options: OrganizationWorkflowQueryOptions,
    ) -> Result<Vec<OrganizationTranscript>> {
        let normalized_project_id = normalize_project_id(project_id);
        let rows = sqlx::query(
            r#"
            SELECT
              t.session_id,
              t.project_id,
              r.name AS project_name,
              t.member_user_id,
              t.member_name,
              t.client,
              t.repo_root,
              t.branch,
              t.received_at,
              t.transcript_path,
              t.content_text
            FROM hook_event_transcripts AS t
            INNER JOIN projects AS r ON r.project_id = t.project_id
            WHERE t.project_id = $1
              AND ($2::timestamptz IS NULL OR t.received_at > $2::timestamptz)
              AND ($3::timestamptz IS NULL OR t.received_at <= $3::timestamptz)
            ORDER BY t.received_at ASC, t.session_id ASC
            LIMIT COALESCE($4, 9223372036854775807)
            "#,
        )
        .bind(&normalized_project_id)
        .bind(options.after_received_at)
        .bind(options.before_received_at)
        .bind(options.limit)
        .fetch_all(&self.pool)
        .await
        .with_context(|| {
            format!("failed to query project transcripts for workflow {normalized_project_id}")
        })?;

        rows.into_iter().map(map_organization_transcript).collect()
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

    pub(crate) async fn execute_workflow_tool_call(
        &self,
        subject_id: &str,
        workflow_kind: WorkflowKind,
        tool: SummaryTool,
    ) -> Result<ToolExecutionResult> {
        let normalized_id = match workflow_kind.scope() {
            WorkflowScope::Organization => subject_id.to_owned(),
            WorkflowScope::Project => normalize_project_id(subject_id),
        };
        match (workflow_kind, tool) {
            (WorkflowKind::ProjectMemoryExtract, SummaryTool::WorkflowGetSnapshot)
            | (WorkflowKind::ProjectMemoryConsolidate, SummaryTool::WorkflowGetSnapshot) => {
                let snapshot = self.get_project_memory_snapshot(&normalized_id).await?;
                serialize_snapshot(&snapshot, "project memory snapshot")
            }
            (
                WorkflowKind::ProjectMemoryExtract,
                SummaryTool::StageRawProjectMemory {
                    session_id,
                    markdown,
                },
            ) => {
                let session_id = session_id.trim();
                if session_id.is_empty() {
                    return Ok(ToolExecutionResult {
                        success: false,
                        message: "session_id must be a non-empty string".to_owned(),
                    });
                }
                self.stage_raw_project_memory(&normalized_id, session_id, &markdown)
                    .await?;
                Ok(ToolExecutionResult {
                    success: true,
                    message: format!("staged raw memory for session {session_id}"),
                })
            }
            (
                WorkflowKind::ProjectMemoryConsolidate,
                SummaryTool::DeleteRawProjectMemory { session_id },
            ) => {
                let session_id = session_id.trim();
                if session_id.is_empty() {
                    return Ok(ToolExecutionResult {
                        success: false,
                        message: "session_id must be a non-empty string".to_owned(),
                    });
                }
                let deleted = self
                    .delete_raw_project_memory(&normalized_id, session_id)
                    .await?;
                Ok(ToolExecutionResult {
                    success: true,
                    message: if deleted {
                        format!("deleted raw memory for session {session_id}")
                    } else {
                        format!("raw memory already absent for session {session_id}")
                    },
                })
            }
            (WorkflowKind::ProjectMemoryConsolidate, SummaryTool::SetHandbook { markdown }) => {
                self.set_project_handbook(&normalized_id, markdown.trim())
                    .await?;
                Ok(ToolExecutionResult {
                    success: true,
                    message: "updated project handbook".to_owned(),
                })
            }
            (
                WorkflowKind::ProjectMemoryConsolidate,
                SummaryTool::SetMemorySummary { markdown },
            ) => {
                self.set_project_memory_summary(&normalized_id, markdown.trim())
                    .await?;
                Ok(ToolExecutionResult {
                    success: true,
                    message: "updated project memory summary".to_owned(),
                })
            }
            (WorkflowKind::ProjectSkills, SummaryTool::WorkflowGetSnapshot) => {
                let skills = self.list_project_skills(&normalized_id).await?;
                serialize_snapshot(
                    &serde_json::json!({ "skills": skills }),
                    "project skills snapshot",
                )
            }
            (WorkflowKind::ProjectSkills, SummaryTool::UpsertSkill { name, body }) => {
                let name = name.trim();
                if name.is_empty() {
                    return Ok(ToolExecutionResult {
                        success: false,
                        message: "skill name must be a non-empty string".to_owned(),
                    });
                }
                self.upsert_project_skill(&normalized_id, name, &body)
                    .await?;
                Ok(ToolExecutionResult {
                    success: true,
                    message: format!("upserted skill {name}"),
                })
            }
            (WorkflowKind::ProjectSkills, SummaryTool::DeleteSkill { name }) => {
                let name = name.trim();
                if name.is_empty() {
                    return Ok(ToolExecutionResult {
                        success: false,
                        message: "skill name must be a non-empty string".to_owned(),
                    });
                }
                let deleted = self.delete_project_skill(&normalized_id, name).await?;
                Ok(ToolExecutionResult {
                    success: true,
                    message: if deleted {
                        format!("deleted skill {name}")
                    } else {
                        format!("skill already absent: {name}")
                    },
                })
            }
            (
                WorkflowKind::OrganizationMemoryConsolidate,
                SummaryTool::WorkflowGetSnapshot,
            ) => {
                let snapshot = self
                    .get_organization_memory_snapshot(&normalized_id)
                    .await?;
                serialize_snapshot(&snapshot, "organization memory snapshot")
            }
            (
                WorkflowKind::OrganizationMemoryConsolidate,
                SummaryTool::SetHandbook { markdown },
            ) => {
                self.set_organization_handbook(&normalized_id, markdown.trim())
                    .await?;
                Ok(ToolExecutionResult {
                    success: true,
                    message: "updated organization handbook".to_owned(),
                })
            }
            (
                WorkflowKind::OrganizationMemoryConsolidate,
                SummaryTool::SetMemorySummary { markdown },
            ) => {
                self.set_organization_memory_summary(&normalized_id, markdown.trim())
                    .await?;
                Ok(ToolExecutionResult {
                    success: true,
                    message: "updated organization memory summary".to_owned(),
                })
            }
            (WorkflowKind::OrganizationSkills, SummaryTool::WorkflowGetSnapshot) => {
                let snapshot = self
                    .get_organization_skills_snapshot(&normalized_id)
                    .await?;
                serialize_snapshot(&snapshot, "organization skills snapshot")
            }
            (WorkflowKind::OrganizationSkills, SummaryTool::UpsertSkill { name, body }) => {
                let name = name.trim();
                if name.is_empty() {
                    return Ok(ToolExecutionResult {
                        success: false,
                        message: "skill name must be a non-empty string".to_owned(),
                    });
                }
                self.upsert_organization_skill(&normalized_id, name, &body)
                    .await?;
                Ok(ToolExecutionResult {
                    success: true,
                    message: format!("upserted skill {name}"),
                })
            }
            (WorkflowKind::OrganizationSkills, SummaryTool::DeleteSkill { name }) => {
                let name = name.trim();
                if name.is_empty() {
                    return Ok(ToolExecutionResult {
                        success: false,
                        message: "skill name must be a non-empty string".to_owned(),
                    });
                }
                let deleted = self.delete_organization_skill(&normalized_id, name).await?;
                Ok(ToolExecutionResult {
                    success: true,
                    message: if deleted {
                        format!("deleted skill {name}")
                    } else {
                        format!("skill already absent: {name}")
                    },
                })
            }
            (kind, _) => Ok(ToolExecutionResult {
                success: false,
                message: format!("tool is not available for {} workflow", kind.as_str()),
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

fn decode_organization_project(row: sqlx::postgres::PgRow) -> Result<OrganizationProject> {
    Ok(OrganizationProject {
        project_id: row
            .try_get("project_id")
            .context("failed to decode project_id")?,
        name: row
            .try_get("name")
            .context("failed to decode project name")?,
    })
}

fn decode_last_processed_received_at(row: &sqlx::postgres::PgRow) -> Result<Option<String>> {
    Ok(row
        .try_get::<Option<OffsetDateTime>, _>("last_processed_received_at")
        .context("failed to decode last_processed_received_at")?
        .map(format_timestamp)
        .transpose()?
        .filter(|value| value != "1970-01-01T00:00:00Z"))
}

fn decode_skill_row(row: sqlx::postgres::PgRow) -> Result<SkillEntry> {
    Ok(SkillEntry {
        name: row
            .try_get("skill_name")
            .context("failed to decode skill_name")?,
        body: row
            .try_get("content_text")
            .context("failed to decode skill content_text")?,
        updated_at: row
            .try_get::<OffsetDateTime, _>("updated_at")
            .context("failed to decode skill updated_at")
            .and_then(format_timestamp)?,
    })
}

fn serialize_snapshot<T: Serialize>(snapshot: &T, context: &str) -> Result<ToolExecutionResult> {
    Ok(ToolExecutionResult {
        success: true,
        message: serde_json::to_string_pretty(snapshot)
            .with_context(|| format!("failed to serialize {context}"))?,
    })
}

fn map_organization_transcript(row: sqlx::postgres::PgRow) -> Result<OrganizationTranscript> {
    Ok(OrganizationTranscript {
        session_id: row
            .try_get("session_id")
            .context("failed to decode session_id")?,
        project_id: row
            .try_get("project_id")
            .context("failed to decode project_id")?,
        project_name: row
            .try_get("project_name")
            .context("failed to decode project_name")?,
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
        received_at: format_timestamp(
            row.try_get::<OffsetDateTime, _>("received_at")
                .context("failed to decode received_at")?,
        )?,
        transcript_path: row
            .try_get("transcript_path")
            .context("failed to decode transcript_path")?,
        transcript_text: row
            .try_get("content_text")
            .context("failed to decode content_text")?,
    })
}

fn format_timestamp(timestamp: OffsetDateTime) -> Result<String> {
    timestamp
        .format(&Rfc3339)
        .context("failed to format timestamp as RFC3339")
}

pub(crate) fn now_rfc3339() -> Result<String> {
    format_timestamp(OffsetDateTime::now_utc())
}

