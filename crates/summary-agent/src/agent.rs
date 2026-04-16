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
    event::format_event,
    ipc::{AgentMessage, PendingToolCalls},
    prompt::SYSTEM_PROMPT,
    tools::{SummaryTool, tool_failure},
};

const SUMMARY_MODEL: &str = "gpt-5.4-mini";

pub(crate) enum AgentCommand {
    EnqueueEvent {
        room_id: String,
        event: StoredHookEvent,
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

pub(crate) struct AgentLoop {
    client: InProcessAppServerClient,
    command_rx: mpsc::Receiver<AgentCommand>,
    output_tx: mpsc::Sender<AgentMessage>,
    rooms_dir: PathBuf,
    next_request_id: i64,
    next_tool_call_id: u64,
    rooms: HashMap<String, RoomState>,
    thread_to_room: HashMap<String, String>,
    pending_tool_calls: PendingToolCalls,
}

struct RoomState {
    thread_id: Option<String>,
    active_turn: Option<String>,
}

impl AgentLoop {
    pub(crate) fn new(
        client: InProcessAppServerClient,
        command_rx: mpsc::Receiver<AgentCommand>,
        output_tx: mpsc::Sender<AgentMessage>,
        rooms_dir: PathBuf,
        pending_tool_calls: PendingToolCalls,
    ) -> Self {
        Self {
            client,
            command_rx,
            output_tx,
            rooms_dir,
            next_request_id: 1,
            next_tool_call_id: 1,
            rooms: HashMap::new(),
            thread_to_room: HashMap::new(),
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
                LoopInput::Command(Some(AgentCommand::EnqueueEvent { room_id, event })) => {
                    self.send_event(&room_id, &event).await?;
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
                                "unsupported Codex server request during room summarization: {:?}",
                                other
                            ),
                            data: Some(Value::String(
                                "room summarization only supports dynamic tool calls".to_owned(),
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
                eprintln!(
                    "[summary-agent] turn error for room thread {} turn {}: {}",
                    payload.thread_id, payload.turn_id, payload.error
                );
            }
            ServerNotification::TurnCompleted(payload) => {
                let Some(room_id) = self.thread_to_room.get(&payload.thread_id).cloned() else {
                    return Ok(());
                };
                let Some(room) = self.rooms.get_mut(&room_id) else {
                    return Ok(());
                };
                if room.active_turn.as_deref() != Some(payload.turn.id.as_str()) {
                    return Ok(());
                }
                room.active_turn = None;
                let status = match payload.turn.status {
                    TurnStatus::Completed => SummaryStatus::Ready,
                    _ => SummaryStatus::Error,
                };
                self.emit_summary_status(&room_id, status).await;
            }
            _ => {}
        }

        Ok(())
    }

    async fn send_event(&mut self, room_id: &str, event: &StoredHookEvent) -> Result<()> {
        let thread_id = self.ensure_thread(room_id).await?;
        let input = vec![UserInput::Text {
            text: format_event(event)?,
            text_elements: Vec::new(),
        }];

        let active_turn = self
            .rooms
            .get(room_id)
            .and_then(|room| room.active_turn.clone());

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
                    if let Some(room) = self.rooms.get_mut(room_id) {
                        room.active_turn = Some(response.turn_id);
                    }
                }
                Err(error) => {
                    eprintln!("[summary-agent] steer failed for room {room_id}: {error}");
                }
            }
        } else {
            self.emit_summary_status(room_id, SummaryStatus::Generating)
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
                    if let Some(room) = self.rooms.get_mut(room_id) {
                        room.active_turn = Some(response.turn.id);
                    }
                }
                Err(error) => {
                    eprintln!("[summary-agent] turn start failed for room {room_id}: {error}");
                    self.emit_summary_status(room_id, SummaryStatus::Error)
                        .await;
                }
            }
        }

        Ok(())
    }

    async fn ensure_thread(&mut self, room_id: &str) -> Result<String> {
        if let Some(thread_id) = self
            .rooms
            .get(room_id)
            .and_then(|room| room.thread_id.clone())
        {
            return Ok(thread_id);
        }

        let cwd = self.room_cwd(room_id)?;
        let cwd_str = cwd.display().to_string();

        let stored_thread_id = self.read_thread_id(room_id)?;
        let thread_id = if let Some(thread_id) = stored_thread_id {
            match self.resume_thread(&thread_id, &cwd_str).await {
                Ok(thread_id) => thread_id,
                Err(error) => {
                    eprintln!(
                        "[summary-agent] failed to resume thread {thread_id} for room {room_id}: {error}. Creating new thread."
                    );
                    self.create_thread(&cwd_str).await?
                }
            }
        } else {
            self.create_thread(&cwd_str).await?
        };

        self.write_thread_id(room_id, &thread_id)?;

        let room = self
            .rooms
            .entry(room_id.to_owned())
            .or_insert_with(|| RoomState {
                thread_id: None,
                active_turn: None,
            });
        room.thread_id = Some(thread_id.clone());
        self.thread_to_room
            .insert(thread_id.clone(), room_id.to_owned());

        Ok(thread_id)
    }

    fn room_dir(&self, room_id: &str) -> PathBuf {
        self.rooms_dir.join(room_id)
    }

    fn room_cwd(&self, room_id: &str) -> Result<PathBuf> {
        let dir = self.room_dir(room_id).join("cwd");
        fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create room dir: {}", dir.display()))?;
        Ok(dir)
    }

    fn thread_id_path(&self, room_id: &str) -> Result<PathBuf> {
        let dir = self.room_dir(room_id);
        fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create room state dir: {}", dir.display()))?;
        Ok(dir.join("thread-id"))
    }

    fn read_thread_id(&self, room_id: &str) -> Result<Option<String>> {
        let path = self.thread_id_path(room_id)?;
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

    fn write_thread_id(&self, room_id: &str, thread_id: &str) -> Result<()> {
        let path = self.thread_id_path(room_id)?;
        fs::write(&path, thread_id)
            .with_context(|| format!("failed to write thread id to {}", path.display()))?;
        Ok(())
    }

    async fn create_thread(&mut self, cwd: &str) -> Result<String> {
        let request_id = self.next_request_id();
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
                    base_instructions: Some(SYSTEM_PROMPT.to_owned()),
                    ephemeral: Some(false),
                    dynamic_tools: Some(SummaryTool::specs()),
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
        let Some(room_id) = self.thread_to_room.get(&params.thread_id).cloned() else {
            return Ok(tool_failure(format!(
                "unknown room thread: {}",
                params.thread_id
            )));
        };

        let tool = match SummaryTool::parse(params) {
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
                room_id,
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

    async fn emit_summary_status(&self, room_id: &str, status: SummaryStatus) {
        let _ = self
            .output_tx
            .send(AgentMessage::SummaryStatus {
                room_id: room_id.to_owned(),
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
