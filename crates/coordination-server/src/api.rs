use std::{convert::Infallible, path::PathBuf, time::Duration};

use async_stream::stream;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{
        IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
};
use reporter_protocol::{FeedResponse, IngestResponse, ProgressNote, StoredProgressNote};
use serde_json::{Value, json};
use tokio::sync::broadcast;

use crate::store;

#[derive(Clone)]
pub struct AppState {
    pub data_dir: PathBuf,
    pub note_events: broadcast::Sender<StoredProgressNote>,
}

pub async fn health() -> &'static str {
    "ok"
}

pub async fn ingest_progress(
    State(state): State<AppState>,
    Json(note): Json<ProgressNote>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let received_at = store::now_rfc3339();
    let stored = StoredProgressNote::new(note, received_at.clone());
    let note_id = stored.note_id;

    store::persist_note(&state.data_dir, &stored).map_err(internal_error)?;
    let _ = state.note_events.send(stored.clone());

    Ok((
        StatusCode::ACCEPTED,
        Json(IngestResponse {
            note_id,
            received_at,
        }),
    ))
}

pub async fn get_feed(
    State(state): State<AppState>,
) -> Result<Json<FeedResponse>, (StatusCode, String)> {
    let notes = store::read_all_notes(&state.data_dir).map_err(internal_error)?;
    Ok(Json(FeedResponse { notes }))
}

pub async fn get_manager_summary(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let summary = store::read_manager_summary(&state.data_dir).map_err(internal_error)?;
    Ok((
        [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
        summary,
    ))
}

pub async fn update_manager_summary(
    State(state): State<AppState>,
    body: String,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    store::persist_manager_summary(&state.data_dir, &body).map_err(internal_error)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn stream_feed(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)>
{
    let replay = headers
        .get("last-event-id")
        .and_then(|value| value.to_str().ok())
        .map(|note_id| store::read_notes_after(&state.data_dir, note_id))
        .transpose()
        .map_err(internal_error)?
        .unwrap_or_default();

    let mut receiver = state.note_events.subscribe();
    let event_stream = stream! {
        for note in replay {
            yield Ok(progress_event(&note));
        }

        loop {
            match receiver.recv().await {
                Ok(note) => yield Ok(progress_event(&note)),
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    yield Ok(Event::default()
                        .event("warning")
                        .data(format!("lagged:{skipped}")));
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    Ok(Sse::new(event_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

pub async fn handle_mcp(
    State(state): State<AppState>,
    Json(req): Json<Value>,
) -> impl IntoResponse {
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");

    let result = match method {
        "initialize" => {
            let client_version = req
                .pointer("/params/protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or("2025-03-26");
            json!({
                "protocolVersion": client_version,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "supermanager", "version": "0.1.0" },
                "instructions": "CRITICAL: Your FIRST tool call in every conversation must be submit_progress to report that you are starting work. Call submit_progress again when you make meaningful progress, change direction, hit a blocker, or finish. Keep summaries concise. Never skip this — even when other tools need to be called, include submit_progress in parallel."
            })
        }
        _ if method.starts_with("notifications/") => {
            return StatusCode::ACCEPTED.into_response();
        }
        "tools/list" => json!({
            "tools": [
                {
                    "name": "submit_progress",
                    "description": "Submit a progress update to the coordination server. Use this to report what you accomplished.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "employee_name": {
                                "type": "string",
                                "description": "Name of the human user you are working for — never use 'Claude' or your own name"
                            },
                            "repo": {
                                "type": "string",
                                "description": "Repository URL or identifier"
                            },
                            "branch": {
                                "type": "string",
                                "description": "Git branch name"
                            },
                            "progress_text": {
                                "type": "string",
                                "description": "A concise summary of what was accomplished"
                            }
                        },
                        "required": ["employee_name", "repo", "branch", "progress_text"]
                    }
                },
                {
                    "name": "get_feed",
                    "description": "Get the feed of all progress updates from all employees.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {}
                    }
                },
                {
                    "name": "get_manager_summary",
                    "description": "Read the manager-facing Markdown summary document that lives on the coordination server.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {}
                    }
                },
                {
                    "name": "update_manager_summary",
                    "description": "Replace the manager-facing Markdown summary document on the coordination server.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "content_markdown": {
                                "type": "string",
                                "description": "Full Markdown contents for the manager summary document."
                            }
                        },
                        "required": ["content_markdown"]
                    }
                }
            ]
        }),
        "tools/call" => {
            let tool_name = req
                .pointer("/params/name")
                .and_then(Value::as_str)
                .unwrap_or("");
            match tool_name {
                "submit_progress" => mcp_submit_progress(&state, &req),
                "get_feed" => mcp_get_feed(&state),
                "get_manager_summary" => mcp_get_manager_summary(&state),
                "update_manager_summary" => mcp_update_manager_summary(&state, &req),
                _ => json!({
                    "isError": true,
                    "content": [{ "type": "text", "text": format!("Unknown tool: {tool_name}") }]
                }),
            }
        }
        _ => {
            return Json(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("Unknown method: {method}") }
            }))
            .into_response();
        }
    };

    Json(json!({ "jsonrpc": "2.0", "id": id, "result": result })).into_response()
}

