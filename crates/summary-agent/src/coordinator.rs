use std::{collections::HashMap, future::pending, time::Duration};

use anyhow::{Context, Result};
use reporter_protocol::SummaryStatus;
use tokio::{
    sync::mpsc,
    time::{self, Instant, Interval, MissedTickBehavior},
};

use crate::{
    agent::{AgentCommand, AgentEvent},
    db::{OrganizationSummaryQueryOptions, ProjectSummaryQueryOptions, SummaryDb, now_rfc3339},
    event::{
        OrganizationTranscriptRequest, format_organization_memory_request,
        format_organization_skills_request, format_organization_summary_request,
        format_project_event,
    },
    workflow::{WorkflowCursor, WorkflowDispatch, WorkflowKind, WorkflowTarget},
};

const ORGANIZATION_SUMMARY_EVENT_LIMIT: i64 = 500;
const ORGANIZATION_TRANSCRIPT_LIMIT: i64 = 24;
const PROJECT_SUMMARY_EVENT_LIMIT: i64 = 200;
const PROJECT_SUMMARY_SWEEP_LIMIT: i64 = 50;

pub(crate) struct WorkflowCoordinator {
    db: SummaryDb,
    command_tx: mpsc::Sender<AgentCommand>,
    event_rx: mpsc::Receiver<AgentEvent>,
    organization_summary_refresh_interval: Duration,
    project_summary_poll_interval: Duration,
    organization_memory_refresh_interval: Duration,
    organization_skills_refresh_interval: Duration,
    pending_workflow_cursor: HashMap<WorkflowTarget, WorkflowCursor>,
}

impl WorkflowCoordinator {
    pub(crate) fn new(
        db: SummaryDb,
        command_tx: mpsc::Sender<AgentCommand>,
        event_rx: mpsc::Receiver<AgentEvent>,
        organization_summary_refresh_interval: Duration,
        project_summary_poll_interval: Duration,
        organization_memory_refresh_interval: Duration,
        organization_skills_refresh_interval: Duration,
    ) -> Self {
        Self {
            db,
            command_tx,
            event_rx,
            organization_summary_refresh_interval,
            project_summary_poll_interval,
            organization_memory_refresh_interval,
            organization_skills_refresh_interval,
            pending_workflow_cursor: HashMap::new(),
        }
    }

