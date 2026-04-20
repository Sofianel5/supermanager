use anyhow::Result;
use reporter_protocol::StoredHookEvent;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct OrganizationProject {
    pub(crate) project_id: String,
    pub(crate) name: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OrganizationEvent {
    pub(crate) project_id: String,
    pub(crate) project_name: String,
    #[serde(flatten)]
    pub(crate) event: StoredHookEvent,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OrganizationTranscript {
    pub(crate) session_id: String,
    pub(crate) project_id: String,
    pub(crate) project_name: String,
    pub(crate) member_user_id: String,
    pub(crate) member_name: String,
    pub(crate) client: String,
    pub(crate) repo_root: String,
    pub(crate) branch: Option<String>,
    pub(crate) received_at: String,
    pub(crate) transcript_path: String,
    pub(crate) transcript_text: String,
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
member_user_id: {member_user_id}\n\
member_name: {member_name}\n\
client: {client}\n\
repo_root: {repo_root}\n\
branch: {branch}\n\
received_at: {received_at}\n\
payload_json:\n{payload}",
        project_id = project_id,
        project_name = project_name,
        member_user_id = event.member_user_id,
        member_name = event.member_name,
        client = event.client,
        repo_root = event.repo_root,
        branch = branch,
        received_at = event.received_at,
        payload = payload,
    ))
}

pub(crate) fn format_organization_summary_request(
    projects: &[OrganizationProject],
    events: &[OrganizationEvent],
) -> Result<String> {
    let projects_text = format_projects(projects);
    let events_text = if events.is_empty() {
        "(none)".to_owned()
    } else {
        let mut rendered = Vec::with_capacity(events.len());
        for event in events {
            rendered.push(format_project_event(
                &event.project_id,
                &event.project_name,
                &event.event,
            )?);
        }
        rendered.join("\n\n---\n\n")
    };

    Ok(format!(
        "Organization summary heartbeat fired.\n\
current_projects:\n{projects}\n\
org_events_since_previous_heartbeat:\n{events}\n\
Tighten the organization BLUF and member BLUFs. Project BLUFs are maintained separately and should be treated as read-only context from get_snapshot.",
        projects = projects_text,
        events = events_text,
    ))
}

pub(crate) fn format_organization_memory_request(
    projects: &[OrganizationProject],
    transcripts: &[OrganizationTranscript],
) -> Result<String> {
    format_organization_transcript_request(
        "Organization memory heartbeat fired.",
        projects,
        transcripts,
        "Update the durable organization memory files under ./memories/ only when the new transcript evidence supports a real reusable change.",
    )
}

pub(crate) fn format_organization_skills_request(
    projects: &[OrganizationProject],
    transcripts: &[OrganizationTranscript],
) -> Result<String> {
    format_organization_transcript_request(
        "Organization skill maintenance heartbeat fired.",
        projects,
        transcripts,
        "Update the reusable organization skills under ./.codex/skills/ only when the new transcript evidence supports a real reusable skill change.",
    )
}

fn format_organization_transcript_request(
    title: &str,
    projects: &[OrganizationProject],
    transcripts: &[OrganizationTranscript],
    instruction: &str,
) -> Result<String> {
    let projects_text = format_projects(projects);
    let transcripts_text = if transcripts.is_empty() {
        "(none)".to_owned()
    } else {
        let mut rendered = Vec::with_capacity(transcripts.len());
        for transcript in transcripts {
            rendered.push(format_organization_transcript(transcript)?);
        }
        rendered.join("\n\n---\n\n")
    };

    Ok(format!(
        "{title}\n\
current_projects:\n{projects}\n\
org_transcripts_since_previous_heartbeat:\n{transcripts}\n\
{instruction}",
        projects = projects_text,
        transcripts = transcripts_text,
    ))
}

fn format_projects(projects: &[OrganizationProject]) -> String {
    if projects.is_empty() {
        "(none)".to_owned()
    } else {
        projects
            .iter()
            .map(|project| format!("- {}: {}", project.project_id, project.name))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn format_organization_transcript(transcript: &OrganizationTranscript) -> Result<String> {
    let branch = transcript
        .branch
        .as_deref()
        .filter(|branch| !branch.trim().is_empty())
        .unwrap_or("(none)");

    Ok(format!(
        "session_id: {session_id}\n\
project_id: {project_id}\n\
project_name: {project_name}\n\
member_user_id: {member_user_id}\n\
member_name: {member_name}\n\
client: {client}\n\
repo_root: {repo_root}\n\
branch: {branch}\n\
received_at: {received_at}\n\
transcript_path: {transcript_path}\n\
transcript_text:\n{transcript_text}",
        session_id = transcript.session_id,
        project_id = transcript.project_id,
        project_name = transcript.project_name,
        member_user_id = transcript.member_user_id,
        member_name = transcript.member_name,
        client = transcript.client,
        repo_root = transcript.repo_root,
        branch = branch,
        received_at = transcript.received_at,
        transcript_path = transcript.transcript_path,
        transcript_text = transcript.transcript_text,
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
            member_user_id: "user_123".to_owned(),
            member_name: "Dana".to_owned(),
            client: "codex".to_owned(),
            repo_root: "/tmp/repo".to_owned(),
            branch: Some("feature/agent".to_owned()),
            payload: json!({ "hook_event_name": "Stop" }),
        };

        let rendered = format_project_event("PROJECT42", "Operations", &event).unwrap();

        assert!(rendered.contains("project_id: PROJECT42"));
        assert!(rendered.contains("project_name: Operations"));
        assert!(rendered.contains("member_user_id: user_123"));
        assert!(rendered.contains("member_name: Dana"));
        assert!(rendered.contains("branch: feature/agent"));
        assert!(rendered.contains("\"hook_event_name\": \"Stop\""));
    }

    #[test]
    fn format_organization_summary_request_includes_projects_and_events() {
        let event = StoredHookEvent {
            seq: 0,
            event_id: Uuid::nil(),
            received_at: "2026-04-03T12:00:00Z".to_owned(),
            member_user_id: "user_123".to_owned(),
            member_name: "Dana".to_owned(),
            client: "codex".to_owned(),
            repo_root: "/tmp/repo".to_owned(),
            branch: None,
            payload: json!({ "hook_event_name": "Stop" }),
        };

        let rendered = format_organization_summary_request(
            &[OrganizationProject {
                project_id: "PROJECT42".to_owned(),
                name: "Operations".to_owned(),
            }],
            &[OrganizationEvent {
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

    #[test]
    fn format_organization_memory_request_includes_transcript_text() {
        let rendered = format_organization_memory_request(
            &[OrganizationProject {
                project_id: "PROJECT42".to_owned(),
                name: "Operations".to_owned(),
            }],
            &[OrganizationTranscript {
                session_id: "sess_123".to_owned(),
                project_id: "PROJECT42".to_owned(),
                project_name: "Operations".to_owned(),
                member_user_id: "user_123".to_owned(),
                member_name: "Dana".to_owned(),
                client: "codex".to_owned(),
                repo_root: "/tmp/repo".to_owned(),
                branch: None,
                received_at: "2026-04-03T12:00:00Z".to_owned(),
                transcript_path: "/tmp/transcript.jsonl".to_owned(),
                transcript_text: "user: ship it\nassistant: done".to_owned(),
            }],
        )
        .unwrap();

        assert!(rendered.contains("Organization memory heartbeat fired."));
        assert!(rendered.contains("session_id: sess_123"));
        assert!(rendered.contains("transcript_path: /tmp/transcript.jsonl"));
        assert!(rendered.contains("assistant: done"));
    }
}
