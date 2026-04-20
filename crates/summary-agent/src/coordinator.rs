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
    db::{OrganizationSummaryQueryOptions, RoomSummaryQueryOptions, SummaryDb},
};

const ORGANIZATION_HEARTBEAT_EVENT_LIMIT: i64 = 500;
const ROOM_SUMMARY_EVENT_LIMIT: i64 = 200;
const ROOM_SUMMARY_SWEEP_LIMIT: i64 = 50;

pub(crate) struct SummaryCoordinator {
    db: SummaryDb,
    command_tx: mpsc::Sender<AgentCommand>,
    event_rx: mpsc::Receiver<AgentEvent>,
    organization_summary_refresh_interval: Duration,
    room_summary_poll_interval: Duration,
    pending_organization_heartbeat_cutoff: HashMap<String, String>,
    pending_room_summary_seq: HashMap<String, i64>,
}

impl SummaryCoordinator {
    pub(crate) fn new(
        db: SummaryDb,
        command_tx: mpsc::Sender<AgentCommand>,
        event_rx: mpsc::Receiver<AgentEvent>,
        organization_summary_refresh_interval: Duration,
        room_summary_poll_interval: Duration,
    ) -> Self {
        Self {
            db,
            command_tx,
            event_rx,
            organization_summary_refresh_interval,
            room_summary_poll_interval,
            pending_organization_heartbeat_cutoff: HashMap::new(),
            pending_room_summary_seq: HashMap::new(),
        }
    }

    pub(crate) async fn run(&mut self) -> Result<()> {
        tokio::try_join!(
            self.db.reset_generating_organization_summaries(),
            self.db.reset_generating_room_summaries(),
        )?;

        self.run_heartbeat_sweep().await?;
        self.run_room_sweep().await?;

        let mut organization_interval = create_interval(self.organization_summary_refresh_interval);
        let mut room_interval = create_interval(self.room_summary_poll_interval);
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
                _ = wait_for_interval(&mut room_interval) => {
                    self.run_room_sweep().await?;
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
                SummaryScope::Room => {
                    self.persist_room_status(&target_id, status).await?;
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

    async fn persist_room_status(&mut self, room_id: &str, status: SummaryStatus) -> Result<()> {
        if status == SummaryStatus::Ready {
            if let Some(last_processed_seq) = self.pending_room_summary_seq.get(room_id).copied() {
                self.db
                    .set_room_summary_last_processed_seq(room_id, last_processed_seq)
                    .await?;
            }
        }

        self.db.set_room_summary_status(room_id, status).await?;

        if matches!(status, SummaryStatus::Ready | SummaryStatus::Error) {
            self.pending_room_summary_seq.remove(room_id);
        }

        Ok(())
    }

    async fn run_heartbeat_sweep(&mut self) -> Result<()> {
        let organization_ids = self.db.list_organizations_with_rooms().await?;

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
            SummaryScope::Room => {
                self.db
                    .set_room_summary_status(target_id, SummaryStatus::Error)
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

        let rooms = self.db.list_rooms_for_summary(organization_id).await?;
        self.command_tx
            .send(AgentCommand::OrganizationHeartbeat {
                organization_id: organization_id.to_owned(),
                events,
                rooms,
            })
            .await
            .with_context(|| {
                format!("failed to send organization heartbeat for {organization_id} to agent")
            })?;

        Ok(())
    }

    async fn run_room_sweep(&mut self) -> Result<()> {
        let rooms = self
            .db
            .list_rooms_needing_summary(ROOM_SUMMARY_SWEEP_LIMIT)
            .await?;

        for room in rooms {
            if self.pending_room_summary_seq.contains_key(&room.room_id) {
                continue;
            }

            let claim = match self.db.try_start_room_summary(&room.room_id).await {
                Ok(Some(claim)) => claim,
                Ok(None) => continue,
                Err(error) => {
                    self.mark_error(SummaryScope::Room, &room.room_id, "claim", &error)
                        .await;
                    continue;
                }
            };

            if let Err(error) = self
                .enqueue_room_summary(&room.room_id, &room.name, claim.last_processed_seq)
                .await
            {
                self.mark_error(SummaryScope::Room, &room.room_id, "enqueue summary", &error)
                    .await;
                self.pending_room_summary_seq.remove(&room.room_id);
            }
        }

        Ok(())
    }

    async fn enqueue_room_summary(
        &mut self,
        room_id: &str,
        room_name: &str,
        last_processed_seq: i64,
    ) -> Result<()> {
        let events = self
            .db
            .query_room_events_for_summary(
                room_id,
                RoomSummaryQueryOptions {
                    after_seq: Some(last_processed_seq),
                    limit: Some(ROOM_SUMMARY_EVENT_LIMIT),
                },
            )
            .await?;

        let Some(last_seq) = events.last().map(|event| event.seq) else {
            self.db
                .set_room_summary_status(room_id, SummaryStatus::Ready)
                .await?;
            return Ok(());
        };

        self.pending_room_summary_seq
            .insert(room_id.to_owned(), last_seq);

        for event in events {
            self.command_tx
                .send(AgentCommand::EnqueueRoomEvent {
                    room_id: room_id.to_owned(),
                    room_name: room_name.to_owned(),
                    event,
                })
                .await
                .with_context(|| format!("failed to send room event for {room_id} to agent"))?;
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
            room_id: "ROOM42".to_owned(),
            room_name: "Operations".to_owned(),
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
