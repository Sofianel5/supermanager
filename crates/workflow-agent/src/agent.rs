use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use codex_app_server_client::{InProcessAppServerClient, InProcessServerEvent, TypedRequestError};
use codex_app_server_protocol::{
    ClientRequest, DynamicToolCallOutputContentItem, DynamicToolCallParams,
    DynamicToolCallResponse, JSONRPCErrorError, RequestId, ServerNotification, ServerRequest,
    Thread, ThreadResumeParams, ThreadResumeResponse, ThreadStartParams, ThreadStartResponse,
    TurnInterruptParams, TurnInterruptResponse, TurnStartParams, TurnStartResponse, TurnStatus,
    TurnSteerParams, TurnSteerResponse, UserInput,
};
use reporter_protocol::SummaryStatus;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::{
    db::SummaryDb,
    tools::{SummaryTool, tool_failure},
    workflow::{
        WorkflowDecisionRequirement, WorkflowDispatch, WorkflowKind, WorkflowPaths, WorkflowTarget,
    },
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

#[derive(Default)]
struct WorkflowTargetState {
    thread_id: Option<String>,
    active_turn: Option<String>,
    required_decisions: HashSet<WorkflowDecisionRequirement>,
    satisfied_decisions: HashSet<WorkflowDecisionRequirement>,
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
            ServerNotification::TurnStarted(payload) => {
                let Some(target) = self.thread_to_target.get(&payload.thread_id).cloned() else {
                    return Ok(());
                };
                let state = self.targets.entry(target).or_default();
                state.active_turn = Some(payload.turn.id);
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

                let status = completion_status_for_turn(&target, state, &payload.turn.status);
                if status == SummaryStatus::Error
                    && payload.turn.status == TurnStatus::Completed
                    && !missing_required_decisions(state).is_empty()
                {
                    let missing = missing_required_decisions(state)
                        .into_iter()
                        .map(|decision| decision.label())
                        .collect::<Vec<_>>()
                        .join(", ");
                    eprintln!(
                        "[workflow-agent] completed {} without explicit update decisions for {missing}",
                        target.label()
                    );
                }

                reset_turn_tracking(state);
                self.emit_workflow_status(&target, status).await;
            }
            _ => {}
        }

        Ok(())
    }

    async fn dispatch_workflow(&mut self, dispatch: WorkflowDispatch) -> Result<()> {
        let thread_id = self.ensure_thread(&dispatch.target).await?;
        for decision in dispatch.required_decisions {
            self.register_required_decision(&dispatch.target, decision);
        }
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
                            reset_turn_tracking(state);
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
                            reset_turn_tracking(state);
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
                if let Some(state) = self.targets.get_mut(target) {
                    reset_turn_tracking(state);
                }
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
        let thread_contract_path = target_dir.join("thread-contract.json");
        let current_contract = target
            .kind
            .thread_contract()
            .context("failed to build workflow thread contract")?;

        let reusable_thread_id = match (
            read_thread_id(&thread_id_path).await?,
            read_thread_contract(&thread_contract_path).await?,
        ) {
            (Some(thread_id), Some(stored_contract)) if stored_contract == current_contract => {
                Some(thread_id)
            }
            (Some(thread_id), Some(_)) => {
                eprintln!(
                    "[workflow-agent] workflow thread contract changed for {}. Recreating thread {}.",
                    target.label(),
                    thread_id,
                );
                None
            }
            (Some(thread_id), None) => {
                eprintln!(
                    "[workflow-agent] workflow thread contract missing for {}. Recreating thread {}.",
                    target.label(),
                    thread_id,
                );
                None
            }
            (None, _) => None,
        };

        let (thread_id, active_turn) = if let Some(thread_id) = reusable_thread_id {
            match self.resume_thread(&thread_id, &cwd_str, target.kind).await {
                Ok(response) => {
                    let active_turn = active_turn_id(&response.thread);
                    (response.thread.id, active_turn)
                }
                Err(error) => {
                    eprintln!(
                        "[workflow-agent] failed to resume workflow thread {} for {}: {error}. Creating new thread.",
                        thread_id,
                        target.label(),
                    );
                    (self.create_thread(target.kind, &cwd_str).await?, None)
                }
            }
        } else {
            (self.create_thread(target.kind, &cwd_str).await?, None)
        };

        tokio::fs::write(&thread_id_path, &thread_id)
            .await
            .with_context(|| {
                format!("failed to write thread id to {}", thread_id_path.display())
            })?;
        tokio::fs::write(&thread_contract_path, &current_contract)
            .await
            .with_context(|| {
                format!(
                    "failed to write thread contract to {}",
                    thread_contract_path.display()
                )
            })?;

        bind_thread_to_target(
            target,
            thread_id.clone(),
            active_turn,
            &mut self.targets,
            &mut self.thread_to_target,
        );

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
    ) -> Result<ThreadResumeResponse> {
        let request_id = self.next_request_id();
        self.client
            .request_typed::<ThreadResumeResponse>(ClientRequest::ThreadResume {
                request_id,
                params: ThreadResumeParams {
                    thread_id: thread_id.to_owned(),
                    cwd: Some(cwd.to_owned()),
                    approval_policy: Some(kind.approval_policy()),
                    base_instructions: Some(kind.system_prompt().to_owned()),
                    ..Default::default()
                },
            })
            .await
            .with_context(|| format!("failed to resume Codex workflow thread {thread_id}"))
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
        };
        let tool = match tool {
            Ok(tool) => tool,
            Err(error) => return Ok(tool_failure(error.to_string())),
        };
        let satisfied_decision = decision_satisfied_by_tool(target.kind, &tool);

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

        if let Some(decision) = satisfied_decision {
            self.mark_decision_satisfied(&target, decision);
        }

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

    fn register_required_decision(
        &mut self,
        target: &WorkflowTarget,
        decision: WorkflowDecisionRequirement,
    ) {
        let state = self.targets.entry(target.clone()).or_default();
        state.required_decisions.insert(decision.clone());
        state.satisfied_decisions.remove(&decision);
    }

    fn mark_decision_satisfied(
        &mut self,
        target: &WorkflowTarget,
        decision: WorkflowDecisionRequirement,
    ) {
        if let Some(state) = self.targets.get_mut(target) {
            state.satisfied_decisions.insert(decision);
        }
    }
}

