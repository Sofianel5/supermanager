use std::{collections::HashMap, fs, path::PathBuf};

use anyhow::{Context, Result};
use codex_app_server_client::{InProcessAppServerClient, InProcessServerEvent};
use codex_app_server_protocol::{
    AskForApproval, ClientRequest, DynamicToolCallOutputContentItem, DynamicToolCallParams,
    DynamicToolCallResponse, JSONRPCErrorError, RequestId, SandboxMode, ServerNotification,
    ServerRequest, ThreadResumeParams, ThreadResumeResponse, ThreadStartParams,
    ThreadStartResponse, TurnStartParams, TurnStartResponse, TurnStatus, TurnSteerParams,
    TurnSteerResponse, UserInput,
};
use reporter_protocol::StoredHookEvent;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

use crate::{
    event::{
        OrganizationHeartbeatEvent, OrganizationHeartbeatRoom,
        format_organization_heartbeat_request, format_room_event,
    },
    ipc::{AgentMessage, PendingToolCalls},
    prompt::{ORGANIZATION_SYSTEM_PROMPT, ROOM_SYSTEM_PROMPT},
    tools::{SummaryTool, tool_failure},
};

const SUMMARY_MODEL: &str = "gpt-5.4-mini";

pub(crate) enum AgentCommand {
    EnqueueRoomEvent {
        room_id: String,
        room_name: String,
        event: StoredHookEvent,
    },
    OrganizationHeartbeat {
        organization_id: String,
        events: Vec<OrganizationHeartbeatEvent>,
        rooms: Vec<OrganizationHeartbeatRoom>,
    },
    Shutdown,
}

enum LoopInput {
    Command(Option<AgentCommand>),
    Event(Option<InProcessServerEvent>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SummaryStatus {
    Generating,
    Ready,
    Error,
}

impl SummaryStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Generating => "generating",
            Self::Ready => "ready",
            Self::Error => "error",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum SummaryScope {
    Organization,
    Room,
}

impl SummaryScope {
    fn as_str(self) -> &'static str {
        match self {
            Self::Organization => "organization",
            Self::Room => "room",
        }
    }

    fn directory_name(self) -> &'static str {
        match self {
            Self::Organization => "organizations",
            Self::Room => "rooms",
        }
    }

