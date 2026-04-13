pub mod summarize;
pub(crate) mod tools;

use std::{collections::HashMap, fs, path::PathBuf, sync::Arc};

use anyhow::{Context, Result, anyhow};
use codex_app_server_client::{
    DEFAULT_IN_PROCESS_CHANNEL_CAPACITY, InProcessAppServerClient, InProcessClientStartArgs,
    InProcessServerEvent,
};
use codex_app_server_protocol::{
    AskForApproval, ClientRequest, DynamicToolCallParams, DynamicToolCallResponse,
    JSONRPCErrorError, RequestId, SandboxMode, ServerNotification, ServerRequest,
    ThreadResumeParams, ThreadResumeResponse, ThreadStartParams, ThreadStartResponse,
    TurnStartParams, TurnStartResponse, TurnStatus, TurnSteerParams, TurnSteerResponse, UserInput,
};
use codex_arg0::Arg0DispatchPaths;
use codex_core::{
    config::Config,
    config_loader::{CloudRequirementsLoader, LoaderOverrides},
};
use codex_feedback::CodexFeedback;
use codex_protocol::protocol::SessionSource;
use reporter_protocol::StoredHookEvent;
use serde_json::Value;
use tokio::sync::{broadcast, mpsc};

use crate::state::StoragePaths;
use crate::store::Db;

use summarize::{SummaryStatus, SummaryStatusEvent, broadcast_status};
use tools::{SummaryTool, format_event, tool_failure};

const SUMMARY_MODEL: &str = "gpt-5.4-mini";
const CLIENT_NAME: &str = "supermanager_coordination_server";
const SYSTEM_PROMPT: &str = r#"You are the room summarizer for Supermanager.

Your job is to maintain a manager-facing snapshot of a live engineering coordination room. The snapshot is persistent across turns. You will receive hook events as structured text, and a turn may contain one event or several events if more arrived while you were still working. Your task is to fold the newest evidence into the existing room snapshot so a manager can quickly understand what matters now.

The room snapshot has three parts:

1. `bluf_markdown`
This is the top-of-page "bottom line up front". It should be the shortest, highest-signal summary of the room right now.
- Prefer 2-5 bullets.
- Focus on overall momentum, important changes, blockers, risk, and what needs attention.
- Do not dump raw activity logs here.

2. `overview_markdown`
This is the fuller room-level summary.
- Use short markdown paragraphs or bullets.
- Explain the main workstreams, what changed recently, where execution stands, and any coordination concerns.
- This should synthesize the room, not repeat every employee card line by line.

3. Employee cards
Each employee card represents one currently relevant person in the room.
- The card should capture that employee's current focus, recent progress, blockers, decisions, handoffs, or next steps if supported by evidence.
- Employee card markdown must be body content only. Do not include the employee name as a heading.
- Keep cards concise and specific.

Each incoming event has these fields:
- `employee_name`: the person associated with the event. Usually the employee whose card may need updating.
- `client`: which tool emitted the hook event, such as Codex or Claude.
- `repo_root`: the repository or workspace the event came from.
- `branch`: the git branch, if present.
- `received_at`: when the event reached the server.
- `payload_json`: the raw hook payload. This often contains the hook type, working directory, summaries, assistant output, task context, or other client-specific fields. Treat this as primary evidence.

How to interpret the incoming event:
- `employee_name` tells you whose card is most likely affected.
- `repo_root` and `branch` help you distinguish which project or stream of work the event belongs to.
- `client` can help explain the source, but it is usually less important than the content of `payload_json`.
- `received_at` tells you recency.
- `payload_json` is the evidence source for what changed.
- If multiple events arrive in one turn, process them in order and let the newest evidence win.

