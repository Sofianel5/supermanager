use std::{collections::HashMap, future::pending, time::Duration};

use ::time::format_description::well_known::Rfc3339;
use anyhow::{Context, Result};
use reporter_protocol::SummaryStatus;
use sqlx::types::time::OffsetDateTime;
use tokio::{
    sync::mpsc,
    time::{self, Instant, Interval, MissedTickBehavior},
};

use crate::{
    agent::{AgentCommand, AgentEvent, SummaryScope},
    db::{OrganizationSummaryQueryOptions, ProjectSummaryQueryOptions, SummaryDb},
};

const ORGANIZATION_HEARTBEAT_EVENT_LIMIT: i64 = 500;
const PROJECT_SUMMARY_EVENT_LIMIT: i64 = 200;
const PROJECT_SUMMARY_SWEEP_LIMIT: i64 = 50;

pub(crate) struct SummaryCoordinator {
    db: SummaryDb,
    command_tx: mpsc::Sender<AgentCommand>,
    event_rx: mpsc::Receiver<AgentEvent>,
    organization_summary_refresh_interval: Duration,
    project_summary_poll_interval: Duration,
    pending_organization_heartbeat_cutoff: HashMap<String, String>,
    pending_project_summary_seq: HashMap<String, i64>,
}

impl SummaryCoordinator {
    pub(crate) fn new(
        db: SummaryDb,
        command_tx: mpsc::Sender<AgentCommand>,
        event_rx: mpsc::Receiver<AgentEvent>,
        organization_summary_refresh_interval: Duration,
        project_summary_poll_interval: Duration,
    ) -> Self {
        Self {
            db,
            command_tx,
            event_rx,
            organization_summary_refresh_interval,
            project_summary_poll_interval,
            pending_organization_heartbeat_cutoff: HashMap::new(),
            pending_project_summary_seq: HashMap::new(),
        }
    }

    pub(crate) async fn run(&mut self) -> Result<()> {
        tokio::try_join!(
            self.db.reset_generating_organization_summaries(),
            self.db.reset_generating_project_summaries(),
        )?;

        self.run_heartbeat_sweep().await?;
        self.run_project_sweep().await?;

        let mut organization_interval = create_interval(self.organization_summary_refresh_interval);
        let mut project_interval = create_interval(self.project_summary_poll_interval);
        let mut shutdown = Box::pin(shutdown_signal());

        loop {
            tokio::select! {
                maybe_event = self.event_rx.recv() => {
                    let event = maybe_event.context("summary agent loop exited unexpectedly")?;
                    self.handle_agent_event(event).await?;
                }
                _ = wait_for_interval(&mut organization_interval) => {
                    self.run_heartbeat_sweep().await?;
                }
                _ = wait_for_interval(&mut project_interval) => {
                    self.run_project_sweep().await?;
                }
                _ = &mut shutdown => break,
            }
        }

        Ok(())
    }

    async fn handle_agent_event(&mut self, event: AgentEvent) -> Result<()> {
        match event {
            AgentEvent::SummaryStatus {
                scope,
                target_id,
                status,
            } => match scope {
                SummaryScope::Organization => {
                    self.persist_organization_status(&target_id, status).await?;
                }
                SummaryScope::Project => {
                    self.persist_project_status(&target_id, status).await?;
                }
            },
        }

        Ok(())
    }

    async fn persist_organization_status(
        &mut self,
        organization_id: &str,
        status: SummaryStatus,
    ) -> Result<()> {
        if status == SummaryStatus::Ready {
            if let Some(updated_at) = self
                .pending_organization_heartbeat_cutoff
                .get(organization_id)
                .cloned()
            {
                self.db
                    .set_organization_summary_updated_at(organization_id, &updated_at)
                    .await?;
            }
        }

        self.db
            .set_organization_summary_status(organization_id, status)
            .await?;

        if matches!(status, SummaryStatus::Ready | SummaryStatus::Error) {
            self.pending_organization_heartbeat_cutoff
                .remove(organization_id);
        }

        Ok(())
    }

