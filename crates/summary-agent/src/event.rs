use anyhow::Result;
use reporter_protocol::StoredHookEvent;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct RegenerationRoom {
    pub(crate) room_id: String,
    pub(crate) name: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RegenerationEvent {
    pub(crate) room_id: String,
    pub(crate) room_name: String,
    #[serde(flatten)]
    pub(crate) event: StoredHookEvent,
}

pub(crate) fn format_event(
    room_id: &str,
    room_name: &str,
    event: &StoredHookEvent,
) -> Result<String> {
    let payload = serde_json::to_string_pretty(&event.payload)?;
    let branch = event
        .branch
        .as_deref()
        .filter(|branch| !branch.trim().is_empty())
        .unwrap_or("(none)");

    Ok(format!(
        "A new organization hook event arrived.\n\
room_id: {room_id}\n\
room_name: {room_name}\n\
employee_name: {employee_name}\n\
client: {client}\n\
repo_root: {repo_root}\n\
branch: {branch}\n\
received_at: {received_at}\n\
payload_json:\n{payload}",
        room_id = room_id,
        room_name = room_name,
        employee_name = event.employee_name,
        client = event.client,
        repo_root = event.repo_root,
        branch = branch,
        received_at = event.received_at,
        payload = payload,
    ))
}

pub(crate) fn format_regeneration_request(
    reason: &str,
    rooms: &[RegenerationRoom],
    events: &[RegenerationEvent],
) -> Result<String> {
    let rooms_text = if rooms.is_empty() {
        "(none)".to_owned()
    } else {
        rooms
            .iter()
            .map(|room| format!("- {}: {}", room.room_id, room.name))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let events_text = if events.is_empty() {
        "(none)".to_owned()
    } else {
        let mut rendered = Vec::with_capacity(events.len());
        for regeneration_event in events {
            rendered.push(format_event(
                &regeneration_event.room_id,
                &regeneration_event.room_name,
                &regeneration_event.event,
            )?);
        }
        rendered.join("\n\n---\n\n")
    };

    Ok(format!(
        "Regenerate the organization summary now.\n\
trigger: {reason}\n\
current_rooms:\n{rooms}\n\
recent_org_events:\n{events}\n\
Tighten the organization BLUF, room BLUFs, and employee BLUFs so they reflect the latest state.",
        reason = reason,
        rooms = rooms_text,
        events = events_text,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn format_event_includes_snapshot_fields() {
        let event = StoredHookEvent {
            seq: 0,
            event_id: Uuid::nil(),
            received_at: "2026-04-03T12:00:00Z".to_owned(),
            employee_name: "Dana".to_owned(),
            client: "codex".to_owned(),
            repo_root: "/tmp/repo".to_owned(),
            branch: Some("feature/agent".to_owned()),
            payload: json!({ "hook_event_name": "Stop" }),
        };

        let rendered = format_event("ROOM42", "Operations", &event).unwrap();

        assert!(rendered.contains("room_id: ROOM42"));
        assert!(rendered.contains("room_name: Operations"));
        assert!(rendered.contains("employee_name: Dana"));
        assert!(rendered.contains("branch: feature/agent"));
        assert!(rendered.contains("\"hook_event_name\": \"Stop\""));
    }

    #[test]
    fn format_regeneration_request_includes_rooms_and_events() {
        let event = StoredHookEvent {
            seq: 0,
            event_id: Uuid::nil(),
            received_at: "2026-04-03T12:00:00Z".to_owned(),
            employee_name: "Dana".to_owned(),
            client: "codex".to_owned(),
            repo_root: "/tmp/repo".to_owned(),
            branch: None,
            payload: json!({ "hook_event_name": "Stop" }),
        };

        let rendered = format_regeneration_request(
            "timer",
            &[RegenerationRoom {
                room_id: "ROOM42".to_owned(),
                name: "Operations".to_owned(),
            }],
            &[RegenerationEvent {
                room_id: "ROOM42".to_owned(),
                room_name: "Operations".to_owned(),
                event,
            }],
        )
        .unwrap();

        assert!(rendered.contains("trigger: timer"));
        assert!(rendered.contains("- ROOM42: Operations"));
        assert!(rendered.contains("room_name: Operations"));
    }
}