Tool contract:
- Always call `get_snapshot` before deciding what to edit.
- `set_bluf(markdown)` replaces the entire BLUF. When you call it, send the full new BLUF, not a patch.
- `set_overview(markdown)` replaces the entire overview. When you call it, send the full new overview, not a patch.
- `set_employee_card(employee_name, markdown)` creates or replaces one employee card. The server manages `last_update_at` for you.
- `remove_employee_card(employee_name)` deletes a card. Use this rarely and only when the current snapshot clearly contains a card that should no longer exist.

Editing rules:
- Update only the sections that should actually change.
- Preserve useful existing context from `get_snapshot`; do not rewrite everything by default.
- Use only facts grounded in the current snapshot and the event stream. Do not invent status, confidence, blockers, or ownership.
- If evidence is weak or ambiguous, stay conservative and write less.
- Prefer concrete work state over generic phrasing.
- Avoid repeating the same fact across BLUF, overview, and employee cards unless it is truly important at every level.
- Do not mention tools, prompts, or your internal process in the snapshot.
- Do not use shell, filesystem, network, or any tools besides the provided dynamic snapshot tools.

Content guidance:
- Emphasize changes in progress, blockers, risks, decisions, completed milestones, and next steps.
- If the event is minor or redundant, it may justify only a small employee card update and no room-level changes.
- If the snapshot is empty, initialize the BLUF, overview, and the relevant employee card or cards.
- Prefer markdown bullets for BLUF and employee cards.
- Keep writing crisp, operational, and manager-readable.

Removal guidance:
- Do not remove a card just because the new event mentions someone else.
- Only remove a card when the snapshot itself is clearly stale and the available evidence strongly supports that the person should no longer be tracked in this room snapshot.

After finishing any needed tool calls, end with a single short sentence."#;

#[derive(Clone)]
pub struct RoomSummaryAgent {
    command_tx: mpsc::Sender<AgentCommand>,
}

impl RoomSummaryAgent {
    pub async fn start(
        db: Arc<Db>,
        summary_events: broadcast::Sender<SummaryStatusEvent>,
        storage: StoragePaths,
    ) -> Result<Self> {
        let StoragePaths {
            codex_home,
            rooms_dir,
            ..
        } = storage;
        let config = Config::load_default_with_cli_overrides_for_codex_home(codex_home, Vec::new())
            .context("failed to load default Codex config")?;
        let client = InProcessAppServerClient::start(InProcessClientStartArgs {
            arg0_paths: Arg0DispatchPaths::default(),
            config: Arc::new(config),
            cli_overrides: Vec::new(),
            loader_overrides: LoaderOverrides::default(),
            cloud_requirements: CloudRequirementsLoader::default(),
            feedback: CodexFeedback::new(),
            config_warnings: Vec::new(),
            session_source: SessionSource::Custom("supermanager".to_owned()),
            enable_codex_api_key_env: true,
            client_name: CLIENT_NAME.to_owned(),
            client_version: env!("CARGO_PKG_VERSION").to_owned(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: DEFAULT_IN_PROCESS_CHANNEL_CAPACITY,
        })
        .await
        .context("failed to start in-process Codex app server")?;

        let (command_tx, command_rx) = mpsc::channel(256);

        tokio::spawn(async move {
            let loop_state = AgentLoop {
                client,
                command_rx,
                db,
                summary_events,
                rooms_dir,
                next_request_id: 1,
                rooms: HashMap::new(),
                thread_to_room: HashMap::new(),
            };

            if let Err(error) = loop_state.run().await {
                eprintln!("[room_summary_agent] fatal error: {error:#}");
            }
        });

        Ok(Self { command_tx })
    }

    pub async fn enqueue(&self, room_id: String, event: StoredHookEvent) -> Result<()> {
        self.command_tx
            .send(AgentCommand::EnqueueEvent { room_id, event })
            .await
            .map_err(|_| anyhow!("room summary agent is not running"))
    }
}

enum AgentCommand {
    EnqueueEvent {
        room_id: String,
        event: StoredHookEvent,
    },
}

enum LoopInput {
    Command(Option<AgentCommand>),
    Event(Option<InProcessServerEvent>),
}

struct AgentLoop {
    client: InProcessAppServerClient,
    command_rx: mpsc::Receiver<AgentCommand>,
    db: Arc<Db>,
    summary_events: broadcast::Sender<SummaryStatusEvent>,
    rooms_dir: PathBuf,
    next_request_id: i64,
    rooms: HashMap<String, RoomState>,
    thread_to_room: HashMap<String, String>,
}

struct RoomState {
    thread_id: Option<String>,
    active_turn: Option<String>,
}

impl AgentLoop {
    async fn run(mut self) -> Result<()> {
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
                LoopInput::Command(Some(command)) => self.handle_command(command).await?,
                LoopInput::Command(None) => break,
                LoopInput::Event(Some(event)) => self.handle_event(event).await?,
                LoopInput::Event(None) => break,
            }
        }

