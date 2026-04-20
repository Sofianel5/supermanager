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
struct SetEmployeeBlufArgs {
    employee_user_id: String,
    employee_name: String,
    project_ids: Vec<String>,
    markdown: String,
}

#[derive(Debug, Deserialize)]
struct SetProjectEmployeeBlufArgs {
    employee_user_id: String,
    employee_name: String,
    markdown: String,
}

#[derive(Debug, Deserialize)]
struct RemoveEmployeeBlufArgs {
    employee_user_id: String,
    employee_name: String,
}

pub(crate) enum SummaryTool {
    ProjectGetSnapshot,
    SetProjectBluf {
        markdown: String,
    },
    SetProjectDetailedSummary {
        markdown: String,
    },
    OrganizationGetSnapshot,
    SetOrgBluf {
        markdown: String,
    },
    SetEmployeeBluf {
        employee_user_id: String,
        employee_name: String,
        project_ids: Vec<String>,
        markdown: String,
    },
    RemoveEmployeeBluf {
        employee_user_id: String,
        employee_name: String,
    },
}

impl SummaryTool {
    pub(crate) fn project_specs() -> Vec<DynamicToolSpec> {
        vec![
            spec(
                "get_snapshot",
                "Read the current project snapshot before deciding what to edit.",
                empty_schema(),
            ),
            spec(
                "set_bluf",
                "Replace the project BLUF markdown.",
                markdown_only_schema(),
            ),
            spec(
                "set_detailed_summary",
                "Replace the project detailed summary markdown.",
                markdown_only_schema(),
            ),
            spec(
                "set_employee_bluf",
                "Create or update a single employee BLUF scoped to this project.",
                json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["employee_user_id", "employee_name", "markdown"],
                    "properties": {
                        "employee_user_id": { "type": "string" },
                        "employee_name": { "type": "string" },
                        "markdown": { "type": "string" }
                    }
                }),
            ),
            spec(
                "remove_employee_bluf",
                "Remove an employee BLUF that should no longer appear in this project snapshot.",
                json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["employee_user_id", "employee_name"],
                    "properties": {
                        "employee_user_id": { "type": "string" },
                        "employee_name": { "type": "string" }
                    }
                }),
            ),
        ]
    }

    pub(crate) fn organization_specs() -> Vec<DynamicToolSpec> {
        vec![
            spec(
                "get_snapshot",
                "Read the current organization snapshot before deciding what to edit.",
                empty_schema(),
            ),
            spec(
                "set_org_bluf",
                "Replace the organization BLUF markdown.",
                markdown_only_schema(),
            ),
            spec(
                "set_employee_bluf",
                "Create or update a single employee BLUF using concise markdown body content.",
                json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["employee_user_id", "employee_name", "project_ids", "markdown"],
                    "properties": {
                        "employee_user_id": { "type": "string" },
                        "employee_name": { "type": "string" },
                        "project_ids": {
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
                    "required": ["employee_user_id", "employee_name"],
                    "properties": {
                        "employee_user_id": { "type": "string" },
                        "employee_name": { "type": "string" }
                    }
                }),
            ),
        ]
    }

    pub(crate) fn parse_project(params: &DynamicToolCallParams) -> Result<Self> {
        match params.tool.as_str() {
            "get_snapshot" => Ok(Self::ProjectGetSnapshot),
            "set_bluf" => {
                let args: SetMarkdownArgs = serde_json::from_value(params.arguments.clone())
                    .context("invalid set_bluf arguments")?;
                Ok(Self::SetProjectBluf {
                    markdown: args.markdown,
                })
            }
            "set_detailed_summary" => {
                let args: SetMarkdownArgs = serde_json::from_value(params.arguments.clone())
                    .context("invalid set_detailed_summary arguments")?;
                Ok(Self::SetProjectDetailedSummary {
                    markdown: args.markdown,
                })
            }
            "set_employee_bluf" => {
                let args: SetProjectEmployeeBlufArgs =
                    serde_json::from_value(params.arguments.clone())
                        .context("invalid set_employee_bluf arguments")?;
                Ok(Self::SetEmployeeBluf {
                    employee_user_id: args.employee_user_id,
                    employee_name: args.employee_name,
                    project_ids: Vec::new(),
                    markdown: args.markdown,
                })
            }
            "remove_employee_bluf" => {
                let args: RemoveEmployeeBlufArgs = serde_json::from_value(params.arguments.clone())
                    .context("invalid remove_employee_bluf arguments")?;
                Ok(Self::RemoveEmployeeBluf {
                    employee_user_id: args.employee_user_id,
                    employee_name: args.employee_name,
                })
            }
            other => anyhow::bail!("unknown project summary tool: {other}"),
        }
    }

    pub(crate) fn parse_organization(params: &DynamicToolCallParams) -> Result<Self> {
        match params.tool.as_str() {
            "get_snapshot" => Ok(Self::OrganizationGetSnapshot),
            "set_org_bluf" => {
                let args: SetMarkdownArgs = serde_json::from_value(params.arguments.clone())
                    .context("invalid set_org_bluf arguments")?;
                Ok(Self::SetOrgBluf {
                    markdown: args.markdown,
                })
            }
            "set_employee_bluf" => {
                let args: SetEmployeeBlufArgs = serde_json::from_value(params.arguments.clone())
                    .context("invalid set_employee_bluf arguments")?;
                Ok(Self::SetEmployeeBluf {
                    employee_user_id: args.employee_user_id,
                    employee_name: args.employee_name,
                    project_ids: args.project_ids,
                    markdown: args.markdown,
                })
            }
            "remove_employee_bluf" => {
                let args: RemoveEmployeeBlufArgs = serde_json::from_value(params.arguments.clone())
                    .context("invalid remove_employee_bluf arguments")?;
                Ok(Self::RemoveEmployeeBluf {
                    employee_user_id: args.employee_user_id,
                    employee_name: args.employee_name,
                })
            }
            other => anyhow::bail!("unknown organization summary tool: {other}"),
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

fn spec(name: &str, description: &str, schema: Value) -> DynamicToolSpec {
    DynamicToolSpec {
        name: name.to_owned(),
        description: description.to_owned(),
        input_schema: schema,
        defer_loading: false,
    }
}

fn empty_schema() -> Value {
    json!({ "type": "object", "additionalProperties": false, "properties": {} })
}

fn markdown_only_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["markdown"],
        "properties": { "markdown": { "type": "string" } }
    })
}
