use std::{collections::HashMap, future::pending, time::Duration};

use anyhow::{Context, Result};
use reporter_protocol::SummaryStatus;
use tokio::{
    sync::mpsc,
    time::{self, Instant, Interval, MissedTickBehavior},
};

use crate::{
    agent::{AgentCommand, AgentEvent},
    db::{
        OrganizationSummaryQueryOptions, OrganizationWorkflowQueryOptions,
        ProjectSummaryQueryOptions, SummaryDb, now_rfc3339,
    },
    event::{
        ProjectSkillsRequest, build_organization_summary_source_window_key,
        format_organization_memory_consolidate_request, format_organization_skills_request,
        format_organization_summary_request, format_project_event,
        format_project_memory_consolidate_request, format_project_memory_extract_request,
        render_project_skills_request,
    },
    workflow::{
        WorkflowCursor, WorkflowCursorSecondary, WorkflowDispatch, WorkflowKind, WorkflowTarget,
    },
};

const ORGANIZATION_SUMMARY_EVENT_LIMIT: i64 = 500;
const PROJECT_TRANSCRIPT_LIMIT: i64 = 24;
const PROJECT_MEMORY_EXTRACT_TRANSCRIPT_LIMIT: i64 = 1;
const PROJECT_SUMMARY_EVENT_LIMIT: i64 = 200;
const PROJECT_SUMMARY_SWEEP_LIMIT: i64 = 50;

pub(crate) struct WorkflowCoordinator {
    db: SummaryDb,
    command_tx: mpsc::Sender<AgentCommand>,
    event_rx: mpsc::Receiver<AgentEvent>,
    organization_summary_refresh_interval: Duration,
    project_summary_poll_interval: Duration,
    project_memory_extract_interval: Duration,
    project_memory_consolidate_interval: Duration,
    project_skills_interval: Duration,
    organization_memory_consolidate_interval: Duration,
    organization_skills_interval: Duration,
    pending_workflow_cursor: HashMap<WorkflowTarget, WorkflowCursor>,
}