fn decision_satisfied_by_tool(
    workflow_kind: WorkflowKind,
    tool: &SummaryTool,
) -> Option<WorkflowDecisionRequirement> {
    match (workflow_kind, tool) {
        (
            WorkflowKind::ProjectSummary,
            SummaryTool::SetEventUpdates {
                source_event_id, ..
            },
        ) => Some(WorkflowDecisionRequirement::ProjectEvent {
            source_event_id: source_event_id.trim().to_owned(),
        }),
        (
            WorkflowKind::OrganizationSummary,
            SummaryTool::SetWindowUpdates {
                source_window_key, ..
            },
        ) => Some(WorkflowDecisionRequirement::OrganizationWindow {
            source_window_key: source_window_key.trim().to_owned(),
        }),
        _ => None,
    }
}

fn completion_status_for_turn(
    target: &WorkflowTarget,
    state: &WorkflowTargetState,
    turn_status: &TurnStatus,
) -> SummaryStatus {
    match turn_status {
        TurnStatus::Completed if missing_required_decisions(state).is_empty() => {
            SummaryStatus::Ready
        }
        TurnStatus::Completed
            if matches!(
                target.kind,
                WorkflowKind::ProjectSummary | WorkflowKind::OrganizationSummary
            ) =>
        {
            SummaryStatus::Error
        }
        TurnStatus::Completed => SummaryStatus::Ready,
        _ => SummaryStatus::Error,
    }
}

fn missing_required_decisions(state: &WorkflowTargetState) -> Vec<WorkflowDecisionRequirement> {
    state
        .required_decisions
        .difference(&state.satisfied_decisions)
        .cloned()
        .collect()
}

