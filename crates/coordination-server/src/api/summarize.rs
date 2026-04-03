use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

use anyhow::{Context, bail};
use reporter_protocol::{EmployeeSnapshot, RoomSnapshot, StoredHookEvent};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use time::format_description::well_known::Rfc3339;

use super::AppState;

// ── Types ──────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SummaryStatus {
    Generating,
    Ready,
    Error,
}

impl SummaryStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Generating => "generating",
            Self::Ready => "ready",
            Self::Error => "error",
        }
    }
}

impl std::fmt::Display for SummaryStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for SummaryStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "generating" => Ok(Self::Generating),
            "ready" => Ok(Self::Ready),
            "error" => Ok(Self::Error),
            other => Err(format!("unknown summary status: {other}")),
        }
    }
}

#[derive(Clone)]
pub struct SummaryStatusEvent {
    pub room_id: String,
    pub status: SummaryStatus,
}

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

// ── Public entry point ─────────────────────────────────────

pub fn spawn_auto_summarize(state: &AppState, room_id: &str) {
    let bg_state = state.clone();
    let bg_room = room_id.to_owned();
    tokio::spawn(async move {
        auto_summarize(&bg_state, &bg_room).await;
    });
}

pub fn broadcast_status(state: &AppState, room_id: &str, status: SummaryStatus) {
    let _ = state.db.set_summary_status(room_id, status.as_str());
    let _ = state.summary_events.send(SummaryStatusEvent {
        room_id: room_id.to_owned(),
        status,
    });
}

// ── Core pipeline ──────────────────────────────────────────

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
    broadcast_status(state, room_id, SummaryStatus::Generating);

    let active_events = match state
        .db
        .get_hook_events_filtered(room_id, None, None, None, 100)
    {
        Ok(events) if !events.is_empty() => events,
        Ok(_) => {
            eprintln!("[auto_summarize] no hook events available for room {room_id}");
            broadcast_status(state, room_id, SummaryStatus::Ready);
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

    let snapshot_cursor = latest_snapshot_event_at(&current_snapshot);
    let changed_events: &[StoredHookEvent] = if is_snapshot_empty(&current_snapshot) {
        &active_events
    } else if let Some(cursor) = &snapshot_cursor {
        let boundary = active_events
            .iter()
            .position(|event| {
                time::OffsetDateTime::parse(&event.received_at, &Rfc3339)
                    .map_or(false, |t| t <= *cursor)
            })
            .unwrap_or(active_events.len());
        if boundary == 0 {
            eprintln!("[auto_summarize] no new events to merge for room {room_id}");
            broadcast_status(state, room_id, SummaryStatus::Ready);
            return;
        }
        &active_events[..boundary]
    } else {
        &active_events
    };

    let employee_activity = collect_employee_activity(&active_events);

    eprintln!(
        "[auto_summarize] calling OpenAI with {} changed updates across {} active employees",
        changed_events.len(),
        employee_activity.len(),
    );

    let result = call_openai(
        state,
        "You maintain a structured room snapshot for a live engineering coordination room. Return only valid JSON with this exact shape: {\"bluf_markdown\": string | null, \"overview_markdown\": string | null, \"employees\": [{\"employee_name\": string, \"content_markdown\": string}]}. Omit a field or set it to null when that section should stay unchanged. Only include employee entries that need updates. If the current snapshot is empty, initialize the BLUF and detailed overview. Employee markdown should be concise body content only and should not repeat the employee name as a heading. Use only facts from the provided updates.",
        &build_summary_patch_input(&current_snapshot, changed_events, &employee_activity),
    )
    .await;
    match result {
        Ok(text) if !text.is_empty() => {
            let next_snapshot = match parse_snapshot_patch(&text) {
                Ok(patch) => merge_snapshot_patch(current_snapshot, patch, employee_activity),
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
                "[auto_summarize] success for room {room_id}, {} employee cards",
                next_snapshot.employees.len()
            );
            let _ = state.db.set_summary(room_id, &next_snapshot);
            broadcast_status(state, room_id, SummaryStatus::Ready);
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

fn fail_summarize(state: &AppState, room_id: &str, msg: impl std::fmt::Display) {
    eprintln!("[auto_summarize] {msg}");
    broadcast_status(state, room_id, SummaryStatus::Error);
}

// ── OpenAI ─────────────────────────────────────────────────

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
            "format": snapshot_patch_schema(),
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

// ── Schema ─────────────────────────────────────────────────

fn snapshot_patch_schema() -> &'static Value {
    static SCHEMA: OnceLock<Value> = OnceLock::new();
    SCHEMA.get_or_init(|| {
        let root = schemars::schema_for!(RoomSnapshotPatch);
        let mut schema = serde_json::to_value(root).unwrap_or_default();
        enforce_additional_properties_false(&mut schema);
        json!({
            "type": "json_schema",
            "name": "room_snapshot_patch",
            "strict": true,
            "schema": schema,
        })
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

// ── Snapshot patching ──────────────────────────────────────

fn build_summary_patch_input(
    current_snapshot: &RoomSnapshot,
    changed_events: &[StoredHookEvent],
    employee_activity: &[EmployeeActivity],
) -> String {
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
    active_employees: Vec<EmployeeActivity>,
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
    let mut seen_employees = HashSet::new();

    next_snapshot.employees = active_employees
        .into_iter()
        .map(|activity| {
            let employee_key = normalize_employee_name(&activity.employee_name);
            let existing_card = current_cards.remove(&employee_key);
            let content_markdown = patch_cards
                .remove(&employee_key)
                .and_then(|card| non_empty_text(&card.content_markdown))
                .or_else(|| {
                    existing_card
                        .as_ref()
                        .and_then(|card| non_empty_text(&card.content_markdown))
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
                .and_then(|card| non_empty_text(&card.content_markdown))
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

fn non_empty_text(value: &str) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

fn is_snapshot_empty(snapshot: &RoomSnapshot) -> bool {
    snapshot.bluf_markdown.trim().is_empty()
        && snapshot.overview_markdown.trim().is_empty()
        && snapshot.employees.is_empty()
}

fn latest_snapshot_event_at(snapshot: &RoomSnapshot) -> Option<time::OffsetDateTime> {
    snapshot
        .employees
        .iter()
        .filter_map(|employee| {
            time::OffsetDateTime::parse(employee.last_update_at.trim(), &Rfc3339).ok()
        })
        .max()
}

// ── Tests ──────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use reporter_protocol::StoredHookEvent;
    use serde_json::json;
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
        let employee_activity = collect_employee_activity(&active_events);

        let merged = merge_snapshot_patch(current_snapshot, patch, employee_activity);

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

        let result = latest_snapshot_event_at(&snapshot).unwrap();
        let expected =
            time::OffsetDateTime::parse("2026-04-02T12:05:00Z", &Rfc3339).unwrap();
        assert_eq!(result, expected);
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
