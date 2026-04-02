use std::{convert::Infallible, sync::Arc, time::Duration};

use anyhow::{Context, bail};
use async_stream::stream;
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{
        IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
};
use reporter_protocol::{
    CreateRoomRequest, CreateRoomResponse, FeedResponse, HookTurnReport, IngestResponse,
    PublicConfigResponse, RoomMetadataResponse, StoredHookEvent,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::broadcast;

use crate::store::Db;

// ── Shared state ────────────────────────────────────────────

#[derive(Clone)]
pub struct HookFeedEvent {
    pub room_id: String,
    pub event: StoredHookEvent,
}

#[derive(Clone)]
pub struct SummaryStatusEvent {
    pub room_id: String,
    pub status: String, // "generating", "ready", "error"
}

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Db>,
    pub hook_events: broadcast::Sender<HookFeedEvent>,
    pub summary_events: broadcast::Sender<SummaryStatusEvent>,
    pub public_api_url: String,
    pub public_app_url: String,
    pub cli_install_command: String,
    pub http: reqwest::Client,
    pub openai_api_key: Option<String>,
}

// ── Query params ────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SecretQuery {
    pub secret: Option<String>,
}

// ── Helper: extract secret from header or query ─────────────

fn extract_secret(headers: &HeaderMap, query: &SecretQuery) -> Option<String> {
    // Try Authorization: Bearer <secret> first
    if let Some(auth) = headers.get(header::AUTHORIZATION)
        && let Ok(val) = auth.to_str()
        && let Some(token) = val.strip_prefix("Bearer ")
    {
        let token = token.trim();
        if !token.is_empty() {
            return Some(token.to_owned());
        }
    }
    // Fall back to query param
    query.secret.clone().filter(|s| !s.is_empty())
}

// ── Health ──────────────────────────────────────────────────

pub async fn health() -> &'static str {
    "ok"
}

pub async fn public_config(State(state): State<AppState>) -> Json<PublicConfigResponse> {
    Json(PublicConfigResponse {
        install_command: state.cli_install_command.clone(),
    })
}

// ── Room management ─────────────────────────────────────────

pub async fn create_room(
    State(state): State<AppState>,
    Json(req): Json<CreateRoomRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let room = state.db.create_room(&req.name).map_err(internal_error)?;
    let resp = CreateRoomResponse {
        install_command: state.cli_install_command.clone(),
        dashboard_url: dashboard_url(&state.public_app_url, &room.room_id, &room.secret),
        join_command: cli_join_command(
            &state.public_api_url,
            &state.public_app_url,
            &room.room_id,
            &room.secret,
        ),
        room_id: room.room_id,
        secret: room.secret,
    };
    Ok((StatusCode::CREATED, Json(resp)))
}

// ── Room-scoped routes ──────────────────────────────────────

fn require_room_access(
    state: &AppState,
    room_id: &str,
    headers: &HeaderMap,
    query: &SecretQuery,
) -> Result<(), (StatusCode, String)> {
    let secret = extract_secret(headers, query)
        .ok_or((StatusCode::UNAUTHORIZED, "missing secret".to_owned()))?;

    let valid = state
        .db
        .verify_room_secret(room_id, &secret)
        .map_err(internal_error)?;
    if !valid {
        return Err((StatusCode::UNAUTHORIZED, "invalid secret".to_owned()));
    }

    Ok(())
}

pub async fn ingest_hook_turn(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<SecretQuery>,
    Json(report): Json<HookTurnReport>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    require_room_access(&state, &room_id, &headers, &query)?;

    let stored = state
        .db
        .insert_hook_event(&room_id, &report)
        .map_err(internal_error)?;
    let event_id = stored.event_id;
    let received_at = stored.received_at.clone();

    let _ = state.hook_events.send(HookFeedEvent {
        room_id: room_id.clone(),
        event: stored,
    });

    spawn_auto_summarize(&state, &room_id);

    Ok((
        StatusCode::ACCEPTED,
        Json(IngestResponse {
            event_id,
            received_at,
        }),
    ))
}

pub async fn get_room(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<SecretQuery>,
) -> Result<Json<RoomMetadataResponse>, (StatusCode, String)> {
    require_room_access(&state, &room_id, &headers, &query)?;
    let room = state.db.get_room(&room_id).map_err(internal_error)?;
    match room {
        Some(room) => Ok(Json(RoomMetadataResponse {
            room_id: room.room_id,
            name: room.name,
            created_at: room.created_at,
        })),
        None => Err((StatusCode::NOT_FOUND, format!("room not found: {room_id}"))),
    }
}

pub async fn get_feed(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<SecretQuery>,
) -> Result<Json<FeedResponse>, (StatusCode, String)> {
    require_room_access(&state, &room_id, &headers, &query)?;
    let events = state.db.get_hook_events(&room_id).map_err(internal_error)?;
    Ok(Json(FeedResponse { events }))
}

