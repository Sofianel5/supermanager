use anyhow::{Context, Result};
use codex_app_server_protocol::{
    DynamicToolCallOutputContentItem, DynamicToolCallParams, DynamicToolCallResponse,
    DynamicToolSpec,
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
struct SetMarkdownArgs {
    markdown: String,
}

#[derive(Debug, Deserialize)]
struct SetRoomBlufArgs {
    room_id: String,
    markdown: String,
}

#[derive(Debug, Deserialize)]
struct SetEmployeeBlufArgs {
    employee_name: String,
    room_ids: Vec<String>,
    markdown: String,
}

#[derive(Debug, Deserialize)]
struct RemoveEmployeeBlufArgs {
    employee_name: String,
}

#[derive(Debug, Deserialize)]
struct RemoveRoomBlufArgs {
    room_id: String,
}

pub(crate) enum SummaryTool {
    GetSnapshot,
    SetOrgBluf {
        markdown: String,
    },
    SetRoomBluf {
        room_id: String,
        markdown: String,
    },
    RemoveRoomBluf {
        room_id: String,
    },
    SetEmployeeBluf {
        employee_name: String,
        room_ids: Vec<String>,
        markdown: String,
    },
    RemoveEmployeeBluf {
        employee_name: String,
    },
}

impl SummaryTool {
    pub(crate) fn specs() -> Vec<DynamicToolSpec> {
        let spec = |name: &str, description: &str, schema: Value| DynamicToolSpec {
            name: name.to_owned(),
            description: description.to_owned(),
            input_schema: schema,
            defer_loading: false,
        };
        let empty = || json!({ "type": "object", "additionalProperties": false, "properties": {} });
        let markdown_only = || {
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["markdown"],
                "properties": { "markdown": { "type": "string" } }
            })
        };

        vec![
            spec(
                "get_snapshot",
                "Read the current organization snapshot before deciding what to edit.",
                empty(),
            ),
            spec(
                "set_org_bluf",
                "Replace the organization BLUF markdown.",
                markdown_only(),
            ),
            spec(
                "set_room_bluf",
                "Create or update a room BLUF using concise markdown body content.",
                json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["room_id", "markdown"],
                    "properties": {
                        "room_id": { "type": "string" },
                        "markdown": { "type": "string" }
                    }
                }),
            ),
            spec(
                "remove_room_bluf",
                "Remove a room BLUF that should no longer appear in the organization snapshot.",
                json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["room_id"],
                    "properties": { "room_id": { "type": "string" } }
                }),
            ),
            spec(
                "set_employee_bluf",
                "Create or update a single employee BLUF using concise markdown body content.",
                json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["employee_name", "room_ids", "markdown"],
                    "properties": {
                        "employee_name": { "type": "string" },
                        "room_ids": {
                            "type": "array",
                            "items": { "type": "string" }
                        },
                        "markdown": { "type": "string" }
                    }
                }),
            ),
            spec(
                "remove_employee_bluf",
                "Remove an employee BLUF that should no longer appear in the organization snapshot.",
                json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["employee_name"],
                    "properties": { "employee_name": { "type": "string" } }
                }),
            ),
        ]
    }

    pub(crate) fn parse(params: &DynamicToolCallParams) -> Result<Self> {
        match params.tool.as_str() {
            "get_snapshot" => Ok(Self::GetSnapshot),
            "set_org_bluf" => {
                let args: SetMarkdownArgs = serde_json::from_value(params.arguments.clone())
                    .context("invalid set_org_bluf arguments")?;
                Ok(Self::SetOrgBluf {
                    markdown: args.markdown,
                })
            }
            "set_room_bluf" => {
                let args: SetRoomBlufArgs = serde_json::from_value(params.arguments.clone())
                    .context("invalid set_room_bluf arguments")?;
                Ok(Self::SetRoomBluf {
                    room_id: args.room_id,
                    markdown: args.markdown,
                })
            }
            "remove_room_bluf" => {
                let args: RemoveRoomBlufArgs = serde_json::from_value(params.arguments.clone())
                    .context("invalid remove_room_bluf arguments")?;
                Ok(Self::RemoveRoomBluf {
                    room_id: args.room_id,
                })
            }
            "set_employee_bluf" => {
                let args: SetEmployeeBlufArgs = serde_json::from_value(params.arguments.clone())
                    .context("invalid set_employee_bluf arguments")?;
                Ok(Self::SetEmployeeBluf {
                    employee_name: args.employee_name,
                    room_ids: args.room_ids,
                    markdown: args.markdown,
                })
            }
            "remove_employee_bluf" => {
                let args: RemoveEmployeeBlufArgs = serde_json::from_value(params.arguments.clone())
                    .context("invalid remove_employee_bluf arguments")?;
                Ok(Self::RemoveEmployeeBluf {
                    employee_name: args.employee_name,
                })
            }
            other => anyhow::bail!("unknown tool: {other}"),
        }
    }

    pub(crate) fn into_wire(self) -> (String, Value) {
        match self {
            Self::GetSnapshot => ("get_snapshot".to_owned(), json!({})),
            Self::SetOrgBluf { markdown } => {
                ("set_org_bluf".to_owned(), json!({ "markdown": markdown }))
            }
            Self::SetRoomBluf { room_id, markdown } => (
                "set_room_bluf".to_owned(),
                json!({
                    "room_id": room_id,
                    "markdown": markdown,
                }),
            ),
            Self::RemoveRoomBluf { room_id } => {
                ("remove_room_bluf".to_owned(), json!({ "room_id": room_id }))
            }
            Self::SetEmployeeBluf {
                employee_name,
                room_ids,
                markdown,
            } => (
                "set_employee_bluf".to_owned(),
                json!({
                    "employee_name": employee_name,
                    "room_ids": room_ids,
                    "markdown": markdown,
                }),
            ),
            Self::RemoveEmployeeBluf { employee_name } => (
                "remove_employee_bluf".to_owned(),
                json!({ "employee_name": employee_name }),
            ),
        }
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