    fn system_prompt(self) -> &'static str {
        match self {
            Self::Organization => ORGANIZATION_SYSTEM_PROMPT,
            Self::Room => ROOM_SYSTEM_PROMPT,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct SummaryTarget {
    scope: SummaryScope,
    id: String,
}

impl SummaryTarget {
    fn organization(organization_id: impl Into<String>) -> Self {
        Self {
            scope: SummaryScope::Organization,
            id: organization_id.into(),
        }
    }

    fn room(room_id: impl Into<String>) -> Self {
        Self {
            scope: SummaryScope::Room,
            id: room_id.into(),
        }
    }
}

struct SummaryTargetState {
    thread_id: Option<String>,
    active_turn: Option<String>,
}

pub(crate) struct AgentLoop {
    client: InProcessAppServerClient,
    command_rx: mpsc::Receiver<AgentCommand>,
    output_tx: mpsc::Sender<AgentMessage>,
    summary_threads_dir: PathBuf,
    next_request_id: i64,
    next_tool_call_id: u64,
    targets: HashMap<SummaryTarget, SummaryTargetState>,
    thread_to_target: HashMap<String, SummaryTarget>,
    pending_tool_calls: PendingToolCalls,
}

impl AgentLoop {
    pub(crate) fn new(
        client: InProcessAppServerClient,
        command_rx: mpsc::Receiver<AgentCommand>,
        output_tx: mpsc::Sender<AgentMessage>,
        summary_threads_dir: PathBuf,
        pending_tool_calls: PendingToolCalls,
    ) -> Self {
        Self {
            client,
            command_rx,
            output_tx,
            summary_threads_dir,
            next_request_id: 1,
            next_tool_call_id: 1,
            targets: HashMap::new(),
            thread_to_target: HashMap::new(),
            pending_tool_calls,
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
                LoopInput::Command(Some(AgentCommand::EnqueueRoomEvent {
                    room_id,
                    room_name,
                    event,
                })) => {
                    self.send_room_event(&room_id, &room_name, &event).await?;
                }
                LoopInput::Command(Some(AgentCommand::OrganizationHeartbeat {
                    organization_id,
                    events,
                    rooms,
                })) => {
                    self.handle_organization_heartbeat(&organization_id, &rooms, &events)
                        .await?;
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
                eprintln!("[summary-agent] lagged by {skipped} events");
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
                                "unsupported Codex server request during summary generation: {:?}",
                                other
                            ),
                            data: Some(Value::String(
                                "summary workflows only support dynamic tool calls".to_owned(),
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
            ServerNotification::Error(payload) => {
                let label = self
                    .thread_to_target
                    .get(&payload.thread_id)
                    .map(|target| format!("{} {}", target.scope.as_str(), target.id))
                    .unwrap_or_else(|| format!("thread {}", payload.thread_id));
                eprintln!(
                    "[summary-agent] turn error for {label} turn {}: {}",
                    payload.turn_id, payload.error
                );
            }
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
                self.emit_summary_status(&target, status).await;
            }
            _ => {}
        }

        Ok(())
    }

    async fn send_room_event(
        &mut self,
        room_id: &str,
        room_name: &str,
        event: &StoredHookEvent,
    ) -> Result<()> {
        let target = SummaryTarget::room(room_id.to_owned());
        let thread_id = self.ensure_thread(&target).await?;
        let input = vec![UserInput::Text {
            text: format_room_event(room_id, room_name, event)?,
            text_elements: Vec::new(),
        }];

        self.send_input(&target, thread_id, input).await
    }

    async fn handle_organization_heartbeat(
        &mut self,
        organization_id: &str,
        rooms: &[OrganizationHeartbeatRoom],
        events: &[OrganizationHeartbeatEvent],
    ) -> Result<()> {
        let target = SummaryTarget::organization(organization_id.to_owned());
        let thread_id = self.ensure_thread(&target).await?;
        let input = vec![UserInput::Text {
            text: format_organization_heartbeat_request(rooms, events)?,
            text_elements: Vec::new(),
        }];

        self.send_input(&target, thread_id, input).await
    }

    async fn send_input(
        &mut self,
        target: &SummaryTarget,
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
                        thread_id,
                        input,
                        expected_turn_id: turn_id,
                    },
                })
                .await
            {
                Ok(response) => {
                    if let Some(state) = self.targets.get_mut(target) {
                        state.active_turn = Some(response.turn_id);
                    }
                }
                Err(error) => {
                    eprintln!(
                        "[summary-agent] steer failed for {} {}: {error}",
                        target.scope.as_str(),
                        target.id
                    );
                }
            }
        } else {
            self.emit_summary_status(target, SummaryStatus::Generating)
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
                        "[summary-agent] turn start failed for {} {}: {error}",
                        target.scope.as_str(),
                        target.id
                    );
                    self.emit_summary_status(target, SummaryStatus::Error).await;
                }
            }
        }

        Ok(())
    }

    async fn ensure_thread(&mut self, target: &SummaryTarget) -> Result<String> {
        if let Some(thread_id) = self
            .targets
            .get(target)
            .and_then(|state| state.thread_id.clone())
        {
            return Ok(thread_id);
        }

        let cwd = self.target_cwd(target)?;
        let cwd_str = cwd.display().to_string();

        let stored_thread_id = self.read_thread_id(target)?;
        let thread_id = if let Some(thread_id) = stored_thread_id {
            match self.resume_thread(&thread_id, &cwd_str).await {
                Ok(thread_id) => thread_id,
                Err(error) => {
                    eprintln!(
                        "[summary-agent] failed to resume {} thread {} for {}: {error}. Creating new thread.",
                        target.scope.as_str(),
                        thread_id,
                        target.id,
                    );
                    self.create_thread(target.scope, &cwd_str).await?
                }
            }
        } else {
            self.create_thread(target.scope, &cwd_str).await?
        };

        self.write_thread_id(target, &thread_id)?;

        let state = self
            .targets
            .entry(target.clone())
            .or_insert_with(|| SummaryTargetState {
                thread_id: None,
                active_turn: None,
            });
        state.thread_id = Some(thread_id.clone());
        self.thread_to_target
            .insert(thread_id.clone(), target.clone());

        Ok(thread_id)
    }

    fn target_dir(&self, target: &SummaryTarget) -> PathBuf {
        self.summary_threads_dir
            .join(target.scope.directory_name())
            .join(&target.id)
    }

    fn target_cwd(&self, target: &SummaryTarget) -> Result<PathBuf> {
        let dir = self.target_dir(target).join("cwd");
        fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create target dir: {}", dir.display()))?;
        Ok(dir)
    }

    fn thread_id_path(&self, target: &SummaryTarget) -> Result<PathBuf> {
        let dir = self.target_dir(target);
        fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create target state dir: {}", dir.display()))?;
        Ok(dir.join("thread-id"))
    }

    fn read_thread_id(&self, target: &SummaryTarget) -> Result<Option<String>> {
        let path = self.thread_id_path(target)?;
        if !path.is_file() {
            return Ok(None);
        }
        let value = fs::read_to_string(&path)
            .with_context(|| format!("failed to read thread id from {}", path.display()))?;
        let value = value.trim().to_owned();
        if value.is_empty() {
            return Ok(None);
        }
        Ok(Some(value))
    }

    fn write_thread_id(&self, target: &SummaryTarget, thread_id: &str) -> Result<()> {
        let path = self.thread_id_path(target)?;
        fs::write(&path, thread_id)
            .with_context(|| format!("failed to write thread id to {}", path.display()))?;
        Ok(())
    }

    async fn create_thread(&mut self, scope: SummaryScope, cwd: &str) -> Result<String> {
        let request_id = self.next_request_id();
        let dynamic_tools = match scope {
            SummaryScope::Organization => SummaryTool::organization_specs(),
            SummaryScope::Room => SummaryTool::room_specs(),
        };

        let response = self
            .client
            .request_typed::<ThreadStartResponse>(ClientRequest::ThreadStart {
                request_id,
                params: ThreadStartParams {
                    model: Some(SUMMARY_MODEL.to_owned()),
                    cwd: Some(cwd.to_owned()),
                    approval_policy: Some(AskForApproval::OnRequest),
                    sandbox: Some(SandboxMode::ReadOnly),
                    service_name: Some("supermanager".to_owned()),
                    base_instructions: Some(scope.system_prompt().to_owned()),
                    ephemeral: Some(false),
                    dynamic_tools: Some(dynamic_tools),
                    ..Default::default()
                },
            })
            .await
            .context("failed to create Codex summary thread")?;

        Ok(response.thread.id)
    }

    async fn resume_thread(&mut self, thread_id: &str, cwd: &str) -> Result<String> {
        let request_id = self.next_request_id();
        let response = self
            .client
            .request_typed::<ThreadResumeResponse>(ClientRequest::ThreadResume {
                request_id,
                params: ThreadResumeParams {
                    thread_id: thread_id.to_owned(),
                    cwd: Some(cwd.to_owned()),
                    approval_policy: Some(AskForApproval::OnRequest),
                    ..Default::default()
                },
            })
            .await
            .with_context(|| format!("failed to resume Codex summary thread {thread_id}"))?;

        Ok(response.thread.id)
    }

    async fn execute_tool_call(
        &mut self,
        params: &DynamicToolCallParams,
    ) -> Result<DynamicToolCallResponse> {
        let Some(target) = self.thread_to_target.get(&params.thread_id).cloned() else {
            return Ok(tool_failure(format!(
                "unknown summary thread: {}",
                params.thread_id
            )));
        };

        let tool = match target.scope {
            SummaryScope::Organization => SummaryTool::parse_organization(params),
            SummaryScope::Room => SummaryTool::parse_room(params),
        };
        let tool = match tool {
            Ok(tool) => tool,
            Err(error) => return Ok(tool_failure(error.to_string())),
        };

        let (tool_name, arguments) = tool.into_wire();
        let tool_call_id = self.next_tool_call_id();
        let (tx, rx) = oneshot::channel();
        self.pending_tool_calls
            .lock()
            .await
            .insert(tool_call_id.clone(), tx);

        if self
            .output_tx
            .send(AgentMessage::ToolCall {
                id: tool_call_id.clone(),
                scope: target.scope.as_str().to_owned(),
                target_id: target.id.clone(),
                tool: tool_name,
                arguments,
            })
            .await
            .is_err()
        {
            self.pending_tool_calls.lock().await.remove(&tool_call_id);
            return Ok(tool_failure("host is not accepting tool calls"));
        }

        match rx.await {
            Ok(result) => Ok(DynamicToolCallResponse {
                content_items: vec![DynamicToolCallOutputContentItem::InputText {
                    text: result.message,
                }],
                success: result.success,
            }),
            Err(_) => Ok(tool_failure("tool call channel closed unexpectedly")),
        }
    }

    async fn emit_summary_status(&self, target: &SummaryTarget, status: SummaryStatus) {
        let _ = self
            .output_tx
            .send(AgentMessage::SummaryStatus {
                scope: target.scope.as_str().to_owned(),
                target_id: target.id.clone(),
                status: status.as_str().to_owned(),
            })
            .await;
    }

    fn next_request_id(&mut self) -> RequestId {
        let request_id = self.next_request_id;
        self.next_request_id += 1;
        RequestId::Integer(request_id)
    }

    fn next_tool_call_id(&mut self) -> String {
        let tool_call_id = format!("tool_{}", self.next_tool_call_id);
        self.next_tool_call_id += 1;
        tool_call_id
    }
}