pub async fn stream_feed(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<SecretQuery>,
) -> Result<Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)>
{
    require_room_access(&state, &room_id, &headers, &query)?;

    let replay = headers
        .get("last-event-id")
        .and_then(|value| value.to_str().ok())
        .map(|event_id| state.db.get_hook_events_after(&room_id, event_id))
        .transpose()
        .map_err(internal_error)?
        .unwrap_or_default();

    let mut hook_rx = state.hook_events.subscribe();
    let mut summary_rx = state.summary_events.subscribe();
    let target_room = room_id.clone();

    // Send initial summary status
    let initial_status = state
        .db
        .get_summary_status(&room_id)
        .unwrap_or_else(|_| "ready".to_owned());

    let event_stream = stream! {
        // Replay missed events
        for event in replay {
            yield Ok(hook_event(&event));
        }

        // Send current summary status on connect
        yield Ok(Event::default()
            .event("summary_status")
            .data(json!({ "status": initial_status }).to_string()));

        loop {
            tokio::select! {
                hook_result = hook_rx.recv() => {
                    match hook_result {
                        Ok(evt) => {
                            if evt.room_id == target_room {
                                yield Ok(hook_event(&evt.event));
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            yield Ok(Event::default()
                                .event("warning")
                                .data(format!("lagged:{skipped}")));
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
                summary_result = summary_rx.recv() => {
                    match summary_result {
                        Ok(evt) => {
                            if evt.room_id == target_room {
                                yield Ok(Event::default()
                                    .event("summary_status")
                                    .data(json!({ "status": evt.status }).to_string()));
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {}
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    };

    Ok(Sse::new(event_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

pub async fn get_manager_summary(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<SecretQuery>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    require_room_access(&state, &room_id, &headers, &query)?;
    let summary = state.db.get_summary(&room_id).map_err(internal_error)?;
    Ok((
        [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
        summary,
    ))
}

/// Shared: resolve filter args → fetch hook events → format context string.
fn resolve_hook_context(
    state: &AppState,
    room_id: &str,
    args: &Value,
    default_limit: u32,
) -> anyhow::Result<(String, String)> {
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(default_limit as u64) as u32;
    let minutes = args.get("minutes").and_then(Value::as_u64);
    let employee_name = args.get("employee_name").and_then(Value::as_str);
    let branch = args.get("branch").and_then(Value::as_str);
    let since_last_update_by = args.get("since_last_update_by").and_then(Value::as_str);

    // Resolve time cutoff
    let after_time = if let Some(person) = since_last_update_by {
        state
            .db
            .get_last_hook_event_time(room_id, person)
            .with_context(|| format!("failed to look up last hook event by {person}"))?
    } else if let Some(mins) = minutes {
        let cutoff = time::OffsetDateTime::now_utc() - time::Duration::minutes(mins as i64);
        Some(
            cutoff
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
        )
    } else {
        None
    };

    let events = state
        .db
        .get_hook_events_filtered(room_id, after_time.as_deref(), employee_name, branch, limit)
        .context("failed to fetch hook events")?;

    if events.is_empty() {
        bail!("no hook updates found matching the filter");
    }

    let mut context = String::new();
    for event in &events {
        let line = json!({
            "received_at": event.received_at,
            "employee_name": event.employee_name,
            "client": event.client,
            "repo_root": event.repo_root,
            "branch": event.branch,
            "payload": event.payload,
        });
        context.push_str(&serde_json::to_string(&line).unwrap_or_default());
        context.push('\n');
    }

    let mut filter_desc = format!("{} most recent hook updates", events.len());
    if let Some(name) = employee_name {
        filter_desc = format!("{filter_desc} from {name}");
    }
    if let Some(b) = branch {
        filter_desc = format!("{filter_desc} on branch {b}");
    }
    if let Some(person) = since_last_update_by {
        filter_desc = format!("{filter_desc} (since {person}'s last update)");
    }
    if let Some(mins) = minutes {
        filter_desc = format!("{filter_desc} from the last {mins} minutes");
    }

    Ok((context, filter_desc))
}

/// Shared: call OpenAI Responses API and return the generated text.
async fn call_openai(state: &AppState, instructions: &str, input: &str) -> anyhow::Result<String> {
    let api_key = match &state.openai_api_key {
        Some(k) => k,
        None => bail!("OPENAI_API_KEY not configured on the server"),
    };

    let body = json!({
        "model": "gpt-5.4-mini",
        "instructions": instructions,
        "input": input,
    });

    eprintln!("[call_openai] sending request to OpenAI (model: gpt-5.4-mini)");
    let resp = state
        .http
        .post("https://api.openai.com/v1/responses")
        .header("Authorization", format!("Bearer {api_key}"))
        .json(&body)
        .send()
        .await;

    let resp = match resp {
        Ok(r) => r,
        Err(e) => bail!("OpenAI request failed: {e}"),
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        bail!("OpenAI returned {status}: {body_text}");
    }

    let resp_json: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => bail!("Failed to parse OpenAI response: {e}"),
    };

    Ok(resp_json
        .pointer("/output/0/content/0/text")
        .and_then(Value::as_str)
        .unwrap_or("(empty response from OpenAI)")
        .to_owned())
}

/// Background auto-summarize: triggered after every new hook event.
async fn auto_summarize(state: &AppState, room_id: &str) {
    eprintln!("[auto_summarize] starting for room {room_id}");

    // Mark as generating + broadcast
    let _ = state.db.set_summary_status(room_id, "generating");
    let _ = state.summary_events.send(SummaryStatusEvent {
        room_id: room_id.to_owned(),
        status: "generating".to_owned(),
    });

    // Build context from the most recent hook events
    let args = json!({});
    let (context, filter_desc) = match resolve_hook_context(state, room_id, &args, 100) {
        Ok(v) => v,
        Err(error) => {
            eprintln!("[auto_summarize] no hook events available for room {room_id}: {error}");
            let _ = state.db.set_summary_status(room_id, "ready");
            let _ = state.summary_events.send(SummaryStatusEvent {
                room_id: room_id.to_owned(),
                status: "ready".to_owned(),
            });
            return;
        }
    };

    eprintln!("[auto_summarize] calling OpenAI with {filter_desc}");

    let result = call_openai(
        state,
        "You are a concise project manager assistant. You will receive raw hook updates from coding agents as JSON lines. Each line includes metadata such as employee_name, client, repo_root, branch, received_at, and the original hook payload. Summarize the work into a clear, actionable briefing. Group by person or theme. Highlight blockers, completions, and key decisions. Be brief.",
        &format!("Summarize these {filter_desc}:\n\n{context}"),
    ).await;
    match result {
        Ok(text) if !text.is_empty() => {
            eprintln!(
                "[auto_summarize] success for room {room_id}, {} chars",
                text.len()
            );
            let _ = state.db.set_summary(room_id, &text);
            let _ = state.summary_events.send(SummaryStatusEvent {
                room_id: room_id.to_owned(),
                status: "ready".to_owned(),
            });
        }
        Ok(_) => {
            eprintln!("[auto_summarize] empty response for room {room_id}");
            let _ = state.db.set_summary_status(room_id, "error");
            let _ = state.summary_events.send(SummaryStatusEvent {
                room_id: room_id.to_owned(),
                status: "error".to_owned(),
            });
        }
        Err(error) => {
            eprintln!("[auto_summarize] error for room {room_id}: {error}");
            let _ = state.db.set_summary_status(room_id, "error");
            let _ = state.summary_events.send(SummaryStatusEvent {
                room_id: room_id.to_owned(),
                status: "error".to_owned(),
            });
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────

fn spawn_auto_summarize(state: &AppState, room_id: &str) {
    let bg_state = state.clone();
    let bg_room = room_id.to_owned();
    tokio::spawn(async move {
        auto_summarize(&bg_state, &bg_room).await;
    });
}

fn cli_join_command(api_url: &str, app_url: &str, room_id: &str, secret: &str) -> String {
    format!(
        "supermanager join --server \"{}\" --app-url \"{}\" --room \"{}\" --secret \"{}\"",
        trim_url(api_url),
        trim_url(app_url),
        room_id,
        secret,
    )
}

fn dashboard_url(app_url: &str, room_id: &str, secret: &str) -> String {
    format!("{}/r/{room_id}#secret={secret}", trim_url(app_url))
}

fn trim_url(url: &str) -> &str {
    url.trim_end_matches('/')
}

fn internal_error(error: anyhow::Error) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}

fn hook_event(event: &StoredHookEvent) -> Event {
    let data = serde_json::to_string(event).unwrap_or_else(|_| "{}".to_owned());
    Event::default()
        .event("hook_event")
        .id(event.event_id.to_string())
        .data(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_join_command_includes_app_url() {
        let command = cli_join_command(
            "https://api.example.com/",
            "https://app.example.com/",
            "bright-fox-1",
            "sm_sec_123",
        );

        assert_eq!(
            command,
            "supermanager join --server \"https://api.example.com\" --app-url \"https://app.example.com\" --room \"bright-fox-1\" --secret \"sm_sec_123\""
        );
    }

    #[test]
    fn dashboard_url_uses_secret_fragment() {
        let url = dashboard_url("https://app.example.com/", "bright-fox-1", "sm_sec_123");

        assert_eq!(url, "https://app.example.com/r/bright-fox-1#secret=sm_sec_123");
    }
}