fn mcp_submit_progress(state: &AppState, req: &Value) -> Value {
    let args = req.pointer("/params/arguments");
    let str_arg = |field| {
        args.and_then(|a| a.get(field))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned()
    };

    let note = ProgressNote {
        employee_name: str_arg("employee_name"),
        repo: str_arg("repo"),
        branch: Some(str_arg("branch")),
        progress_text: str_arg("progress_text"),
    };
    let stored = StoredProgressNote::new(note, store::now_rfc3339());
    let note_id = stored.note_id;

    match store::persist_note(&state.data_dir, &stored) {
        Ok(_) => {
            let _ = state.note_events.send(stored.clone());
            json!({
                "content": [{ "type": "text", "text": format!("Progress submitted (note_id: {note_id})") }]
            })
        }
        Err(e) => json!({
            "isError": true,
            "content": [{ "type": "text", "text": format!("Failed to submit: {e}") }]
        }),
    }
}

fn mcp_get_feed(state: &AppState) -> Value {
    match store::read_all_notes(&state.data_dir) {
        Ok(notes) => json!({
            "content": [{ "type": "text", "text": serde_json::to_string_pretty(&notes).unwrap_or_default() }]
        }),
        Err(e) => json!({
            "isError": true,
            "content": [{ "type": "text", "text": format!("Failed to read feed: {e}") }]
        }),
    }
}

fn mcp_get_manager_summary(state: &AppState) -> Value {
    match store::read_manager_summary(&state.data_dir) {
        Ok(summary) => json!({
            "content": [{ "type": "text", "text": summary }]
        }),
        Err(e) => json!({
            "isError": true,
            "content": [{ "type": "text", "text": format!("Failed to read manager summary: {e}") }]
        }),
    }
}

fn mcp_update_manager_summary(state: &AppState, req: &Value) -> Value {
    let content_markdown = req
        .pointer("/params/arguments/content_markdown")
        .and_then(Value::as_str)
        .unwrap_or("");

    if content_markdown.is_empty() {
        return json!({
            "isError": true,
            "content": [{ "type": "text", "text": "Missing required field: content_markdown" }]
        });
    }

    match store::persist_manager_summary(&state.data_dir, content_markdown) {
        Ok(_) => json!({
            "content": [{
                "type": "text",
                "text": format!(
                    "Manager summary updated at {}",
                    store::manager_summary_path(&state.data_dir).display()
                )
            }]
        }),
        Err(e) => json!({
            "isError": true,
            "content": [{ "type": "text", "text": format!("Failed to update manager summary: {e}") }]
        }),
    }
}

fn internal_error(error: anyhow::Error) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}

fn progress_event(note: &StoredProgressNote) -> Event {
    let data = serde_json::to_string(note).unwrap_or_else(|_| "{}".to_owned());
    Event::default()
        .event("progress_note")
        .id(note.note_id.to_string())
        .data(data)
}
