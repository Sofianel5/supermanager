use std::collections::HashMap;

use anyhow::{Context, Result};
use codex_app_server_client::{InProcessAppServerClient, InProcessServerEvent, TypedRequestError};
use codex_app_server_protocol::{
    ClientRequest, DynamicToolCallOutputContentItem, DynamicToolCallParams,
    DynamicToolCallResponse, JSONRPCErrorError, RequestId, ServerNotification, ServerRequest,
    ThreadResumeParams, ThreadResumeResponse, ThreadStartParams, ThreadStartResponse,
    TurnInterruptParams, TurnInterruptResponse, TurnStartParams, TurnStartResponse, TurnStatus,
    TurnSteerParams, TurnSteerResponse, UserInput,
};
use reporter_protocol::SummaryStatus;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::{
    db::SummaryDb,
    tools::{SummaryTool, tool_failure},
    workflow::{WorkflowDispatch, WorkflowKind, WorkflowPaths, WorkflowTarget},
};

const NO_ACTIVE_TURN_TO_STEER_ERROR: &str = "no active turn to steer";
const EXPECTED_ACTIVE_TURN_ID_MISMATCH_PREFIX: &str = "expected active turn id `";

pub(crate) enum AgentCommand {
    DispatchWorkflow(WorkflowDispatch),
    Shutdown,
}

