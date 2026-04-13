use anyhow::{Context, Result};
use codex_app_server_protocol::{
    DynamicToolCallOutputContentItem, DynamicToolCallParams, DynamicToolCallResponse,
    DynamicToolSpec,
};
use reporter_protocol::{EmployeeSnapshot, RoomSnapshot, StoredHookEvent};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::store::Db;
use crate::util::now_rfc3339;

#[derive(Deserialize)]
pub(crate) struct SetMarkdownArgs {
    pub(crate) markdown: String,
}

#[derive(Deserialize)]
pub(crate) struct SetEmployeeCardArgs {
    pub(crate) employee_name: String,
    pub(crate) markdown: String,
}

#[derive(Deserialize)]
pub(crate) struct RemoveEmployeeCardArgs {
    pub(crate) employee_name: String,
}

pub(crate) enum SummaryTool {
    GetSnapshot,
    SetBluf {
        markdown: String,
    },
    SetOverview {
        markdown: String,
    },
    SetEmployeeCard {
        employee_name: String,
        markdown: String,
    },
    RemoveEmployeeCard {
        employee_name: String,
    },
}

impl SummaryTool {
    pub(crate) fn specs() -> Vec<DynamicToolSpec> {
        let s = |name: &str, description: &str, schema: Value| DynamicToolSpec {
            name: name.to_owned(),
            description: description.to_owned(),
            input_schema: schema,
            defer_loading: false,
        };
        let empty = || json!({ "type": "object", "additionalProperties": false, "properties": {} });
        let markdown_only = || {
            json!({
                "type": "object", "additionalProperties": false,
                "required": ["markdown"],
                "properties": { "markdown": { "type": "string" } }
            })
        };

        vec![
            s(
                "get_snapshot",
                "Read the current room snapshot before deciding what to edit.",
                empty(),
            ),
            s(
                "set_bluf",
                "Replace the BLUF markdown for the room snapshot.",
                markdown_only(),
            ),
            s(
                "set_overview",
                "Replace the detailed overview markdown for the room snapshot.",
                markdown_only(),
            ),
            s(
                "set_employee_card",
                "Create or update a single employee card using concise markdown body content.",
                json!({
                    "type": "object", "additionalProperties": false,
                    "required": ["employee_name", "markdown"],
                    "properties": { "employee_name": { "type": "string" }, "markdown": { "type": "string" } }
                }),
            ),
            s(
                "remove_employee_card",
                "Remove an employee card that should no longer appear in the room snapshot.",
                json!({
                    "type": "object", "additionalProperties": false,
                    "required": ["employee_name"],
                    "properties": { "employee_name": { "type": "string" } }
                }),
            ),
        ]
    }

    pub(crate) fn parse(params: &DynamicToolCallParams) -> Result<Self> {
        match params.tool.as_str() {
            "get_snapshot" => Ok(Self::GetSnapshot),
            "set_bluf" => {
                let args: SetMarkdownArgs = serde_json::from_value(params.arguments.clone())
                    .context("invalid set_bluf arguments")?;
                Ok(Self::SetBluf {
                    markdown: args.markdown,
                })
            }
            "set_overview" => {
                let args: SetMarkdownArgs = serde_json::from_value(params.arguments.clone())
                    .context("invalid set_overview arguments")?;
                Ok(Self::SetOverview {
                    markdown: args.markdown,
                })
            }
            "set_employee_card" => {
                let args: SetEmployeeCardArgs = serde_json::from_value(params.arguments.clone())
                    .context("invalid set_employee_card arguments")?;
                Ok(Self::SetEmployeeCard {
                    employee_name: args.employee_name,
                    markdown: args.markdown,
                })
            }
            "remove_employee_card" => {
                let args: RemoveEmployeeCardArgs =
                    serde_json::from_value(params.arguments.clone())
                        .context("invalid remove_employee_card arguments")?;
                Ok(Self::RemoveEmployeeCard {
                    employee_name: args.employee_name,
                })
            }
            other => anyhow::bail!("unknown tool: {other}"),
        }
    }

    pub(crate) async fn execute(self, db: &Db, room_id: &str) -> Result<DynamicToolCallResponse> {
        match self {
            Self::GetSnapshot => {
                let snapshot = db.get_summary(room_id).await?;
                Ok(tool_success(serde_json::to_string_pretty(&snapshot)?))
            }
            Self::SetBluf { markdown } => {
                let markdown = markdown.trim().to_owned();
                mutate_summary(db, room_id, move |snapshot| {
                    snapshot.bluf_markdown = markdown;
                    Ok((true, tool_success("updated BLUF")))
                })
                .await
            }
            Self::SetOverview { markdown } => {
                let markdown = markdown.trim().to_owned();
                mutate_summary(db, room_id, move |snapshot| {
                    snapshot.overview_markdown = markdown;
                    Ok((true, tool_success("updated overview")))
                })
                .await
            }
            Self::SetEmployeeCard {
                employee_name,
                markdown,
            } => {
                let employee_name = employee_name.trim().to_owned();
                if employee_name.is_empty() {
                    anyhow::bail!("employee_name must not be empty");
                }

                let employee_key = normalize_employee_name(&employee_name);
                let content_markdown = markdown.trim().to_owned();
                let updated_at = now_rfc3339();

                mutate_summary(db, room_id, move |snapshot| {
                    if let Some(existing) = snapshot.employees.iter_mut().find(|employee| {
                        normalize_employee_name(&employee.employee_name) == employee_key
                    }) {
                        existing.employee_name = employee_name.clone();
                        existing.content_markdown = content_markdown.clone();
                        existing.last_update_at = updated_at.clone();
                    } else {
                        snapshot.employees.push(EmployeeSnapshot {
                            employee_name: employee_name.clone(),
                            content_markdown: content_markdown.clone(),
                            last_update_at: updated_at.clone(),
                        });
                    }

                    Ok((
                        true,
                        tool_success(format!("updated employee card for {employee_name}")),
                    ))
                })
                .await
            }
            Self::RemoveEmployeeCard { employee_name } => {
                let employee_name = employee_name.trim().to_owned();
                let employee_key = normalize_employee_name(&employee_name);

                mutate_summary(db, room_id, move |snapshot| {
                    let before_len = snapshot.employees.len();
                    snapshot.employees.retain(|employee| {
                        normalize_employee_name(&employee.employee_name) != employee_key
                    });

                    let changed = snapshot.employees.len() != before_len;
                    let message = if changed {
                        format!("removed employee card for {employee_name}")
                    } else {
                        format!("employee card already absent for {employee_name}")
                    };
                    Ok((changed, tool_success(message)))
                })
                .await
            }
        }
    }
}