impl WorkflowCoordinator {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        db: SummaryDb,
        command_tx: mpsc::Sender<AgentCommand>,
        event_rx: mpsc::Receiver<AgentEvent>,
        organization_summary_refresh_interval: Duration,
        project_summary_poll_interval: Duration,
        project_memory_extract_interval: Duration,
        project_memory_consolidate_interval: Duration,
        project_skills_interval: Duration,
        organization_memory_consolidate_interval: Duration,
        organization_skills_interval: Duration,
    ) -> Self {
        Self {
            db,
            command_tx,
            event_rx,
            organization_summary_refresh_interval,
            project_summary_poll_interval,
            project_memory_extract_interval,
            project_memory_consolidate_interval,
            project_skills_interval,
            organization_memory_consolidate_interval,
            organization_skills_interval,
            pending_workflow_cursor: HashMap::new(),
        }
    }

    pub(crate) async fn run(&mut self) -> Result<()> {
        tokio::try_join!(
            self.db.reset_generating_organization_summaries(),
            self.db.reset_generating_project_summaries(),
            self.db.reset_generating_organization_workflows(),
            self.db.reset_generating_project_workflows(),
        )?;

        self.run_organization_summary_sweep().await?;
        self.run_project_summary_sweep().await?;
        self.run_project_transcript_sweep(WorkflowKind::ProjectMemoryExtract)
            .await?;
        self.run_project_periodic_sweep(WorkflowKind::ProjectMemoryConsolidate)
            .await?;
        self.run_project_transcript_sweep(WorkflowKind::ProjectSkills)
            .await?;
        self.run_organization_periodic_sweep(WorkflowKind::OrganizationMemoryConsolidate)
            .await?;
        self.run_organization_periodic_sweep(WorkflowKind::OrganizationSkills)
            .await?;

        let mut organization_summary_interval =
            create_interval(self.organization_summary_refresh_interval);
        let mut project_summary_interval = create_interval(self.project_summary_poll_interval);
        let mut project_memory_extract_interval =
            create_interval(self.project_memory_extract_interval);
        let mut project_memory_consolidate_interval =
            create_interval(self.project_memory_consolidate_interval);
        let mut project_skills_interval = create_interval(self.project_skills_interval);
        let mut organization_memory_consolidate_interval =
            create_interval(self.organization_memory_consolidate_interval);
        let mut organization_skills_interval = create_interval(self.organization_skills_interval);
        let mut shutdown = Box::pin(shutdown_signal());

        loop {
            tokio::select! {
                maybe_event = self.event_rx.recv() => {
                    let event = maybe_event.context("summary agent loop exited unexpectedly")?;
                    self.handle_agent_event(event).await?;
                }
                _ = wait_for_interval(&mut organization_summary_interval) => {
                    self.run_organization_summary_sweep().await?;
                }
                _ = wait_for_interval(&mut project_summary_interval) => {
                    self.run_project_summary_sweep().await?;
                }
                _ = wait_for_interval(&mut project_memory_extract_interval) => {
                    self.run_project_transcript_sweep(WorkflowKind::ProjectMemoryExtract).await?;
                }
                _ = wait_for_interval(&mut project_memory_consolidate_interval) => {
                    self.run_project_periodic_sweep(WorkflowKind::ProjectMemoryConsolidate).await?;
                }
                _ = wait_for_interval(&mut project_skills_interval) => {
                    self.run_project_transcript_sweep(WorkflowKind::ProjectSkills).await?;
                }
                _ = wait_for_interval(&mut organization_memory_consolidate_interval) => {
                    self.run_organization_periodic_sweep(WorkflowKind::OrganizationMemoryConsolidate).await?;
                }
                _ = wait_for_interval(&mut organization_skills_interval) => {
                    self.run_organization_periodic_sweep(WorkflowKind::OrganizationSkills).await?;
                }
                _ = &mut shutdown => break,
            }
        }

        Ok(())
    }

    async fn handle_agent_event(&mut self, event: AgentEvent) -> Result<()> {
        match event {
            AgentEvent::WorkflowStatus { target, status } => {
                self.persist_workflow_status(&target, status).await?;
            }
        }

        Ok(())
    }

    async fn persist_workflow_status(
        &mut self,
        target: &WorkflowTarget,
        status: SummaryStatus,
    ) -> Result<()> {
        if status == SummaryStatus::Ready
            && let Some(cursor) = self.pending_workflow_cursor.get(target).cloned()
        {
            match (target.kind, cursor) {
                (
                    WorkflowKind::OrganizationSummary,
                    WorkflowCursor::ReceivedAt {
                        received_at,
                        secondary,
                    },
                ) => {
                    let last_seq = match secondary {
                        Some(WorkflowCursorSecondary::Seq(seq)) => Some(seq),
                        Some(WorkflowCursorSecondary::SessionId(_)) | None => None,
                    };
                    self.db
                        .set_organization_summary_updated_at(&target.id, &received_at, last_seq)
                        .await?;
                }
                (WorkflowKind::ProjectSummary, WorkflowCursor::Seq(last_processed_seq)) => {
                    self.db
                        .set_project_summary_last_processed_seq(&target.id, last_processed_seq)
                        .await?;
                }
                (
                    kind,
                    WorkflowCursor::ReceivedAt {
                        received_at,
                        secondary,
                    },
                ) => {
                    let last_session_id = match &secondary {
                        Some(WorkflowCursorSecondary::SessionId(session_id)) => {
                            Some(session_id.as_str())
                        }
                        Some(WorkflowCursorSecondary::Seq(_)) | None => None,
                    };
                    self.db
                        .set_workflow_updated_at(&target.id, kind, &received_at, last_session_id)
                        .await?;
                }
                _ => {}
            }
        }

        match target.kind {
            WorkflowKind::OrganizationSummary => {
                self.db
                    .set_organization_summary_status(&target.id, status)
                    .await?;
            }
            WorkflowKind::ProjectSummary => {
                self.db
                    .set_project_summary_status(&target.id, status)
                    .await?;
            }
            kind => {
                self.db
                    .set_workflow_status(&target.id, kind, status)
                    .await?;
            }
        }

        if matches!(status, SummaryStatus::Ready | SummaryStatus::Error) {
            self.pending_workflow_cursor.remove(target);
        }

        Ok(())
    }

    async fn run_organization_summary_sweep(&mut self) -> Result<()> {
        let organization_ids = self.db.list_organizations_with_projects().await?;

        for organization_id in organization_ids {
            let target = WorkflowTarget::new(WorkflowKind::OrganizationSummary, organization_id);
            if self.pending_workflow_cursor.contains_key(&target) {
                continue;
            }

            let claim = match self.db.try_start_organization_summary(&target.id).await {
                Ok(Some(claim)) => claim,
                Ok(None) => continue,
                Err(error) => {
                    self.mark_error(&target, "claim", &error).await;
                    continue;
                }
            };

            if let Err(error) = self
                .enqueue_organization_summary(
                    &target,
                    claim.previous_summary_updated_at,
                    claim.previous_last_processed_seq,
                )
                .await
            {
                self.mark_error(&target, "enqueue", &error).await;
                self.pending_workflow_cursor.remove(&target);
            }
        }

        Ok(())
    }

    async fn enqueue_organization_summary(
        &mut self,
        target: &WorkflowTarget,
        previous_summary_updated_at: Option<String>,
        previous_last_processed_seq: Option<i64>,
    ) -> Result<()> {
        let heartbeat_cutoff = now_rfc3339()?;
        let events = self
            .db
            .query_organization_events_for_summary(
                &target.id,
                OrganizationSummaryQueryOptions {
                    after_received_at: previous_summary_updated_at,
                    after_seq: previous_last_processed_seq,
                    before_received_at: Some(heartbeat_cutoff.clone()),
                    limit: Some(ORGANIZATION_SUMMARY_EVENT_LIMIT),
                },
            )
            .await?;

        let (summary_updated_at, at_limit) = received_at_workflow_cutoff(
            &events
                .iter()
                .map(|event| event.event.received_at.as_str())
                .collect::<Vec<_>>(),
            ORGANIZATION_SUMMARY_EVENT_LIMIT,
            &heartbeat_cutoff,
        );
        let secondary = if at_limit {
            events
                .last()
                .map(|event| WorkflowCursorSecondary::Seq(event.event.seq))
        } else {
            None
        };
        self.pending_workflow_cursor.insert(
            target.clone(),
            WorkflowCursor::ReceivedAt {
                received_at: summary_updated_at.to_owned(),
                secondary,
            },
        );

        if events.is_empty() {
            self.persist_workflow_status(target, SummaryStatus::Ready)
                .await?;
            return Ok(());
        }

        let projects = self.db.list_projects_for_summary(&target.id).await?;
        let source_window_key = build_organization_summary_source_window_key(
            previous_summary_updated_at.as_deref(),
            previous_last_processed_seq,
            summary_updated_at,
        );
        self.dispatch_workflow(WorkflowDispatch {
            target: target.clone(),
            input: format_organization_summary_request(&projects, &events, &source_window_key)?,
        })
        .await
    }

    async fn run_project_summary_sweep(&mut self) -> Result<()> {
        let projects = self
            .db
            .list_projects_needing_summary(PROJECT_SUMMARY_SWEEP_LIMIT)
            .await?;

        for project in projects {
            let target =
                WorkflowTarget::new(WorkflowKind::ProjectSummary, project.project_id.clone());
            if self.pending_workflow_cursor.contains_key(&target) {
                continue;
            }

            let claim = match self.db.try_start_project_summary(&target.id).await {
                Ok(Some(claim)) => claim,
                Ok(None) => continue,
                Err(error) => {
                    self.mark_error(&target, "claim", &error).await;
                    continue;
                }
            };

            if let Err(error) = self
                .enqueue_project_summary(&target, &project.name, claim.last_processed_seq)
                .await
            {
                self.mark_error(&target, "enqueue", &error).await;
                self.pending_workflow_cursor.remove(&target);
            }
        }

        Ok(())
    }

    async fn enqueue_project_summary(
        &mut self,
        target: &WorkflowTarget,
        project_name: &str,
        last_processed_seq: i64,
    ) -> Result<()> {
        let events = self
            .db
            .query_project_events_for_summary(
                &target.id,
                ProjectSummaryQueryOptions {
                    after_seq: Some(last_processed_seq),
                    limit: Some(PROJECT_SUMMARY_EVENT_LIMIT),
                },
            )
            .await?;

        let Some(last_seq) = events.last().map(|event| event.seq) else {
            self.db
                .set_project_summary_status(&target.id, SummaryStatus::Ready)
                .await?;
            return Ok(());
        };

        self.pending_workflow_cursor
            .insert(target.clone(), WorkflowCursor::Seq(last_seq));

        for event in events {
            self.dispatch_workflow(WorkflowDispatch {
                target: target.clone(),
                input: format_project_event(&target.id, project_name, &event)?,
            })
            .await?;
        }

        Ok(())
    }

    async fn run_project_transcript_sweep(&mut self, kind: WorkflowKind) -> Result<()> {
        debug_assert!(matches!(
            kind,
            WorkflowKind::ProjectMemoryExtract | WorkflowKind::ProjectSkills
        ));

        let projects = self.db.list_projects_with_transcripts().await?;

        for project in projects {
            let target = WorkflowTarget::new(kind, project.project_id.clone());
            if self.pending_workflow_cursor.contains_key(&target) {
                continue;
            }

            let claim = match self.db.try_start_workflow(&target.id, kind).await {
                Ok(Some(claim)) => claim,
                Ok(None) => continue,
                Err(error) => {
                    self.mark_error(&target, "claim", &error).await;
                    continue;
                }
            };

            if let Err(error) = self
                .enqueue_project_transcript_workflow(
                    &target,
                    &project.name,
                    claim.previous_processed_received_at,
                    claim.previous_last_processed_session_id,
                )
                .await
            {
                self.mark_error(&target, "enqueue", &error).await;
                self.pending_workflow_cursor.remove(&target);
            }
        }

        Ok(())
    }

    async fn enqueue_project_transcript_workflow(
        &mut self,
        target: &WorkflowTarget,
        project_name: &str,
        previous_processed_received_at: Option<String>,
        previous_last_processed_session_id: Option<String>,
    ) -> Result<()> {
        let heartbeat_cutoff = now_rfc3339()?;
        let transcript_limit = match target.kind {
            WorkflowKind::ProjectMemoryExtract => PROJECT_MEMORY_EXTRACT_TRANSCRIPT_LIMIT,
            WorkflowKind::ProjectSkills => PROJECT_TRANSCRIPT_LIMIT,
            _ => unreachable!(),
        };
        let transcripts = self
            .db
            .query_project_transcripts_for_workflow(
                &target.id,
                OrganizationWorkflowQueryOptions {
                    after_received_at: previous_processed_received_at.clone(),
                    after_session_id: previous_last_processed_session_id,
                    before_received_at: Some(heartbeat_cutoff.clone()),
                    limit: Some(transcript_limit),
                },
            )
            .await?;

        if transcripts.is_empty() {
            self.persist_workflow_status(target, SummaryStatus::Ready)
                .await?;
            return Ok(());
        }

        match target.kind {
            WorkflowKind::ProjectMemoryExtract => {
                self.pending_workflow_cursor.insert(
                    target.clone(),
                    transcript_workflow_cursor(
                        &transcripts,
                        transcripts.len(),
                        transcript_limit,
                        &heartbeat_cutoff,
                    ),
                );
                for transcript in &transcripts {
                    self.dispatch_workflow(WorkflowDispatch {
                        target: target.clone(),
                        input: format_project_memory_extract_request(transcript)?,
                    })
                    .await?;
                }
                Ok(())
            }
            WorkflowKind::ProjectSkills => {
                let project = crate::event::OrganizationProject {
                    project_id: target.id.clone(),
                    name: project_name.to_owned(),
                };
                let rendered = render_project_skills_request(ProjectSkillsRequest {
                    project: &project,
                    transcripts: &transcripts,
                    previous_processed_received_at: previous_processed_received_at.as_deref(),
                    heartbeat_cutoff: &heartbeat_cutoff,
                })?;
                let included_transcript_count = rendered.included_transcript_count.max(1);
                self.pending_workflow_cursor.insert(
                    target.clone(),
                    transcript_workflow_cursor(
                        &transcripts,
                        included_transcript_count,
                        transcript_limit,
                        &heartbeat_cutoff,
                    ),
                );
                self.dispatch_workflow(WorkflowDispatch {
                    target: target.clone(),
                    input: rendered.input,
                })
                .await
            }
            _ => unreachable!(),
        }
    }

    async fn run_project_periodic_sweep(&mut self, kind: WorkflowKind) -> Result<()> {
        debug_assert!(matches!(kind, WorkflowKind::ProjectMemoryConsolidate));

        let projects = self.db.list_projects_with_pending_memory().await?;

        for project in projects {
            let target = WorkflowTarget::new(kind, project.project_id.clone());
            if self.pending_workflow_cursor.contains_key(&target) {
                continue;
            }

            match self.db.try_start_workflow(&target.id, kind).await {
                Ok(Some(_)) => {}
                Ok(None) => continue,
                Err(error) => {
                    self.mark_error(&target, "claim", &error).await;
                    continue;
                }
            }

            let heartbeat_cutoff = match now_rfc3339() {
                Ok(value) => value,
                Err(error) => {
                    self.mark_error(&target, "heartbeat", &error).await;
                    continue;
                }
            };

            self.pending_workflow_cursor.insert(
                target.clone(),
                WorkflowCursor::ReceivedAt {
                    received_at: heartbeat_cutoff.clone(),
                    secondary: None,
                },
            );

            let org_project = crate::event::OrganizationProject {
                project_id: target.id.clone(),
                name: project.name.clone(),
            };
            let input =
                match format_project_memory_consolidate_request(&org_project, &heartbeat_cutoff) {
                    Ok(value) => value,
                    Err(error) => {
                        self.mark_error(&target, "format", &error).await;
                        self.pending_workflow_cursor.remove(&target);
                        continue;
                    }
                };

            if let Err(error) = self
                .dispatch_workflow(WorkflowDispatch {
                    target: target.clone(),
                    input,
                })
                .await
            {
                self.mark_error(&target, "enqueue", &error).await;
                self.pending_workflow_cursor.remove(&target);
            }
        }

        Ok(())
    }

    async fn run_organization_periodic_sweep(&mut self, kind: WorkflowKind) -> Result<()> {
        debug_assert!(matches!(
            kind,
            WorkflowKind::OrganizationMemoryConsolidate | WorkflowKind::OrganizationSkills
        ));

        let organization_ids = self.db.list_organizations_with_projects().await?;

        for organization_id in organization_ids {
            let target = WorkflowTarget::new(kind, organization_id);
            if self.pending_workflow_cursor.contains_key(&target) {
                continue;
            }

            match self.db.try_start_workflow(&target.id, kind).await {
                Ok(Some(_)) => {}
                Ok(None) => continue,
                Err(error) => {
                    self.mark_error(&target, "claim", &error).await;
                    continue;
                }
            };

            let heartbeat_cutoff = match now_rfc3339() {
                Ok(value) => value,
                Err(error) => {
                    self.mark_error(&target, "heartbeat", &error).await;
                    continue;
                }
            };

            self.pending_workflow_cursor.insert(
                target.clone(),
                WorkflowCursor::ReceivedAt {
                    received_at: heartbeat_cutoff.clone(),
                    secondary: None,
                },
            );

            let projects = match self.db.list_projects_for_summary(&target.id).await {
                Ok(value) => value,
                Err(error) => {
                    self.mark_error(&target, "list projects", &error).await;
                    self.pending_workflow_cursor.remove(&target);
                    continue;
                }
            };

            let input = match target.kind {
                WorkflowKind::OrganizationMemoryConsolidate => {
                    format_organization_memory_consolidate_request(&projects, &heartbeat_cutoff)
                }
                WorkflowKind::OrganizationSkills => {
                    format_organization_skills_request(&projects, &heartbeat_cutoff)
                }
                _ => unreachable!(),
            };
            let input = match input {
                Ok(value) => value,
                Err(error) => {
                    self.mark_error(&target, "format", &error).await;
                    self.pending_workflow_cursor.remove(&target);
                    continue;
                }
            };

            if let Err(error) = self
                .dispatch_workflow(WorkflowDispatch {
                    target: target.clone(),
                    input,
                })
                .await
            {
                self.mark_error(&target, "enqueue", &error).await;
                self.pending_workflow_cursor.remove(&target);
            }
        }

        Ok(())
    }

    async fn dispatch_workflow(&mut self, dispatch: WorkflowDispatch) -> Result<()> {
        let target = dispatch.target.clone();
        self.command_tx
            .send(AgentCommand::DispatchWorkflow(dispatch))
            .await
            .with_context(|| format!("failed to dispatch workflow {}", target.label()))
    }

    async fn mark_error(&self, target: &WorkflowTarget, action: &str, error: &anyhow::Error) {
        eprintln!(
            "[workflow-agent] failed to {action} workflow {}: {error:#}",
            target.label()
        );

        let result = match target.kind {
            WorkflowKind::OrganizationSummary => {
                self.db
                    .set_organization_summary_status(&target.id, SummaryStatus::Error)
                    .await
            }
            WorkflowKind::ProjectSummary => {
                self.db
                    .set_project_summary_status(&target.id, SummaryStatus::Error)
                    .await
            }
            kind => {
                self.db
                    .set_workflow_status(&target.id, kind, SummaryStatus::Error)
                    .await
            }
        };

        if let Err(persist_error) = result {
            eprintln!(
                "[workflow-agent] failed to persist error status for {}: {persist_error:#}",
                target.label()
            );
        }
    }
}