enum LoopInput {
    Command(Option<AgentCommand>),
    Event(Option<InProcessServerEvent>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SteerFailureRecovery {
    RestartTurn,
    InterruptThenRestart,
}

#[derive(Debug)]
pub(crate) enum AgentEvent {
    WorkflowStatus {
        target: WorkflowTarget,
        status: SummaryStatus,
    },
}

struct WorkflowTargetState {
    thread_id: Option<String>,
    active_turn: Option<String>,
}

pub(crate) struct AgentLoop {
    client: InProcessAppServerClient,
    command_rx: mpsc::Receiver<AgentCommand>,
    event_tx: mpsc::Sender<AgentEvent>,
    db: SummaryDb,
    workflow_paths: WorkflowPaths,
    next_request_id: i64,
    targets: HashMap<WorkflowTarget, WorkflowTargetState>,
    thread_to_target: HashMap<String, WorkflowTarget>,
}

impl AgentLoop {
    pub(crate) fn new(
        client: InProcessAppServerClient,
        command_rx: mpsc::Receiver<AgentCommand>,
        event_tx: mpsc::Sender<AgentEvent>,
        db: SummaryDb,
        workflow_paths: WorkflowPaths,
    ) -> Self {
        Self {
            client,
            command_rx,
            event_tx,
            db,
            workflow_paths,
            next_request_id: 1,
            targets: HashMap::new(),
            thread_to_target: HashMap::new(),
        }
    }

    pub(crate) async fn run(mut self) -> Result<()> {
        loop {
            let input = {
                let command_rx = &mut self.command_rx;
                let client = &mut self.client;
                tokio::select! {
                    command = command_rx.recv() => LoopInput::Command(command),
                    event = client.next_event() => LoopInput::Event(event),
                }
            };

            match input {
                LoopInput::Command(Some(AgentCommand::DispatchWorkflow(dispatch))) => {
                    self.dispatch_workflow(dispatch).await?;
                }
                LoopInput::Command(Some(AgentCommand::Shutdown)) => break,
                LoopInput::Command(None) => break,
                LoopInput::Event(Some(event)) => self.handle_event(event).await?,
                LoopInput::Event(None) => break,
            }
        }

        Ok(())
    }

    async fn handle_event(&mut self, event: InProcessServerEvent) -> Result<()> {
        match event {
            InProcessServerEvent::Lagged { skipped } => {
                eprintln!("[workflow-agent] lagged by {skipped} events");
            }
            InProcessServerEvent::ServerRequest(request) => {
                self.handle_server_request(request).await?;
            }
            InProcessServerEvent::ServerNotification(notification) => {
                self.handle_notification(notification).await?;
            }
        }

        Ok(())
    }

    async fn handle_server_request(&mut self, request: ServerRequest) -> Result<()> {
        match request {
            ServerRequest::DynamicToolCall { request_id, params } => {
                let response = self.execute_tool_call(&params).await?;
                let result = serde_json::to_value(response)?;
                self.client
                    .resolve_server_request(request_id, result)
                    .await
                    .context("failed to resolve dynamic tool call")?;
            }
            other => {
                self.client
                    .reject_server_request(
                        other.id().clone(),
                        JSONRPCErrorError {
                            code: -32000,
                            message: format!(
                                "unsupported Codex server request during workflow execution: {:?}",
                                other
                            ),
                            data: Some(Value::String(
                                "workflow execution only supports dynamic tool calls".to_owned(),
                            )),
                        },
                    )
                    .await
                    .context("failed to reject unsupported Codex server request")?;
            }
        }

        Ok(())
    }

    async fn handle_notification(&mut self, notification: ServerNotification) -> Result<()> {
        match notification {
            ServerNotification::TurnCompleted(payload) => {
                let Some(target) = self.thread_to_target.get(&payload.thread_id).cloned() else {
                    return Ok(());
                };
                let Some(state) = self.targets.get_mut(&target) else {
                    return Ok(());
                };
                if state.active_turn.as_deref() != Some(payload.turn.id.as_str()) {
                    return Ok(());
                }

                state.active_turn = None;
                let status = match payload.turn.status {
                    TurnStatus::Completed => SummaryStatus::Ready,
                    _ => SummaryStatus::Error,
                };
                self.emit_workflow_status(&target, status).await;
            }
            _ => {}
        }

        Ok(())
    }

    async fn dispatch_workflow(&mut self, dispatch: WorkflowDispatch) -> Result<()> {
        let thread_id = self.ensure_thread(&dispatch.target).await?;
        let input = vec![UserInput::Text {
            text: dispatch.input,
            text_elements: Vec::new(),
        }];

        self.send_input(&dispatch.target, thread_id, input).await
    }

    async fn send_input(
        &mut self,
        target: &WorkflowTarget,
        thread_id: String,
        input: Vec<UserInput>,
    ) -> Result<()> {
        let active_turn = self
            .targets
            .get(target)
            .and_then(|state| state.active_turn.clone());

        if let Some(turn_id) = active_turn {
            let request_id = self.next_request_id();
            match self
                .client
                .request_typed::<TurnSteerResponse>(ClientRequest::TurnSteer {
                    request_id,
                    params: TurnSteerParams {
                        thread_id: thread_id.clone(),
                        input: input.clone(),
                        expected_turn_id: turn_id.clone(),
                    },
                })
                .await
            {
                Ok(response) => {
                    if let Some(state) = self.targets.get_mut(target) {
                        state.active_turn = Some(response.turn_id);
                    }
                    return Ok(());
                }
                Err(error) => {
                    eprintln!(
                        "[workflow-agent] steer failed for {}: {error}",
                        target.label()
                    );
                    let Some(recovery) = steer_failure_recovery(&error) else {
                        if let Some(state) = self.targets.get_mut(target) {
                            state.active_turn = None;
                        }
                        self.emit_workflow_status(target, SummaryStatus::Error)
                            .await;
                        return Ok(());
                    };
                    if recovery == SteerFailureRecovery::InterruptThenRestart
                        && let Err(error) = self.interrupt_turn(target, &thread_id, &turn_id).await
                    {
                        eprintln!(
                            "[workflow-agent] interrupt failed for {} after steer error: {error}",
                            target.label()
                        );
                        if let Some(state) = self.targets.get_mut(target) {
                            state.active_turn = None;
                        }
                        self.emit_workflow_status(target, SummaryStatus::Error)
                            .await;
                        return Ok(());
                    }
                    if let Some(state) = self.targets.get_mut(target) {
                        state.active_turn = None;
                    }
                }
            }
        }

        self.emit_workflow_status(target, SummaryStatus::Generating)
            .await;
        let request_id = self.next_request_id();
        match self
            .client
            .request_typed::<TurnStartResponse>(ClientRequest::TurnStart {
                request_id,
                params: TurnStartParams {
                    thread_id,
                    input,
                    ..Default::default()
                },
            })
            .await
        {
            Ok(response) => {
                if let Some(state) = self.targets.get_mut(target) {
                    state.active_turn = Some(response.turn.id);
                }
            }
            Err(error) => {
                eprintln!(
                    "[workflow-agent] turn start failed for {}: {error}",
                    target.label()
                );
                self.emit_workflow_status(target, SummaryStatus::Error)
                    .await;
            }
        }

        Ok(())
    }

    async fn interrupt_turn(
        &mut self,
        target: &WorkflowTarget,
        thread_id: &str,
        turn_id: &str,
    ) -> Result<()> {
        let request_id = self.next_request_id();
        self.client
            .request_typed::<TurnInterruptResponse>(ClientRequest::TurnInterrupt {
                request_id,
                params: TurnInterruptParams {
                    thread_id: thread_id.to_owned(),
                    turn_id: turn_id.to_owned(),
                },
            })
            .await
            .with_context(|| format!("failed to interrupt active turn for {}", target.label()))?;

        Ok(())
    }

    async fn ensure_thread(&mut self, target: &WorkflowTarget) -> Result<String> {
        if let Some(thread_id) = self
            .targets
            .get(target)
            .and_then(|state| state.thread_id.clone())
        {
            return Ok(thread_id);
        }

        let target_dir = self.workflow_paths.thread_state_dir(target);
        tokio::fs::create_dir_all(&target_dir)
            .await
            .with_context(|| format!("failed to create target dir: {}", target_dir.display()))?;
        let cwd = self.workflow_paths.prepare_cwd(target).await?;
        let cwd_str = cwd.display().to_string();
        let thread_id_path = target_dir.join("thread-id");

        let stored_thread_id = read_thread_id(&thread_id_path).await?;
        let thread_id = if let Some(thread_id) = stored_thread_id {
            match self.resume_thread(&thread_id, &cwd_str, target.kind).await {
                Ok(thread_id) => thread_id,
                Err(error) => {
                    eprintln!(
                        "[workflow-agent] failed to resume workflow thread {} for {}: {error}. Creating new thread.",
                        thread_id,
                        target.label(),
                    );
                    self.create_thread(target.kind, &cwd_str).await?
                }
            }
        } else {
            self.create_thread(target.kind, &cwd_str).await?
        };

        tokio::fs::write(&thread_id_path, &thread_id)
            .await
            .with_context(|| {
                format!("failed to write thread id to {}", thread_id_path.display())
            })?;

        let state = self
            .targets
            .entry(target.clone())
            .or_insert_with(|| WorkflowTargetState {
                thread_id: None,
                active_turn: None,
            });
        state.thread_id = Some(thread_id.clone());
        self.thread_to_target
            .insert(thread_id.clone(), target.clone());

        Ok(thread_id)
    }

    async fn create_thread(&mut self, kind: WorkflowKind, cwd: &str) -> Result<String> {
        let request_id = self.next_request_id();
        let response = self
            .client
            .request_typed::<ThreadStartResponse>(ClientRequest::ThreadStart {
                request_id,
                params: ThreadStartParams {
                    model: Some(kind.model().to_owned()),
                    cwd: Some(cwd.to_owned()),
                    approval_policy: Some(kind.approval_policy()),
                    sandbox: Some(kind.sandbox()),
                    service_name: Some("supermanager".to_owned()),
                    base_instructions: Some(kind.system_prompt().to_owned()),
                    ephemeral: Some(false),
                    dynamic_tools: kind.dynamic_tools(),
                    ..Default::default()
                },
            })
            .await
            .context("failed to create Codex workflow thread")?;

        Ok(response.thread.id)
    }

    async fn resume_thread(
        &mut self,
        thread_id: &str,
        cwd: &str,
        kind: WorkflowKind,
    ) -> Result<String> {
        let request_id = self.next_request_id();
        let response = self
            .client
            .request_typed::<ThreadResumeResponse>(ClientRequest::ThreadResume {
                request_id,
                params: ThreadResumeParams {
                    thread_id: thread_id.to_owned(),
                    cwd: Some(cwd.to_owned()),
                    approval_policy: Some(kind.approval_policy()),
                    ..Default::default()
                },
            })
            .await
            .with_context(|| format!("failed to resume Codex workflow thread {thread_id}"))?;

        Ok(response.thread.id)
    }

    async fn execute_tool_call(
        &mut self,
        params: &DynamicToolCallParams,
    ) -> Result<DynamicToolCallResponse> {
        let Some(target) = self.thread_to_target.get(&params.thread_id).cloned() else {
            return Ok(tool_failure(format!(
                "unknown workflow thread: {}",
                params.thread_id
            )));
        };

        let tool = match target.kind {
            WorkflowKind::OrganizationSummary => SummaryTool::parse_organization(params),
            WorkflowKind::ProjectSummary => SummaryTool::parse_project(params),
            WorkflowKind::ProjectMemoryExtract => SummaryTool::parse_project_memory_extract(params),
            WorkflowKind::ProjectMemoryConsolidate => {
                SummaryTool::parse_project_memory_consolidate(params)
            }
            WorkflowKind::ProjectSkills | WorkflowKind::OrganizationSkills => {
                SummaryTool::parse_skills(params)
            }
            WorkflowKind::OrganizationMemoryConsolidate => {
                SummaryTool::parse_organization_memory_consolidate(params)
            }
            WorkflowKind::ProjectUpdatesEmit => SummaryTool::parse_project_updates_emit(params),
            WorkflowKind::OrganizationUpdatesEmit => {
                SummaryTool::parse_organization_updates_emit(params)
            }
        };
        let tool = match tool {
            Ok(tool) => tool,
            Err(error) => return Ok(tool_failure(error.to_string())),
        };

        let result = match target.kind {
            WorkflowKind::OrganizationSummary => {
                self.db
                    .execute_organization_tool_call(&target.id, tool)
                    .await
            }
            WorkflowKind::ProjectSummary => {
                self.db.execute_project_tool_call(&target.id, tool).await
            }
            kind => {
                self.db
                    .execute_workflow_tool_call(&target.id, kind, tool)
                    .await
            }
        };

        let result = match result {
            Ok(result) => result,
            Err(error) => return Ok(tool_failure(error.to_string())),
        };

        Ok(DynamicToolCallResponse {
            content_items: vec![DynamicToolCallOutputContentItem::InputText {
                text: result.message,
            }],
            success: result.success,
        })
    }

    async fn emit_workflow_status(&self, target: &WorkflowTarget, status: SummaryStatus) {
        if let Err(error) = self
            .event_tx
            .send(AgentEvent::WorkflowStatus {
                target: target.clone(),
                status,
            })
            .await
        {
            eprintln!(
                "[workflow-agent] failed to deliver status {status:?} for {}: {error}",
                target.label(),
            );
        }
    }

    fn next_request_id(&mut self) -> RequestId {
        let request_id = self.next_request_id;
        self.next_request_id += 1;
        RequestId::Integer(request_id)
    }
}

fn steer_failure_recovery(error: &TypedRequestError) -> Option<SteerFailureRecovery> {
    match error {
        TypedRequestError::Server { source, .. }
            if source.message == NO_ACTIVE_TURN_TO_STEER_ERROR =>
        {
            Some(SteerFailureRecovery::RestartTurn)
        }
        TypedRequestError::Server { source, .. }
            if source
                .message
                .starts_with(EXPECTED_ACTIVE_TURN_ID_MISMATCH_PREFIX) =>
        {
            Some(SteerFailureRecovery::InterruptThenRestart)
        }
        _ => None,
    }
}

async fn read_thread_id(path: &std::path::Path) -> Result<Option<String>> {
    match tokio::fs::read_to_string(path).await {
        Ok(value) => {
            let trimmed = value.trim().to_owned();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed))
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => {
            Err(error).with_context(|| format!("failed to read thread id from {}", path.display()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server_error(message: &str) -> TypedRequestError {
        TypedRequestError::Server {
            method: "turn/steer".to_owned(),
            source: JSONRPCErrorError {
                code: -32000,
                data: None,
                message: message.to_owned(),
            },
        }
    }

    #[test]
    fn no_active_turn_errors_restart_the_turn() {
        assert_eq!(
            steer_failure_recovery(&server_error(NO_ACTIVE_TURN_TO_STEER_ERROR)),
            Some(SteerFailureRecovery::RestartTurn),
        );
    }

    #[test]
    fn expected_turn_mismatch_interrupts_before_restart() {
        assert_eq!(
            steer_failure_recovery(&server_error(
                "expected active turn id `expected` but found `actual`",
            )),
            Some(SteerFailureRecovery::InterruptThenRestart),
        );
    }

    #[test]
    fn unrelated_steer_errors_fail_the_workflow() {
        assert_eq!(
            steer_failure_recovery(&server_error("cannot steer a compact turn")),
            None,
        );
    }
}
