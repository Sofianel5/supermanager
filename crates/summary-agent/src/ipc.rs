use std::{collections::HashMap, sync::Arc};

use anyhow::{Context, Result};
use reporter_protocol::StoredHookEvent;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::{Mutex, mpsc, oneshot},
};

use crate::agent::AgentCommand;
use crate::event::{RegenerationEvent, RegenerationRoom};

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum HostMessage {
    EnqueueEvent {
        organization_id: String,
        room_id: String,
        room_name: String,
        event: StoredHookEvent,
    },
    RegenerateOrganization {
        organization_id: String,
        events: Vec<RegenerationEvent>,
        rooms: Vec<RegenerationRoom>,
        reason: String,
    },
    ToolResult {
        id: String,
        success: bool,
        message: String,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum AgentMessage {
    ToolCall {
        id: String,
        organization_id: String,
        tool: String,
        arguments: Value,
    },
    SummaryStatus {
        organization_id: String,
        status: String,
    },
}

#[derive(Debug)]
pub(crate) struct ToolResult {
    pub(crate) success: bool,
    pub(crate) message: String,
}

pub(crate) type PendingToolCalls = Arc<Mutex<HashMap<String, oneshot::Sender<ToolResult>>>>;

pub(crate) async fn read_host_messages(
    command_tx: mpsc::Sender<AgentCommand>,
    pending_tool_calls: PendingToolCalls,
) -> Result<()> {
    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();

    while let Some(line) = lines
        .next_line()
        .await
        .context("failed to read stdin line")?
    {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let message: HostMessage = match serde_json::from_str(line) {
            Ok(message) => message,
            Err(error) => {
                eprintln!("[summary-agent] invalid host message: {error}");
                continue;
            }
        };

        match message {
            HostMessage::EnqueueEvent {
                organization_id,
                room_id,
                room_name,
                event,
            } => {
                if command_tx
                    .send(AgentCommand::EnqueueEvent {
                        organization_id,
                        room_id,
                        room_name,
                        event,
                    })
                    .await
                    .is_err()
                {
                    break;
                }
            }
            HostMessage::RegenerateOrganization {
                organization_id,
                events,
                rooms,
                reason,
            } => {
                if command_tx
                    .send(AgentCommand::RegenerateOrganization {
                        organization_id,
                        events,
                        rooms,
                        reason,
                    })
                    .await
                    .is_err()
                {
                    break;
                }
            }
            HostMessage::ToolResult {
                id,
                success,
                message,
            } => {
                let sender = pending_tool_calls.lock().await.remove(&id);
                if let Some(sender) = sender {
                    let _ = sender.send(ToolResult { success, message });
                } else {
                    eprintln!("[summary-agent] unknown tool result id: {id}");
                }
            }
        }
    }

    let _ = command_tx.send(AgentCommand::Shutdown).await;
    Ok(())
}

pub(crate) async fn write_agent_messages(
    mut output_rx: mpsc::Receiver<AgentMessage>,
) -> Result<()> {
    let stdout = tokio::io::stdout();
    let mut stdout = tokio::io::BufWriter::new(stdout);

    while let Some(message) = output_rx.recv().await {
        let line = serde_json::to_string(&message).context("failed to encode agent message")?;
        stdout
            .write_all(line.as_bytes())
            .await
            .context("failed to write agent message")?;
        stdout
            .write_all(b"\n")
            .await
            .context("failed to write agent newline")?;
        stdout
            .flush()
            .await
            .context("failed to flush agent stdout")?;
    }

    Ok(())
}
