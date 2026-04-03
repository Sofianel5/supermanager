use std::{
    collections::{HashMap, HashSet},
    convert::Infallible,
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, bail};
use async_stream::stream;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{
        IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
};
use reporter_protocol::{
    CreateRoomRequest, CreateRoomResponse, EmployeeSnapshot, FeedResponse, HookTurnReport,
    IngestResponse, Room, RoomMetadataResponse, RoomSnapshot, StoredHookEvent,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::broadcast;

use crate::store::Db;

const DEFAULT_PUBLIC_API_URL: &str = "https://supermanager.fly.dev";
const DEFAULT_PUBLIC_APP_URL: &str = "https://supermanager.dev";

#[derive(Debug, Default, Deserialize, JsonSchema)]
struct RoomSnapshotPatch {
    #[serde(default)]
    bluf_markdown: Option<String>,
    #[serde(default)]
    overview_markdown: Option<String>,
    #[serde(default)]
    employees: Vec<EmployeeSnapshotPatch>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
struct EmployeeSnapshotPatch {
    employee_name: String,
    #[serde(default)]
    content_markdown: String,
}

#[derive(Debug, Clone)]
struct EmployeeActivity {
    employee_name: String,
    latest_update_at: String,
    events: Vec<StoredHookEvent>,
}

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
    pub http: reqwest::Client,
    pub openai_api_key: Option<String>,
}

// ── Health ──────────────────────────────────────────────────

pub async fn health() -> &'static str {
    "ok"
}

// ── Room management ─────────────────────────────────────────

pub async fn create_room(
    State(state): State<AppState>,
    Json(req): Json<CreateRoomRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let room = state.db.create_room(&req.name).map_err(internal_error)?;
    let resp = CreateRoomResponse {
        dashboard_url: dashboard_url(&state.public_app_url, &room.room_id),
        join_command: cli_join_command(&state.public_api_url, &state.public_app_url, &room.room_id),
        room_id: room.room_id,
    };
    Ok((StatusCode::CREATED, Json(resp)))
}

// ── Room-scoped routes ──────────────────────────────────────

pub async fn ingest_hook_turn(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Json(report): Json<HookTurnReport>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let room = resolve_room(&state, &room_id)?;
    let room_id = room.room_id;

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
) -> Result<Json<RoomMetadataResponse>, (StatusCode, String)> {
    let room = resolve_room(&state, &room_id)?;
    Ok(Json(RoomMetadataResponse {
        room_id: room.room_id,
        name: room.name,
        created_at: room.created_at,
    }))
}