    async fn persist_project_status(&mut self, project_id: &str, status: SummaryStatus) -> Result<()> {
        if status == SummaryStatus::Ready {
            if let Some(last_processed_seq) = self.pending_project_summary_seq.get(project_id).copied() {
                self.db
                    .set_project_summary_last_processed_seq(project_id, last_processed_seq)
                    .await?;
            }
        }

        self.db.set_project_summary_status(project_id, status).await?;

        if matches!(status, SummaryStatus::Ready | SummaryStatus::Error) {
            self.pending_project_summary_seq.remove(project_id);
        }

        Ok(())
    }

    async fn run_heartbeat_sweep(&mut self) -> Result<()> {
        let organization_ids = self.db.list_organizations_with_projects().await?;

        for organization_id in organization_ids {
            if self
                .pending_organization_heartbeat_cutoff
                .contains_key(&organization_id)
            {
                continue;
            }

            let claim = match self
                .db
                .try_start_organization_summary(&organization_id)
                .await
            {
                Ok(Some(claim)) => claim,
                Ok(None) => continue,
                Err(error) => {
                    self.mark_error(
                        SummaryScope::Organization,
                        &organization_id,
                        "claim",
                        &error,
                    )
                    .await;
                    continue;
                }
            };

            if let Err(error) = self
                .enqueue_organization_heartbeat(&organization_id, claim.previous_summary_updated_at)
                .await
            {
                self.mark_error(
                    SummaryScope::Organization,
                    &organization_id,
                    "enqueue heartbeat",
                    &error,
                )
                .await;
                self.pending_organization_heartbeat_cutoff
                    .remove(&organization_id);
            }
        }

        Ok(())
    }

    async fn mark_error(
        &self,
        scope: SummaryScope,
        target_id: &str,
        action: &str,
        error: &anyhow::Error,
    ) {
        let scope_label = scope.as_str();
        eprintln!(
            "[summary-agent] failed to {action} {scope_label} summary for {target_id}: {error:#}"
        );
        let result = match scope {
            SummaryScope::Organization => {
                self.db
                    .set_organization_summary_status(target_id, SummaryStatus::Error)
                    .await
            }
            SummaryScope::Project => {
                self.db
                    .set_project_summary_status(target_id, SummaryStatus::Error)
                    .await
            }
        };
        if let Err(persist_error) = result {
            eprintln!(
                "[summary-agent] failed to persist error status for {scope_label} {target_id}: {persist_error:#}"
            );
        }
    }

    async fn enqueue_organization_heartbeat(
        &mut self,
        organization_id: &str,
        previous_summary_updated_at: Option<String>,
    ) -> Result<()> {
        let heartbeat_cutoff = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .context("failed to format organization heartbeat cutoff")?;
        let events = self
            .db
            .query_organization_events_for_summary(
                organization_id,
                OrganizationSummaryQueryOptions {
                    after_received_at: previous_summary_updated_at,
                    before_received_at: Some(heartbeat_cutoff.clone()),
                    limit: Some(ORGANIZATION_HEARTBEAT_EVENT_LIMIT),
                },
            )
            .await?;

        let summary_updated_at = organization_heartbeat_cutoff(&events, &heartbeat_cutoff);
        self.pending_organization_heartbeat_cutoff
            .insert(organization_id.to_owned(), summary_updated_at.to_owned());

        if events.is_empty() {
            self.persist_organization_status(organization_id, SummaryStatus::Ready)
                .await?;
            return Ok(());
        }

        let projects = self.db.list_projects_for_summary(organization_id).await?;
        self.command_tx
            .send(AgentCommand::OrganizationHeartbeat {
                organization_id: organization_id.to_owned(),
                events,
                projects,
            })
            .await
            .with_context(|| {
                format!("failed to send organization heartbeat for {organization_id} to agent")
            })?;

        Ok(())
    }

