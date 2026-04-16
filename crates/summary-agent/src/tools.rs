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
struct SetEmployeeCardArgs {
    employee_name: String,
    markdown: String,
}

#[derive(Debug, Deserialize)]
struct RemoveEmployeeCardArgs {
    employee_name: String,
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
                "Read the current room snapshot before deciding what to edit.",
                empty(),
            ),
            spec(
                "set_bluf",
                "Replace the BLUF markdown for the room snapshot.",
                markdown_only(),
            ),
            spec(
                "set_overview",
                "Replace the detailed overview markdown for the room snapshot.",
                markdown_only(),
            ),
            spec(
                "set_employee_card",
                "Create or update a single employee card using concise markdown body content.",
                json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["employee_name", "markdown"],
                    "properties": {
                        "employee_name": { "type": "string" },
                        "markdown": { "type": "string" }
                    }
                }),
            ),
            spec(
                "remove_employee_card",
                "Remove an employee card that should no longer appear in the room snapshot.",
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
                let args: RemoveEmployeeCardArgs = serde_json::from_value(params.arguments.clone())
                    .context("invalid remove_employee_card arguments")?;
                Ok(Self::RemoveEmployeeCard {
                    employee_name: args.employee_name,
                })
            }
            other => anyhow::bail!("unknown tool: {other}"),
        }
    }

    pub(crate) fn into_wire(self) -> (String, Value) {
        match self {
            Self::GetSnapshot => ("get_snapshot".to_owned(), json!({})),
            Self::SetBluf { markdown } => ("set_bluf".to_owned(), json!({ "markdown": markdown })),
            Self::SetOverview { markdown } => {
                ("set_overview".to_owned(), json!({ "markdown": markdown }))
            }
            Self::SetEmployeeCard {
                employee_name,
                markdown,
            } => (
                "set_employee_card".to_owned(),
                json!({
                    "employee_name": employee_name,
                    "markdown": markdown,
                }),
            ),
            Self::RemoveEmployeeCard { employee_name } => (
                "remove_employee_card".to_owned(),
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