pub async fn get_feed(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<FeedResponse>, (StatusCode, String)> {
    let room = resolve_room(&state, &room_id)?;
    let room_id = room.room_id;
    let events = state.db.get_hook_events(&room_id).map_err(internal_error)?;
    Ok(Json(FeedResponse { events }))
}

pub async fn stream_feed(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)>
{
    let room = resolve_room(&state, &room_id)?;
    let room_id = room.room_id;

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
) -> Result<Json<RoomSnapshot>, (StatusCode, String)> {
    let room = resolve_room(&state, &room_id)?;
    let room_id = room.room_id;
    let summary = state.db.get_summary(&room_id).map_err(internal_error)?;
    Ok(Json(summary))
}

fn build_summary_patch_input(
    current_snapshot: &RoomSnapshot,
    changed_events: &[StoredHookEvent],
    active_events: &[StoredHookEvent],
) -> String {
    let employee_activity = collect_employee_activity(active_events);
    serde_json::to_string_pretty(&json!({
        "current_snapshot": current_snapshot,
        "new_updates": serialize_events(changed_events),
        "active_employees": employee_activity.iter().map(|activity| {
            json!({
                "employee_name": activity.employee_name,
                "last_update_at": activity.latest_update_at,
                "recent_update_count": activity.events.len(),
            })
        }).collect::<Vec<_>>(),
    }))
    .unwrap_or_default()
}

fn serialize_events(events: &[StoredHookEvent]) -> Vec<Value> {
    events
        .iter()
        .map(|event| {
            json!({
                "received_at": event.received_at,
                "employee_name": event.employee_name,
                "client": event.client,
                "repo_root": event.repo_root,
                "branch": event.branch,
                "payload": event.payload,
            })
        })
        .collect()
}

fn parse_snapshot_patch(raw: &str) -> anyhow::Result<RoomSnapshotPatch> {
    serde_json::from_str(raw.trim()).context("failed to parse room snapshot patch JSON")
}

fn merge_snapshot_patch(
    current_snapshot: RoomSnapshot,
    patch: RoomSnapshotPatch,
    active_events: &[StoredHookEvent],
) -> RoomSnapshot {
    let mut next_snapshot = current_snapshot;
    if let Some(bluf_markdown) = patch.bluf_markdown {
        next_snapshot.bluf_markdown = bluf_markdown;
    }
    if let Some(overview_markdown) = patch.overview_markdown {
        next_snapshot.overview_markdown = overview_markdown;
    }

    let current_order = next_snapshot
        .employees
        .iter()
        .map(|employee| normalize_employee_name(&employee.employee_name))
        .collect::<Vec<_>>();
    let mut current_cards = std::mem::take(&mut next_snapshot.employees)
        .into_iter()
        .map(|employee| (normalize_employee_name(&employee.employee_name), employee))
        .collect::<HashMap<_, _>>();
    let mut patch_cards = patch
        .employees
        .into_iter()
        .map(|employee| (normalize_employee_name(&employee.employee_name), employee))
        .collect::<HashMap<_, _>>();
    let active_employees = collect_employee_activity(active_events);
    let mut seen_employees = HashSet::new();

    next_snapshot.employees = active_employees
        .into_iter()
        .map(|activity| {
            let employee_key = normalize_employee_name(&activity.employee_name);
            let existing_card = current_cards.remove(&employee_key);
            let content_markdown = patch_cards
                .remove(&employee_key)
                .and_then(|card| non_empty_text(card.content_markdown))
                .or_else(|| {
                    existing_card
                        .as_ref()
                        .and_then(|card| non_empty_text(card.content_markdown.clone()))
                })
                .unwrap_or_else(|| build_employee_fallback(&activity));
            seen_employees.insert(employee_key);

            EmployeeSnapshot {
                employee_name: activity.employee_name,
                content_markdown,
                last_update_at: activity.latest_update_at,
            }
        })
        .collect::<Vec<_>>();

    next_snapshot
        .employees
        .extend(current_order.into_iter().filter_map(|employee_key| {
            if seen_employees.contains(&employee_key) {
                return None;
            }

            let existing_card = current_cards.remove(&employee_key)?;
            let content_markdown = patch_cards
                .remove(&employee_key)
                .and_then(|card| non_empty_text(card.content_markdown))
                .unwrap_or(existing_card.content_markdown);

            Some(EmployeeSnapshot {
                employee_name: existing_card.employee_name,
                content_markdown,
                last_update_at: existing_card.last_update_at,
            })
        }));

    next_snapshot
}

fn collect_employee_activity(events: &[StoredHookEvent]) -> Vec<EmployeeActivity> {
    let mut employees: Vec<EmployeeActivity> = Vec::new();
    let mut employee_indexes: HashMap<String, usize> = HashMap::new();

    for event in events {
        let employee_key = normalize_employee_name(&event.employee_name);
        if let Some(&index) = employee_indexes.get(&employee_key) {
            employees[index].events.push(event.clone());
            continue;
        }

        employee_indexes.insert(employee_key, employees.len());
        employees.push(EmployeeActivity {
            employee_name: event.employee_name.clone(),
            latest_update_at: event.received_at.clone(),
            events: vec![event.clone()],
        });
    }

    employees
}

fn build_employee_fallback(activity: &EmployeeActivity) -> String {
    let latest = match activity.events.first() {
        Some(event) => event,
        None => return "Recent activity landed in the feed for this employee.".to_owned(),
    };

    let mut first_line = format!("Recent activity arrived from `{}`", latest.repo_root);
    if let Some(branch) = latest
        .branch
        .as_deref()
        .filter(|branch| !branch.trim().is_empty())
    {
        first_line.push_str(&format!(" on `{branch}`"));
    }
    first_line.push_str(&format!(" via `{}`.", latest.client));

    format!("- {first_line}\n- Raw details are available in the live feed below.")
}

fn normalize_employee_name(value: &str) -> String {
    value.trim().to_lowercase()
}

fn non_empty_text(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}

fn is_snapshot_empty(snapshot: &RoomSnapshot) -> bool {
    snapshot.bluf_markdown.trim().is_empty()
        && snapshot.overview_markdown.trim().is_empty()
        && snapshot.employees.is_empty()
}

fn latest_snapshot_event_at(snapshot: &RoomSnapshot) -> Option<String> {
    snapshot
        .employees
        .iter()
        .map(|employee| employee.last_update_at.trim())
        .filter(|timestamp| !timestamp.is_empty())
        .max()
        .map(str::to_owned)
}

/// Build an OpenAI structured-output format block from a `schemars`-derived schema.
/// Adds `"additionalProperties": false` to every object node (required by strict mode).
fn openai_strict_schema<T: JsonSchema>(name: &str) -> Value {
    let root = schemars::schema_for!(T);
    let mut schema = serde_json::to_value(root).unwrap_or_default();
    enforce_additional_properties_false(&mut schema);
    json!({
        "type": "json_schema",
        "name": name,
        "strict": true,
        "schema": schema,
    })
}

fn enforce_additional_properties_false(value: &mut Value) {
    if let Some(obj) = value.as_object_mut() {
        if obj.contains_key("properties") {
            obj.insert("additionalProperties".to_owned(), Value::Bool(false));
        }
        for child in obj.values_mut() {
            enforce_additional_properties_false(child);
        }
    }
    if let Some(arr) = value.as_array_mut() {
        for child in arr {
            enforce_additional_properties_false(child);
        }
    }
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
        "text": {
            "format": openai_strict_schema::<RoomSnapshotPatch>("room_snapshot_patch"),
        },
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

fn broadcast_status(state: &AppState, room_id: &str, status: &str) {
    let _ = state.db.set_summary_status(room_id, status);
    let _ = state.summary_events.send(SummaryStatusEvent {
        room_id: room_id.to_owned(),
        status: status.to_owned(),
    });
}

fn fail_summarize(state: &AppState, room_id: &str, msg: impl std::fmt::Display) {
    eprintln!("[auto_summarize] {msg}");
    broadcast_status(state, room_id, "error");
}

/// Background auto-summarize: triggered after every new hook event.
async fn auto_summarize(state: &AppState, room_id: &str) {
    eprintln!("[auto_summarize] starting for room {room_id}");

    let current_snapshot = match state.db.get_summary(room_id) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            fail_summarize(
                state,
                room_id,
                format_args!("failed to load current snapshot for room {room_id}: {error}"),
            );
            return;
        }
    };
    broadcast_status(state, room_id, "generating");

    let active_events = match state
        .db
        .get_hook_events_filtered(room_id, None, None, None, 100)
    {
        Ok(events) if !events.is_empty() => events,
        Ok(_) => {
            eprintln!("[auto_summarize] no hook events available for room {room_id}");
            broadcast_status(state, room_id, "ready");
            return;
        }
        Err(error) => {
            fail_summarize(
                state,
                room_id,
                format_args!("failed to load active events for room {room_id}: {error}"),
            );
            return;
        }
    };

    let current_snapshot_event_at = latest_snapshot_event_at(&current_snapshot);
    let changed_events = if is_snapshot_empty(&current_snapshot) {
        active_events.clone()
    } else {
        match state.db.get_hook_events_filtered(
            room_id,
            current_snapshot_event_at.as_deref(),
            None,
            None,
            100,
        ) {
            Ok(events) if !events.is_empty() => events,
            Ok(_) => {
                eprintln!("[auto_summarize] no new events to merge for room {room_id}");
                broadcast_status(state, room_id, "ready");
                return;
            }
            Err(error) => {
                fail_summarize(
                    state,
                    room_id,
                    format_args!("failed to load changed events for room {room_id}: {error}"),
                );
                return;
            }
        }
    };

    eprintln!(
        "[auto_summarize] calling OpenAI with {} changed updates across {} active employees",
        changed_events.len(),
        collect_employee_activity(&active_events).len(),
    );

    let result = call_openai(
        state,
        "You maintain a structured room snapshot for a live engineering coordination room. Return only valid JSON with this exact shape: {\"bluf_markdown\": string | null, \"overview_markdown\": string | null, \"employees\": [{\"employee_name\": string, \"content_markdown\": string}]}. Omit a field or set it to null when that section should stay unchanged. Only include employee entries that need updates. If the current snapshot is empty, initialize the BLUF and detailed overview. Employee markdown should be concise body content only and should not repeat the employee name as a heading. Use only facts from the provided updates.",
        &build_summary_patch_input(&current_snapshot, &changed_events, &active_events),
    )
    .await;
    match result {
        Ok(text) if !text.is_empty() => {
            let next_snapshot = match parse_snapshot_patch(&text) {
                Ok(patch) => merge_snapshot_patch(current_snapshot, patch, &active_events),
                Err(error) => {
                    fail_summarize(
                        state,
                        room_id,
                        format_args!("invalid snapshot patch for room {room_id}: {error}"),
                    );
                    return;
                }
            };
            eprintln!(
                "[auto_summarize] success for room {room_id}, {} chars",
                text.len()
            );
            let _ = state.db.set_summary(room_id, &next_snapshot);
            broadcast_status(state, room_id, "ready");
        }
        Ok(_) => {
            fail_summarize(
                state,
                room_id,
                format_args!("empty response for room {room_id}"),
            );
        }
        Err(error) => {
            fail_summarize(
                state,
                room_id,
                format_args!("error for room {room_id}: {error}"),
            );
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

fn resolve_room(state: &AppState, room_id: &str) -> Result<Room, (StatusCode, String)> {
    state
        .db
        .get_room(room_id)
        .map_err(internal_error)?
        .ok_or((StatusCode::NOT_FOUND, format!("room not found: {room_id}")))
}

fn cli_join_command(api_url: &str, app_url: &str, room_id: &str) -> String {
    let api_url = trim_url(api_url);
    let app_url = trim_url(app_url);
    if api_url == DEFAULT_PUBLIC_API_URL && app_url == DEFAULT_PUBLIC_APP_URL {
        return format!("supermanager join {room_id}");
    }

    format!("supermanager join {room_id} --server \"{api_url}\" --app-url \"{app_url}\"")
}

fn dashboard_url(app_url: &str, room_id: &str) -> String {
    format!("{}/r/{room_id}", trim_url(app_url))
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

    use axum::{body::to_bytes, extract::State};
    use tempfile::TempDir;
    use uuid::Uuid;

    #[test]
    fn parse_snapshot_patch_accepts_valid_json() {
        let patch = parse_snapshot_patch(
            r#"{
                "bluf_markdown": "- top line",
                "employees": [
                    {
                        "employee_name": "Alice",
                        "content_markdown": "- Wrapped up the endpoint work."
                    }
                ]
            }"#,
        )
        .unwrap();

        assert_eq!(patch.bluf_markdown.as_deref(), Some("- top line"));
        assert!(patch.overview_markdown.is_none());
        assert_eq!(patch.employees.len(), 1);
        assert_eq!(patch.employees[0].employee_name, "Alice");
    }

    #[test]
    fn parse_snapshot_patch_rejects_fenced_output() {
        let error = parse_snapshot_patch("```json\n{\"bluf_markdown\":\"x\"}\n```").unwrap_err();

        assert!(
            error
                .to_string()
                .contains("failed to parse room snapshot patch JSON")
        );
    }

    #[test]
    fn merge_snapshot_patch_updates_only_changed_sections() {
        let current_snapshot = RoomSnapshot {
            bluf_markdown: "- Existing BLUF".to_owned(),
            overview_markdown: "Existing overview".to_owned(),
            employees: vec![
                EmployeeSnapshot {
                    employee_name: "Bob".to_owned(),
                    content_markdown: "- Still debugging".to_owned(),
                    last_update_at: "2026-04-01T09:00:00Z".to_owned(),
                },
                EmployeeSnapshot {
                    employee_name: "Dana".to_owned(),
                    content_markdown: "- Watching staging.".to_owned(),
                    last_update_at: "2026-03-31T17:00:00Z".to_owned(),
                },
            ],
        };
        let patch = RoomSnapshotPatch {
            bluf_markdown: Some("- New BLUF".to_owned()),
            overview_markdown: None,
            employees: vec![EmployeeSnapshotPatch {
                employee_name: "Alice".to_owned(),
                content_markdown: "- Shipped the API refactor.".to_owned(),
            }],
        };
        let active_events = vec![
            stored_event(
                "Alice",
                "2026-04-02T12:00:00Z",
                "repo-a",
                Some("feature/alice"),
            ),
            stored_event("Bob", "2026-04-02T11:00:00Z", "repo-b", Some("feature/bob")),
            stored_event("Carol", "2026-04-02T10:00:00Z", "repo-c", None),
        ];

        let merged = merge_snapshot_patch(current_snapshot, patch, &active_events);

        assert_eq!(merged.bluf_markdown, "- New BLUF");
        assert_eq!(merged.overview_markdown, "Existing overview");
        assert_eq!(
            merged
                .employees
                .iter()
                .map(|employee| employee.employee_name.as_str())
                .collect::<Vec<_>>(),
            vec!["Alice", "Bob", "Carol", "Dana"]
        );
        assert_eq!(
            merged.employees[0].content_markdown,
            "- Shipped the API refactor."
        );
        assert_eq!(merged.employees[0].last_update_at, "2026-04-02T12:00:00Z");
        assert_eq!(merged.employees[1].content_markdown, "- Still debugging");
        assert!(merged.employees[2].content_markdown.contains("Raw details"));
        assert_eq!(merged.employees[3].employee_name, "Dana");
        assert_eq!(merged.employees[3].content_markdown, "- Watching staging.");
        assert_eq!(merged.employees[3].last_update_at, "2026-03-31T17:00:00Z");
    }

    #[test]
    fn latest_snapshot_event_at_uses_employee_timestamps() {
        let snapshot = RoomSnapshot {
            bluf_markdown: String::new(),
            overview_markdown: String::new(),
            employees: vec![
                EmployeeSnapshot {
                    employee_name: "Alice".to_owned(),
                    content_markdown: "- Working".to_owned(),
                    last_update_at: "2026-04-02T12:00:00Z".to_owned(),
                },
                EmployeeSnapshot {
                    employee_name: "Bob".to_owned(),
                    content_markdown: "- Reviewing".to_owned(),
                    last_update_at: "2026-04-02T12:05:00Z".to_owned(),
                },
            ],
        };

        assert_eq!(
            latest_snapshot_event_at(&snapshot),
            Some("2026-04-02T12:05:00Z".to_owned())
        );
    }

    #[test]
    fn cli_join_command_includes_app_url() {
        let command = cli_join_command(
            "https://api.example.com/",
            "https://app.example.com/",
            "bright-fox-1",
        );

        assert_eq!(
            command,
            "supermanager join bright-fox-1 --server \"https://api.example.com\" --app-url \"https://app.example.com\""
        );
    }

    #[test]
    fn cli_join_command_uses_short_form_for_default_deployment() {
        let command = cli_join_command(
            "https://supermanager.fly.dev/",
            "https://supermanager.dev/",
            "ABC123",
        );

        assert_eq!(command, "supermanager join ABC123");
    }

    #[test]
    fn dashboard_url_is_room_path() {
        let url = dashboard_url("https://app.example.com/", "bright-fox-1");

        assert_eq!(url, "https://app.example.com/r/bright-fox-1");
    }

    #[tokio::test]
    async fn get_manager_summary_returns_json_payload() {
        let tempdir = TempDir::new().unwrap();
        let db = Arc::new(Db::open(&tempdir.path().join("api-summary.sqlite")).unwrap());
        let room = db.create_room("Summary Room").unwrap();
        let summary = RoomSnapshot {
            bluf_markdown: "- Top line".to_owned(),
            overview_markdown: "Detailed overview".to_owned(),
            employees: vec![EmployeeSnapshot {
                employee_name: "Alice".to_owned(),
                content_markdown: "- On track".to_owned(),
                last_update_at: "2026-04-02T12:00:00Z".to_owned(),
            }],
        };
        db.set_summary(&room.room_id, &summary).unwrap();

        let (hook_events, _) = broadcast::channel(8);
        let (summary_events, _) = broadcast::channel(8);
        let state = AppState {
            db,
            hook_events,
            summary_events,
            public_api_url: DEFAULT_PUBLIC_API_URL.to_owned(),
            public_app_url: DEFAULT_PUBLIC_APP_URL.to_owned(),
            http: reqwest::Client::new(),
            openai_api_key: None,
        };

        let response = get_manager_summary(State(state), Path(room.room_id.clone()))
            .await
            .unwrap()
            .into_response();

        assert_eq!(
            response
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok()),
            Some("application/json")
        );

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let returned_summary: RoomSnapshot = serde_json::from_slice(&body).unwrap();
        assert_eq!(returned_summary, summary);
    }

    fn stored_event(
        employee_name: &str,
        received_at: &str,
        repo_root: &str,
        branch: Option<&str>,
    ) -> StoredHookEvent {
        StoredHookEvent {
            event_id: Uuid::new_v4(),
            received_at: received_at.to_owned(),
            employee_name: employee_name.to_owned(),
            client: "codex".to_owned(),
            repo_root: repo_root.to_owned(),
            branch: branch.map(ToOwned::to_owned),
            payload: json!({
                "summary": format!("{employee_name} update"),
            }),
        }
    }
}