    async fn run_project_sweep(&mut self) -> Result<()> {
        let projects = self
            .db
            .list_projects_needing_summary(PROJECT_SUMMARY_SWEEP_LIMIT)
            .await?;

        for project in projects {
            if self.pending_project_summary_seq.contains_key(&project.project_id) {
                continue;
            }

            let claim = match self.db.try_start_project_summary(&project.project_id).await {
                Ok(Some(claim)) => claim,
                Ok(None) => continue,
                Err(error) => {
                    self.mark_error(SummaryScope::Project, &project.project_id, "claim", &error)
                        .await;
                    continue;
                }
            };

            if let Err(error) = self
                .enqueue_project_summary(&project.project_id, &project.name, claim.last_processed_seq)
                .await
            {
                self.mark_error(SummaryScope::Project, &project.project_id, "enqueue summary", &error)
                    .await;
                self.pending_project_summary_seq.remove(&project.project_id);
            }
        }

        Ok(())
    }

    async fn enqueue_project_summary(
        &mut self,
        project_id: &str,
        project_name: &str,
        last_processed_seq: i64,
    ) -> Result<()> {
        let events = self
            .db
            .query_project_events_for_summary(
                project_id,
                ProjectSummaryQueryOptions {
                    after_seq: Some(last_processed_seq),
                    limit: Some(PROJECT_SUMMARY_EVENT_LIMIT),
                },
            )
            .await?;

        let Some(last_seq) = events.last().map(|event| event.seq) else {
            self.db
                .set_project_summary_status(project_id, SummaryStatus::Ready)
                .await?;
            return Ok(());
        };

        self.pending_project_summary_seq
            .insert(project_id.to_owned(), last_seq);

        for event in events {
            self.command_tx
                .send(AgentCommand::EnqueueProjectEvent {
                    project_id: project_id.to_owned(),
                    project_name: project_name.to_owned(),
                    event,
                })
                .await
                .with_context(|| format!("failed to send project event for {project_id} to agent"))?;
        }

        Ok(())
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

fn organization_heartbeat_cutoff<'a>(
    events: &'a [crate::event::OrganizationHeartbeatEvent],
    heartbeat_cutoff: &'a str,
) -> &'a str {
    if events.len() as i64 == ORGANIZATION_HEARTBEAT_EVENT_LIMIT {
        events
            .last()
            .map(|event| event.event.received_at.as_str())
            .unwrap_or(heartbeat_cutoff)
    } else {
        heartbeat_cutoff
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use reporter_protocol::StoredHookEvent;
    use serde_json::json;
    use uuid::Uuid;

    use crate::event::OrganizationHeartbeatEvent;

    #[test]
    fn empty_events_use_heartbeat_cutoff() {
        assert_eq!(
            organization_heartbeat_cutoff(&[], "2026-04-17T12:00:00Z"),
            "2026-04-17T12:00:00Z"
        );
    }

    #[test]
    fn at_limit_uses_last_event_received_at() {
        let events = (0..ORGANIZATION_HEARTBEAT_EVENT_LIMIT)
            .map(|_| heartbeat_event("2026-04-17T11:59:58Z"))
            .collect::<Vec<_>>();

        assert_eq!(
            organization_heartbeat_cutoff(&events, "2026-04-17T12:00:00Z"),
            "2026-04-17T11:59:58Z"
        );
    }

    #[test]
    fn below_limit_uses_heartbeat_cutoff() {
        let events = vec![heartbeat_event("2026-04-17T11:59:58Z")];

        assert_eq!(
            organization_heartbeat_cutoff(&events, "2026-04-17T12:00:00Z"),
            "2026-04-17T12:00:00Z"
        );
    }

    fn heartbeat_event(received_at: &str) -> OrganizationHeartbeatEvent {
        OrganizationHeartbeatEvent {
            project_id: "PROJECT42".to_owned(),
            project_name: "Operations".to_owned(),
            event: StoredHookEvent {
                seq: 1,
                event_id: Uuid::nil(),
                received_at: received_at.to_owned(),
                employee_user_id: "user_123".to_owned(),
                employee_name: "Dana".to_owned(),
                client: "codex".to_owned(),
                repo_root: "/tmp/repo".to_owned(),
                branch: None,
                payload: json!({ "hook_event_name": "Stop" }),
            },
        }
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