        Ok(())
    }

    async fn handle_command(&mut self, command: AgentCommand) -> Result<()> {
        match command {
            AgentCommand::EnqueueEvent { room_id, event } => {
                self.send_event(&room_id, &event).await?;
            }
        }
        Ok(())
    }

    async fn handle_event(&mut self, event: InProcessServerEvent) -> Result<()> {
        match event {
            InProcessServerEvent::Lagged { skipped } => {
                eprintln!("[room_summary_agent] lagged by {skipped} events");
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
                let response = self.execute_tool_call(&params).await;
                let result = serde_json::to_value(response?)?;
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
                    "[room_summary_agent] turn error for room thread {} turn {}: {}",
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
                broadcast_status(self.db.as_ref(), &self.summary_events, &room_id, status).await;
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
                    eprintln!("[room_summary_agent] steer failed for room {room_id}: {error}");
                }
            }
        } else {
            broadcast_status(
                self.db.as_ref(),
                &self.summary_events,
                room_id,
                SummaryStatus::Generating,
            )
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
                    eprintln!("[room_summary_agent] turn start failed for room {room_id}: {error}");
                    broadcast_status(
                        self.db.as_ref(),
                        &self.summary_events,
                        room_id,
                        SummaryStatus::Error,
                    )
                    .await;
                }
            }
        }

        Ok(())
    }

    fn room_cwd(&self, room_id: &str) -> Result<PathBuf> {
        let dir = self.rooms_dir.join(room_id).join("cwd");
        fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create room dir: {}", dir.display()))?;
        Ok(dir)
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

        let stored_thread_id = self.db.get_summary_thread_id(room_id).await?;
        let thread_id = if let Some(thread_id) = stored_thread_id {
            match self.resume_thread(&thread_id, &cwd_str).await {
                Ok(thread_id) => thread_id,
                Err(error) => {
                    eprintln!(
                        "[room_summary_agent] failed to resume thread {thread_id} for room {room_id}: {error}. Creating new thread."
                    );
                    self.create_thread(&cwd_str).await?
                }
            }
        } else {
            self.create_thread(&cwd_str).await?
        };

        self.db
            .set_summary_thread_id(room_id, &thread_id)
            .await
            .with_context(|| format!("failed to persist thread id for room {room_id}"))?;

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
        &self,
        params: &DynamicToolCallParams,
    ) -> Result<DynamicToolCallResponse> {
        let Some(room_id) = self.thread_to_room.get(&params.thread_id) else {
            return Ok(tool_failure(format!(
                "unknown room thread: {}",
                params.thread_id
            )));
        };

        match SummaryTool::parse(params) {
            Ok(tool) => match tool.execute(self.db.as_ref(), room_id).await {
                Ok(response) => Ok(response),
                Err(error) => Ok(tool_failure(error.to_string())),
            },
            Err(error) => Ok(tool_failure(error.to_string())),
        }
    }

    fn next_request_id(&mut self) -> RequestId {
        let request_id = self.next_request_id;
        self.next_request_id += 1;
        RequestId::Integer(request_id)
    }
}
