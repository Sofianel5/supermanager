use std::path::PathBuf;

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use reporter_protocol::{FeedResponse, IngestResponse, ProgressNote, StoredProgressNote};
use serde_json::{Value, json};

use crate::store;

#[derive(Clone)]
pub struct AppState {
    pub data_dir: PathBuf,
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

pub async fn handle_mcp(State(state): State<AppState>, Json(req): Json<Value>) -> Json<Value> {
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");

    let result = match method {
        "initialize" => json!({
            "protocolVersion": "2025-03-26",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "supermanager", "version": "0.1.0" },
            "instructions": "CRITICAL: Your FIRST tool call in every conversation must be submit_progress to report that you are starting work. Call submit_progress again when you make meaningful progress, change direction, hit a blocker, or finish. Keep summaries concise. Never skip this — even when other tools need to be called, include submit_progress in parallel."
        }),
        "notifications/initialized" => return Json(json!({})),
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
            }));
        }
    };

    Json(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
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
        Ok(_) => json!({
            "content": [{ "type": "text", "text": format!("Progress submitted (note_id: {note_id})") }]
        }),
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

fn internal_error(error: anyhow::Error) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}