fn reset_turn_tracking(state: &mut WorkflowTargetState) {
    state.active_turn = None;
    state.required_decisions.clear();
    state.satisfied_decisions.clear();
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

fn active_turn_id(thread: &Thread) -> Option<String> {
    thread
        .turns
        .iter()
        .rev()
        .find(|turn| matches!(turn.status, TurnStatus::InProgress))
        .map(|turn| turn.id.clone())
}

fn bind_thread_to_target(
    target: &WorkflowTarget,
    thread_id: String,
    active_turn: Option<String>,
    targets: &mut HashMap<WorkflowTarget, WorkflowTargetState>,
    thread_to_target: &mut HashMap<String, WorkflowTarget>,
) {
    let state = targets.entry(target.clone()).or_default();
    if let Some(previous_thread_id) = state.thread_id.replace(thread_id.clone())
        && previous_thread_id != thread_id
    {
        thread_to_target.remove(&previous_thread_id);
    }
    state.active_turn = active_turn;
    thread_to_target.insert(thread_id, target.clone());
}

async fn read_thread_id(path: &std::path::Path) -> Result<Option<String>> {
    read_optional_nonempty_file(path, "thread id").await
}

async fn read_thread_contract(path: &std::path::Path) -> Result<Option<String>> {
    read_optional_nonempty_file(path, "thread contract").await
}

async fn read_optional_nonempty_file(
    path: &std::path::Path,
    description: &str,
) -> Result<Option<String>> {
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
        Err(error) => Err(error)
            .with_context(|| format!("failed to read {description} from {}", path.display())),
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

    #[test]
    fn resume_response_prefers_latest_in_progress_turn() {
        let thread = Thread {
            id: "thread_123".to_owned(),
            forked_from_id: None,
            preview: String::new(),
            ephemeral: false,
            model_provider: "openai".to_owned(),
            created_at: 0,
            updated_at: 0,
            status: codex_app_server_protocol::ThreadStatus::Idle,
            path: None,
            cwd: std::path::PathBuf::from("/tmp"),
            cli_version: "0.0.0".to_owned(),
            source: codex_app_server_protocol::SessionSource::Cli,
            agent_nickname: None,
            agent_role: None,
            git_info: None,
            name: None,
            turns: vec![
                codex_app_server_protocol::Turn {
                    id: "turn_1".to_owned(),
                    items: Vec::new(),
                    status: TurnStatus::Completed,
                    error: None,
                },
                codex_app_server_protocol::Turn {
                    id: "turn_2".to_owned(),
                    items: Vec::new(),
                    status: TurnStatus::InProgress,
                    error: None,
                },
            ],
        };

        assert_eq!(active_turn_id(&thread), Some("turn_2".to_owned()));
    }

    #[test]
    fn replacing_thread_mapping_drops_stale_thread_id() {
        let target = WorkflowTarget::new(WorkflowKind::ProjectMemoryExtract, "PROJECT42");
        let mut targets = HashMap::new();
        let mut thread_to_target = HashMap::new();

        bind_thread_to_target(
            &target,
            "thread_1".to_owned(),
            None,
            &mut targets,
            &mut thread_to_target,
        );
        bind_thread_to_target(
            &target,
            "thread_2".to_owned(),
            Some("turn_2".to_owned()),
            &mut targets,
            &mut thread_to_target,
        );

        assert!(!thread_to_target.contains_key("thread_1"));
        assert_eq!(thread_to_target.get("thread_2"), Some(&target));
        assert_eq!(
            targets
                .get(&target)
                .and_then(|state| state.active_turn.as_deref()),
            Some("turn_2"),
        );
    }

    #[test]
    fn project_summary_requires_explicit_event_decisions_before_ready() {
        let target = WorkflowTarget::new(WorkflowKind::ProjectSummary, "PROJECT42");
        let mut state = WorkflowTargetState::default();
        state
            .required_decisions
            .insert(WorkflowDecisionRequirement::ProjectEvent {
                source_event_id: "event-123".to_owned(),
            });

        assert_eq!(
            completion_status_for_turn(&target, &state, &TurnStatus::Completed),
            SummaryStatus::Error,
        );

        state
            .satisfied_decisions
            .insert(WorkflowDecisionRequirement::ProjectEvent {
                source_event_id: "event-123".to_owned(),
            });

        assert_eq!(
            completion_status_for_turn(&target, &state, &TurnStatus::Completed),
            SummaryStatus::Ready,
        );
    }

    #[test]
    fn organization_summary_requires_explicit_window_decision_before_ready() {
        let target = WorkflowTarget::new(WorkflowKind::OrganizationSummary, "ORG42");
        let mut state = WorkflowTargetState::default();
        state
            .required_decisions
            .insert(WorkflowDecisionRequirement::OrganizationWindow {
                source_window_key: "window-123".to_owned(),
            });

        assert_eq!(
            completion_status_for_turn(&target, &state, &TurnStatus::Completed),
            SummaryStatus::Error,
        );

        state
            .satisfied_decisions
            .insert(WorkflowDecisionRequirement::OrganizationWindow {
                source_window_key: "window-123".to_owned(),
            });

        assert_eq!(
            completion_status_for_turn(&target, &state, &TurnStatus::Completed),
            SummaryStatus::Ready,
        );
    }

    #[test]
    fn project_update_tools_satisfy_matching_requirements() {
        assert_eq!(
            decision_satisfied_by_tool(
                WorkflowKind::ProjectSummary,
                &SummaryTool::SetEventUpdates {
                    source_event_id: " event-123 ".to_owned(),
                    project_updates: Vec::new(),
                    member_update: None,
                },
            ),
            Some(WorkflowDecisionRequirement::ProjectEvent {
                source_event_id: "event-123".to_owned(),
            }),
        );

        assert_eq!(
            decision_satisfied_by_tool(
                WorkflowKind::OrganizationSummary,
                &SummaryTool::SetWindowUpdates {
                    source_window_key: " window-123 ".to_owned(),
                    updates: Vec::new(),
                },
            ),
            Some(WorkflowDecisionRequirement::OrganizationWindow {
                source_window_key: "window-123".to_owned(),
            }),
        );
    }
}