fn create_interval(duration: Duration) -> Option<Interval> {
    if duration.is_zero() {
        return None;
    }

    let mut interval = time::interval_at(Instant::now() + duration, duration);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    Some(interval)
}

async fn wait_for_interval(interval: &mut Option<Interval>) {
    match interval {
        Some(interval) => {
            interval.tick().await;
        }
        None => pending::<()>().await,
    }
}

/// Returns the cursor `received_at` to persist for the next sweep, plus a
/// flag indicating whether the batch hit its row limit at that timestamp. The
/// caller uses the flag to decide whether a secondary tiebreaker (seq /
/// session_id) is also required so that rows sharing the same `received_at`
/// don't get dropped on the next iteration.
fn received_at_workflow_cutoff<'a>(
    received_ats: &[&'a str],
    limit: i64,
    heartbeat_cutoff: &'a str,
) -> (&'a str, bool) {
    if received_ats.len() as i64 == limit {
        (
            received_ats.last().copied().unwrap_or(heartbeat_cutoff),
            true,
        )
    } else {
        (heartbeat_cutoff, false)
    }
}

fn transcript_workflow_cursor(
    transcripts: &[crate::event::OrganizationTranscript],
    included_transcript_count: usize,
    transcript_limit: i64,
    heartbeat_cutoff: &str,
) -> WorkflowCursor {
    let included_transcript_count = included_transcript_count.min(transcripts.len());
    let included_transcripts = &transcripts[..included_transcript_count];
    let last_included = included_transcripts
        .last()
        .expect("transcript cursor requires at least one transcript");
    let exhausted_queried_transcripts = included_transcript_count == transcripts.len();
    let hit_limit =
        !exhausted_queried_transcripts || included_transcript_count as i64 == transcript_limit;

    if hit_limit {
        WorkflowCursor::ReceivedAt {
            received_at: last_included.received_at.clone(),
            secondary: Some(WorkflowCursorSecondary::SessionId(
                last_included.session_id.clone(),
            )),
        }
    } else {
        WorkflowCursor::ReceivedAt {
            received_at: heartbeat_cutoff.to_owned(),
            secondary: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_transcript(
        session_id: &str,
        received_at: &str,
    ) -> crate::event::OrganizationTranscript {
        crate::event::OrganizationTranscript {
            session_id: session_id.to_owned(),
            project_id: "PROJECT42".to_owned(),
            project_name: "Operations".to_owned(),
            member_user_id: "user_123".to_owned(),
            member_name: "Dana".to_owned(),
            client: "codex".to_owned(),
            repo_root: "/tmp/repo".to_owned(),
            branch: None,
            received_at: received_at.to_owned(),
            transcript_path: "/tmp/transcript.jsonl".to_owned(),
            transcript_text: "user: ship it\nassistant: done".to_owned(),
        }
    }

    #[test]
    fn empty_received_at_batch_uses_heartbeat_cutoff() {
        assert_eq!(
            received_at_workflow_cutoff(&[], 10, "2026-04-17T12:00:00Z"),
            ("2026-04-17T12:00:00Z", false)
        );
    }

    #[test]
    fn batch_at_limit_uses_last_item_received_at_and_flags_at_limit() {
        assert_eq!(
            received_at_workflow_cutoff(
                &["2026-04-17T11:59:58Z", "2026-04-17T11:59:59Z"],
                2,
                "2026-04-17T12:00:00Z",
            ),
            ("2026-04-17T11:59:59Z", true)
        );
    }

    #[test]
    fn batch_below_limit_uses_heartbeat_cutoff() {
        assert_eq!(
            received_at_workflow_cutoff(&["2026-04-17T11:59:58Z"], 2, "2026-04-17T12:00:00Z",),
            ("2026-04-17T12:00:00Z", false)
        );
    }

    #[test]
    fn transcript_workflow_cursor_uses_last_included_session_when_batch_is_trimmed() {
        let transcripts = vec![
            sample_transcript("sess_1", "2026-04-17T11:59:58Z"),
            sample_transcript("sess_2", "2026-04-17T11:59:59Z"),
        ];

        let cursor = transcript_workflow_cursor(
            &transcripts,
            1,
            PROJECT_TRANSCRIPT_LIMIT,
            "2026-04-17T12:00:00Z",
        );

        assert!(matches!(
            cursor,
            WorkflowCursor::ReceivedAt {
                received_at,
                secondary: Some(WorkflowCursorSecondary::SessionId(session_id)),
            } if received_at == "2026-04-17T11:59:58Z" && session_id == "sess_1"
        ));
    }
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        let mut sigint =
            signal(SignalKind::interrupt()).expect("failed to register SIGINT handler");
        let mut sigterm =
            signal(SignalKind::terminate()).expect("failed to register SIGTERM handler");
        tokio::select! {
            _ = sigint.recv() => {}
            _ = sigterm.recv() => {}
        }
        return;
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