    pub(crate) async fn run(&mut self) -> Result<()> {
        tokio::try_join!(
            self.db.reset_generating_organization_summaries(),
            self.db.reset_generating_project_summaries(),
            self.db.reset_generating_organization_workflows(),
        )?;

        self.run_organization_summary_sweep().await?;
        self.run_project_summary_sweep().await?;
        self.run_organization_transcript_sweep(WorkflowKind::OrganizationMemories)
            .await?;
        self.run_organization_transcript_sweep(WorkflowKind::OrganizationSkills)
            .await?;

        let mut organization_summary_interval =
            create_interval(self.organization_summary_refresh_interval);
        let mut project_summary_interval = create_interval(self.project_summary_poll_interval);
        let mut organization_memory_interval =
            create_interval(self.organization_memory_refresh_interval);
        let mut organization_skills_interval =
            create_interval(self.organization_skills_refresh_interval);
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
                _ = wait_for_interval(&mut organization_memory_interval) => {
                    self.run_organization_transcript_sweep(WorkflowKind::OrganizationMemories).await?;
                }
                _ = wait_for_interval(&mut organization_skills_interval) => {
                    self.run_organization_transcript_sweep(WorkflowKind::OrganizationSkills).await?;
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
                (WorkflowKind::OrganizationSummary, WorkflowCursor::ReceivedAt(updated_at)) => {
                    self.db
                        .set_organization_summary_updated_at(&target.id, &updated_at)
                        .await?;
                }
                (
                    WorkflowKind::OrganizationMemories | WorkflowKind::OrganizationSkills,
                    WorkflowCursor::ReceivedAt(updated_at),
                ) => {
                    self.db
                        .set_organization_workflow_updated_at(&target.id, target.kind, &updated_at)
                        .await?;
                }
                (WorkflowKind::ProjectSummary, WorkflowCursor::Seq(last_processed_seq)) => {
                    self.db
                        .set_project_summary_last_processed_seq(&target.id, last_processed_seq)
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
            WorkflowKind::OrganizationMemories | WorkflowKind::OrganizationSkills => {
                self.db
                    .set_organization_workflow_status(&target.id, target.kind, status)
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
                .enqueue_organization_summary(&target, claim.previous_summary_updated_at)
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
    ) -> Result<()> {
        let heartbeat_cutoff = now_rfc3339()?;
        let events = self
            .db
            .query_organization_events_for_summary(
                &target.id,
                OrganizationSummaryQueryOptions {
                    after_received_at: previous_summary_updated_at,
                    before_received_at: Some(heartbeat_cutoff.clone()),
                    limit: Some(ORGANIZATION_SUMMARY_EVENT_LIMIT),
                },
            )
            .await?;

        let summary_updated_at = received_at_workflow_cutoff(
            &events
                .iter()
                .map(|event| event.event.received_at.as_str())
                .collect::<Vec<_>>(),
            ORGANIZATION_SUMMARY_EVENT_LIMIT,
            &heartbeat_cutoff,
        );
        self.pending_workflow_cursor.insert(
            target.clone(),
            WorkflowCursor::ReceivedAt(summary_updated_at.to_owned()),
        );

        if events.is_empty() {
            self.persist_workflow_status(target, SummaryStatus::Ready)
                .await?;
            return Ok(());
        }

        let projects = self.db.list_projects_for_summary(&target.id).await?;
        self.dispatch_workflow(WorkflowDispatch {
            target: target.clone(),
            input: format_organization_summary_request(&projects, &events)?,
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

    async fn run_organization_transcript_sweep(&mut self, kind: WorkflowKind) -> Result<()> {
        debug_assert!(matches!(
            kind,
            WorkflowKind::OrganizationMemories | WorkflowKind::OrganizationSkills
        ));

        let organization_ids = self.db.list_organizations_with_transcripts().await?;

        for organization_id in organization_ids {
            let target = WorkflowTarget::new(kind, organization_id);
            if self.pending_workflow_cursor.contains_key(&target) {
                continue;
            }

            let claim = match self
                .db
                .try_start_organization_workflow(&target.id, kind)
                .await
            {
                Ok(Some(claim)) => claim,
                Ok(None) => continue,
                Err(error) => {
                    self.mark_error(&target, "claim", &error).await;
                    continue;
                }
            };

            if let Err(error) = self
                .enqueue_organization_transcript_workflow(
                    &target,
                    claim.previous_processed_received_at,
                )
                .await
            {
                self.mark_error(&target, "enqueue", &error).await;
                self.pending_workflow_cursor.remove(&target);
            }
        }

        Ok(())
    }

    async fn enqueue_organization_transcript_workflow(
        &mut self,
        target: &WorkflowTarget,
        previous_processed_received_at: Option<String>,
    ) -> Result<()> {
        let heartbeat_cutoff = now_rfc3339()?;
        let transcripts = self
            .db
            .query_organization_transcripts_for_workflow(
                &target.id,
                OrganizationSummaryQueryOptions {
                    after_received_at: previous_processed_received_at.clone(),
                    before_received_at: Some(heartbeat_cutoff.clone()),
                    limit: Some(ORGANIZATION_TRANSCRIPT_LIMIT),
                },
            )
            .await?;

        let updated_at = received_at_workflow_cutoff(
            &transcripts
                .iter()
                .map(|transcript| transcript.received_at.as_str())
                .collect::<Vec<_>>(),
            ORGANIZATION_TRANSCRIPT_LIMIT,
            &heartbeat_cutoff,
        );
        self.pending_workflow_cursor.insert(
            target.clone(),
            WorkflowCursor::ReceivedAt(updated_at.to_owned()),
        );

        if transcripts.is_empty() {
            self.persist_workflow_status(target, SummaryStatus::Ready)
                .await?;
            return Ok(());
        }

        let projects = self.db.list_projects_for_summary(&target.id).await?;
        let request = OrganizationTranscriptRequest {
            projects: &projects,
            transcripts: &transcripts,
            previous_processed_received_at: previous_processed_received_at.as_deref(),
            heartbeat_cutoff: &heartbeat_cutoff,
        };
        let input = match target.kind {
            WorkflowKind::OrganizationMemories => format_organization_memory_request(request)?,
            WorkflowKind::OrganizationSkills => format_organization_skills_request(request)?,
            _ => unreachable!(),
        };

        self.dispatch_workflow(WorkflowDispatch {
            target: target.clone(),
            input,
        })
        .await
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
            "[summary-agent] failed to {action} workflow {}: {error:#}",
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
            WorkflowKind::OrganizationMemories | WorkflowKind::OrganizationSkills => {
                self.db
                    .set_organization_workflow_status(&target.id, target.kind, SummaryStatus::Error)
                    .await
            }
        };

        if let Err(persist_error) = result {
            eprintln!(
                "[summary-agent] failed to persist error status for {}: {persist_error:#}",
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

fn received_at_workflow_cutoff<'a>(
    received_ats: &[&'a str],
    limit: i64,
    heartbeat_cutoff: &'a str,
) -> &'a str {
    if received_ats.len() as i64 == limit {
        received_ats.last().copied().unwrap_or(heartbeat_cutoff)
    } else {
        heartbeat_cutoff
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_received_at_batch_uses_heartbeat_cutoff() {
        assert_eq!(
            received_at_workflow_cutoff(&[], 10, "2026-04-17T12:00:00Z"),
            "2026-04-17T12:00:00Z"
        );
    }

    #[test]
    fn batch_at_limit_uses_last_item_received_at() {
        assert_eq!(
            received_at_workflow_cutoff(
                &["2026-04-17T11:59:58Z", "2026-04-17T11:59:59Z"],
                2,
                "2026-04-17T12:00:00Z",
            ),
            "2026-04-17T11:59:59Z"
        );
    }

    #[test]
    fn batch_below_limit_uses_heartbeat_cutoff() {
        assert_eq!(
            received_at_workflow_cutoff(&["2026-04-17T11:59:58Z"], 2, "2026-04-17T12:00:00Z",),
            "2026-04-17T12:00:00Z"
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
