use anyhow::Result;
use reporter_protocol::StoredHookEvent;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct OrganizationHeartbeatProject {
    pub(crate) project_id: String,
    pub(crate) name: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OrganizationHeartbeatEvent {
    pub(crate) project_id: String,
    pub(crate) project_name: String,
    #[serde(flatten)]
    pub(crate) event: StoredHookEvent,
}

pub(crate) fn format_project_event(
    project_id: &str,
    project_name: &str,
    event: &StoredHookEvent,
) -> Result<String> {
    let payload = serde_json::to_string_pretty(&event.payload)?;
    let branch = event
        .branch
        .as_deref()
        .filter(|branch| !branch.trim().is_empty())
        .unwrap_or("(none)");

    Ok(format!(
        "A new project hook event arrived.\n\
project_id: {project_id}\n\
project_name: {project_name}\n\
employee_user_id: {employee_user_id}\n\
employee_name: {employee_name}\n\
client: {client}\n\
repo_root: {repo_root}\n\
branch: {branch}\n\
received_at: {received_at}\n\
payload_json:\n{payload}",
        project_id = project_id,
        project_name = project_name,
        employee_user_id = event.employee_user_id,
        employee_name = event.employee_name,
        client = event.client,
        repo_root = event.repo_root,
        branch = branch,
        received_at = event.received_at,
        payload = payload,
    ))
}

pub(crate) fn format_organization_heartbeat_request(
    projects: &[OrganizationHeartbeatProject],
    events: &[OrganizationHeartbeatEvent],
) -> Result<String> {
    let projects_text = if projects.is_empty() {
        "(none)".to_owned()
    } else {
        projects
            .iter()
            .map(|project| format!("- {}: {}", project.project_id, project.name))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let events_text = if events.is_empty() {
        "(none)".to_owned()
    } else {
        let mut rendered = Vec::with_capacity(events.len());
        for heartbeat_event in events {
            rendered.push(format_project_event(
                &heartbeat_event.project_id,
                &heartbeat_event.project_name,
                &heartbeat_event.event,
            )?);
        }
        rendered.join("\n\n---\n\n")
    };

    Ok(format!(
        "Organization summary heartbeat fired.\n\
current_projects:\n{projects}\n\
org_events_since_previous_heartbeat:\n{events}\n\
Tighten the organization BLUF and employee BLUFs. Project BLUFs are maintained separately and should be treated as read-only context from get_snapshot.",
        projects = projects_text,
        events = events_text,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn format_project_event_includes_snapshot_fields() {
        let event = StoredHookEvent {
            seq: 0,
            event_id: Uuid::nil(),
            received_at: "2026-04-03T12:00:00Z".to_owned(),
            employee_user_id: "user_123".to_owned(),
            employee_name: "Dana".to_owned(),
            client: "codex".to_owned(),
            repo_root: "/tmp/repo".to_owned(),
            branch: Some("feature/agent".to_owned()),
            payload: json!({ "hook_event_name": "Stop" }),
        };

        let rendered = format_project_event("PROJECT42", "Operations", &event).unwrap();

        assert!(rendered.contains("project_id: PROJECT42"));
        assert!(rendered.contains("project_name: Operations"));
        assert!(rendered.contains("employee_user_id: user_123"));
        assert!(rendered.contains("employee_name: Dana"));
        assert!(rendered.contains("branch: feature/agent"));
        assert!(rendered.contains("\"hook_event_name\": \"Stop\""));
    }

    #[test]
    fn format_organization_heartbeat_request_includes_projects_and_events() {
        let event = StoredHookEvent {
            seq: 0,
            event_id: Uuid::nil(),
            received_at: "2026-04-03T12:00:00Z".to_owned(),
            employee_user_id: "user_123".to_owned(),
            employee_name: "Dana".to_owned(),
            client: "codex".to_owned(),
            repo_root: "/tmp/repo".to_owned(),
            branch: None,
            payload: json!({ "hook_event_name": "Stop" }),
        };

        let rendered = format_organization_heartbeat_request(
            &[OrganizationHeartbeatProject {
                project_id: "PROJECT42".to_owned(),
                name: "Operations".to_owned(),
            }],
            &[OrganizationHeartbeatEvent {
                project_id: "PROJECT42".to_owned(),
                project_name: "Operations".to_owned(),
                event,
            }],
        )
        .unwrap();

        assert!(rendered.contains("Organization summary heartbeat fired."));
        assert!(rendered.contains("- PROJECT42: Operations"));
        assert!(rendered.contains("project_name: Operations"));
        assert!(rendered.contains("Project BLUFs are maintained separately"));
    }
}
