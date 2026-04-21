use std::{collections::HashMap, future::pending, time::Duration};

use anyhow::{Context, Result, ensure};
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
        format_project_event, format_project_memory_consolidate_request,
        format_project_memory_extract_request, render_organization_summary_request,
        render_project_skills_request,
    },
    workflow::{
        WorkflowCursor, WorkflowCursorSecondary, WorkflowDecisionRequirement, WorkflowDispatch,
        WorkflowKind, WorkflowTarget,
    },
};

const ORGANIZATION_SUMMARY_EVENT_LIMIT: i64 = 500;
const PROJECT_TRANSCRIPT_LIMIT: i64 = 24;
const PROJECT_MEMORY_EXTRACT_TRANSCRIPT_LIMIT: i64 = 1;
const PROJECT_SUMMARY_EVENT_LIMIT: i64 = 200;
const PROJECT_SUMMARY_SWEEP_LIMIT: i64 = 50;
const STALE_PENDING_WORKFLOW_TIMEOUT: Duration = Duration::from_secs(15 * 60);

#[derive(Clone)]
struct PendingWorkflowState {
    cursor: WorkflowCursor,
    enqueued_at: Instant,
}

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
    pending_workflow_cursor: HashMap<WorkflowTarget, PendingWorkflowState>,
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
            && let Some(cursor) = self
                .pending_workflow_cursor
                .get(target)
                .map(|pending| pending.cursor.clone())
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
        self.expire_stale_pending_workflows().await;
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
                    after_received_at: previous_summary_updated_at.clone(),
                    after_seq: previous_last_processed_seq,
                    before_received_at: Some(heartbeat_cutoff.clone()),
                    limit: Some(ORGANIZATION_SUMMARY_EVENT_LIMIT),
                },
            )
            .await?;

        if events.is_empty() {
            self.persist_workflow_status(target, SummaryStatus::Ready)
                .await?;
            return Ok(());
        }

        let projects = self.db.list_projects_for_summary(&target.id).await?;
        let provisional_source_window_key = build_organization_summary_source_window_key(
            previous_summary_updated_at.as_deref(),
            previous_last_processed_seq,
            &heartbeat_cutoff,
        );
        let render = render_organization_summary_request(
            &projects,
            &events,
            &provisional_source_window_key,
        )?;
        ensure!(
            render.included_event_count > 0,
            "organization summary render omitted every event"
        );

        let cursor = organization_event_workflow_cursor(
            &events,
            render.included_event_count,
            ORGANIZATION_SUMMARY_EVENT_LIMIT,
            &heartbeat_cutoff,
        );
        let (summary_updated_at, last_processed_seq) = match &cursor {
            WorkflowCursor::ReceivedAt {
                received_at,
                secondary,
            } => (
                received_at.clone(),
                match secondary {
                    Some(WorkflowCursorSecondary::Seq(seq)) => Some(*seq),
                    Some(WorkflowCursorSecondary::SessionId(_)) | None => None,
                },
            ),
            WorkflowCursor::Seq(_) => unreachable!("organization summary uses received_at cursor"),
        };
        self.record_pending_workflow(target.clone(), cursor);

        let source_window_key = build_organization_summary_source_window_key(
            previous_summary_updated_at.as_deref(),
            previous_last_processed_seq,
            &summary_updated_at,
        );
        let final_render = render_organization_summary_request(
            &projects,
            &events[..render.included_event_count],
            &source_window_key,
        )?;
        debug_assert_eq!(
            last_processed_seq.is_some(),
            render.included_event_count < events.len()
                || events.len() as i64 == ORGANIZATION_SUMMARY_EVENT_LIMIT,
        );

        self.dispatch_workflow(WorkflowDispatch {
            target: target.clone(),
            input: final_render.input,
            required_decision: Some(WorkflowDecisionRequirement::OrganizationWindow {
                source_window_key,
            }),
        })
        .await
    }

    async fn run_project_summary_sweep(&mut self) -> Result<()> {
        self.expire_stale_pending_workflows().await;
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

        self.record_pending_workflow(target.clone(), WorkflowCursor::Seq(last_seq));

        for event in events {
            self.dispatch_workflow(WorkflowDispatch {
                target: target.clone(),
                input: format_project_event(&target.id, project_name, &event)?,
                required_decision: Some(WorkflowDecisionRequirement::ProjectEvent {
                    source_event_id: event.event_id.to_string(),
                }),
            })
            .await?;
        }

        Ok(())
    }

    async fn run_project_transcript_sweep(&mut self, kind: WorkflowKind) -> Result<()> {
        self.expire_stale_pending_workflows().await;
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
                self.record_pending_workflow(
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
                        required_decision: None,
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
                self.record_pending_workflow(
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
                    required_decision: None,
                })
                .await
            }
            _ => unreachable!(),
        }
    }

    async fn run_project_periodic_sweep(&mut self, kind: WorkflowKind) -> Result<()> {
        self.expire_stale_pending_workflows().await;
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

            self.record_pending_workflow(
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
                    required_decision: None,
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
        self.expire_stale_pending_workflows().await;
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

            self.record_pending_workflow(
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
                    required_decision: None,
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

    fn record_pending_workflow(&mut self, target: WorkflowTarget, cursor: WorkflowCursor) {
        self.pending_workflow_cursor.insert(
            target,
            PendingWorkflowState {
                cursor,
                enqueued_at: Instant::now(),
            },
        );
    }

    async fn expire_stale_pending_workflows(&mut self) {
        let stale_targets = stale_pending_targets(
            &self.pending_workflow_cursor,
            Instant::now(),
            STALE_PENDING_WORKFLOW_TIMEOUT,
        );

        for target in stale_targets {
            let Some(pending) = self.pending_workflow_cursor.remove(&target) else {
                continue;
            };

            eprintln!(
                "[workflow-agent] stale pending workflow {} exceeded {:?} without completion. Resetting target.",
                target.label(),
                pending.enqueued_at.elapsed(),
            );

            if let Err(error) = self
                .command_tx
                .send(AgentCommand::ResetWorkflowTarget(target.clone()))
                .await
            {
                eprintln!(
                    "[workflow-agent] failed to reset stale workflow {}: {error:#}",
                    target.label()
                );
            }

            let error = anyhow::anyhow!(
                "workflow exceeded stale pending timeout of {:?}",
                STALE_PENDING_WORKFLOW_TIMEOUT
            );
            self.mark_error(&target, "reset stale pending workflow", &error)
                .await;
        }
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

fn stale_pending_targets(
    pending_workflows: &HashMap<WorkflowTarget, PendingWorkflowState>,
    now: Instant,
    stale_after: Duration,
) -> Vec<WorkflowTarget> {
    pending_workflows
        .iter()
        .filter_map(|(target, pending)| {
            if now.duration_since(pending.enqueued_at) >= stale_after {
                Some(target.clone())
            } else {
                None
            }
        })
        .collect()
}

async fn wait_for_interval(interval: &mut Option<Interval>) {
    match interval {
        Some(interval) => {
            interval.tick().await;
        }
        None => pending::<()>().await,
    }
}

fn organization_event_workflow_cursor(
    events: &[crate::event::OrganizationEvent],
    included_event_count: usize,
    event_limit: i64,
    heartbeat_cutoff: &str,
) -> WorkflowCursor {
    let included_event_count = included_event_count.min(events.len());
    let included_events = &events[..included_event_count];
    let last_included = included_events
        .last()
        .expect("organization event cursor requires at least one event");
    let exhausted_queried_events = included_event_count == events.len();
    let hit_limit = !exhausted_queried_events || included_event_count as i64 == event_limit;

    if hit_limit {
        WorkflowCursor::ReceivedAt {
            received_at: last_included.event.received_at.clone(),
            secondary: Some(WorkflowCursorSecondary::Seq(last_included.event.seq)),
        }
    } else {
        WorkflowCursor::ReceivedAt {
            received_at: heartbeat_cutoff.to_owned(),
            secondary: None,
        }
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

    fn sample_org_event(
        event_id: &str,
        seq: i64,
        received_at: &str,
    ) -> crate::event::OrganizationEvent {
        crate::event::OrganizationEvent {
            project_id: "PROJECT42".to_owned(),
            project_name: "Operations".to_owned(),
            event: reporter_protocol::StoredHookEvent {
                seq,
                event_id: uuid::Uuid::parse_str(event_id).unwrap(),
                received_at: received_at.to_owned(),
                member_user_id: "user_123".to_owned(),
                member_name: "Dana".to_owned(),
                client: "codex".to_owned(),
                repo_root: "/tmp/repo".to_owned(),
                branch: None,
                payload: serde_json::json!({ "hook_event_name": "Stop" }),
            },
        }
    }

    #[test]
    fn organization_event_workflow_cursor_uses_last_included_seq_when_batch_is_trimmed() {
        let events = vec![
            sample_org_event(
                "00000000-0000-0000-0000-000000000001",
                11,
                "2026-04-17T11:59:58Z",
            ),
            sample_org_event(
                "00000000-0000-0000-0000-000000000002",
                12,
                "2026-04-17T11:59:59Z",
            ),
        ];

        let cursor = organization_event_workflow_cursor(
            &events,
            1,
            ORGANIZATION_SUMMARY_EVENT_LIMIT,
            "2026-04-17T12:00:00Z",
        );

        assert!(matches!(
            cursor,
            WorkflowCursor::ReceivedAt {
                received_at,
                secondary: Some(WorkflowCursorSecondary::Seq(seq)),
            } if received_at == "2026-04-17T11:59:58Z" && seq == 11
        ));
    }

    #[test]
    fn organization_event_workflow_cursor_uses_heartbeat_cutoff_when_everything_fits() {
        let events = vec![sample_org_event(
            "00000000-0000-0000-0000-000000000001",
            11,
            "2026-04-17T11:59:58Z",
        )];

        let cursor = organization_event_workflow_cursor(
            &events,
            events.len(),
            ORGANIZATION_SUMMARY_EVENT_LIMIT,
            "2026-04-17T12:00:00Z",
        );

        assert!(matches!(
            cursor,
            WorkflowCursor::ReceivedAt {
                received_at,
                secondary: None,
            } if received_at == "2026-04-17T12:00:00Z"
        ));
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

    #[test]
    fn stale_pending_targets_only_returns_expired_workflows() {
        let now = Instant::now();
        let stale_target = WorkflowTarget::new(WorkflowKind::OrganizationSummary, "ORG42");
        let fresh_target = WorkflowTarget::new(WorkflowKind::ProjectSummary, "PROJECT42");
        let pending = HashMap::from([
            (
                stale_target.clone(),
                PendingWorkflowState {
                    cursor: WorkflowCursor::ReceivedAt {
                        received_at: "2026-04-17T12:00:00Z".to_owned(),
                        secondary: None,
                    },
                    enqueued_at: now - STALE_PENDING_WORKFLOW_TIMEOUT - Duration::from_secs(1),
                },
            ),
            (
                fresh_target,
                PendingWorkflowState {
                    cursor: WorkflowCursor::Seq(123),
                    enqueued_at: now - Duration::from_secs(30),
                },
            ),
        ]);

        assert_eq!(
            stale_pending_targets(&pending, now, STALE_PENDING_WORKFLOW_TIMEOUT),
            vec![stale_target]
        );
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