pub(crate) async fn mutate_summary<T>(
    db: &Db,
    room_id: &str,
    mutate: impl FnOnce(&mut RoomSnapshot) -> Result<(bool, T)>,
) -> Result<T> {
    let mut snapshot = db.get_summary(room_id).await?;
    let (changed, output) = mutate(&mut snapshot)?;
    if changed {
        db.set_summary(room_id, &snapshot).await?;
    }

    Ok(output)
}

pub(crate) fn normalize_employee_name(value: &str) -> String {
    value
        .split_whitespace()
        .map(|part| part.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn format_event(event: &StoredHookEvent) -> Result<String> {
    let payload = serde_json::to_string_pretty(&event.payload)?;
    let branch = event
        .branch
        .as_deref()
        .filter(|branch| !branch.trim().is_empty())
        .unwrap_or("(none)");

    Ok(format!(
        "A new room hook event arrived.\n\
employee_name: {employee_name}\n\
client: {client}\n\
repo_root: {repo_root}\n\
branch: {branch}\n\
received_at: {received_at}\n\
payload_json:\n{payload}",
        employee_name = event.employee_name,
        client = event.client,
        repo_root = event.repo_root,
        branch = branch,
        received_at = event.received_at,
        payload = payload,
    ))
}

pub(crate) fn tool_success(message: impl Into<String>) -> DynamicToolCallResponse {
    DynamicToolCallResponse {
        content_items: vec![DynamicToolCallOutputContentItem::InputText {
            text: message.into(),
        }],
        success: true,
    }
}

pub(crate) fn tool_failure(message: impl Into<String>) -> DynamicToolCallResponse {
    DynamicToolCallResponse {
        content_items: vec![DynamicToolCallOutputContentItem::InputText {
            text: message.into(),
        }],
        success: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::test_support::TestDb;
    use reporter_protocol::HookTurnReport;

    #[tokio::test]
    async fn set_employee_card_uses_latest_employee_timestamp() {
        let Some(test_db) = TestDb::new().await else {
            eprintln!("skipping PostgreSQL test: TEST_DATABASE_URL is not set");
            return;
        };
        let db = test_db.db.clone();
        let room = db
            .create_room("Summary Room", "org_test", "user_test")
            .await
            .unwrap();

        db.insert_hook_event(
            &room.room_id,
            &HookTurnReport {
                employee_name: "Alice Example".to_owned(),
                client: "codex".to_owned(),
                repo_root: "/tmp/repo".to_owned(),
                branch: Some("main".to_owned()),
                payload: json!({ "last_assistant_message": "First update" }),
            },
        )
        .await
        .unwrap();

        let response = SummaryTool::SetEmployeeCard {
            employee_name: "Alice Example".to_owned(),
            markdown: "- Shipped the agent runtime.".to_owned(),
        }
        .execute(&db, &room.room_id)
        .await
        .unwrap();

        assert!(response.success);

        let snapshot = db.get_summary(&room.room_id).await.unwrap();
        assert_eq!(snapshot.employees.len(), 1);
        assert_eq!(snapshot.employees[0].employee_name, "Alice Example");
        assert_eq!(
            snapshot.employees[0].content_markdown,
            "- Shipped the agent runtime."
        );
        assert!(!snapshot.employees[0].last_update_at.is_empty());

        test_db.cleanup().await;
    }

    #[test]
    fn format_event_includes_snapshot_fields() {
        let event = StoredHookEvent {
            seq: 0,
            event_id: uuid::Uuid::nil(),
            received_at: "2026-04-03T12:00:00Z".to_owned(),
            employee_name: "Dana".to_owned(),
            client: "codex".to_owned(),
            repo_root: "/tmp/repo".to_owned(),
            branch: Some("feature/agent".to_owned()),
            payload: json!({ "hook_event_name": "Stop" }),
        };

        let rendered = format_event(&event).unwrap();

        assert!(rendered.contains("employee_name: Dana"));
        assert!(rendered.contains("branch: feature/agent"));
        assert!(rendered.contains("\"hook_event_name\": \"Stop\""));
    }
}
