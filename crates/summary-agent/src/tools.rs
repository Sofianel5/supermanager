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
struct SetMemberBlufArgs {
    member_user_id: String,
    member_name: String,
    project_ids: Vec<String>,
    markdown: String,
}

#[derive(Debug, Deserialize)]
struct SetProjectMemberBlufArgs {
    member_user_id: String,
    member_name: String,
    markdown: String,
}

#[derive(Debug, Deserialize)]
struct RemoveMemberBlufArgs {
    member_user_id: String,
    member_name: String,
}

#[derive(Debug, Deserialize)]
struct UpsertWorkflowFileArgs {
    path: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct DeleteWorkflowFileArgs {
    path: String,
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
    SetMemberBluf {
        member_user_id: String,
        member_name: String,
        project_ids: Vec<String>,
        markdown: String,
    },
    RemoveMemberBluf {
        member_user_id: String,
        member_name: String,
    },
    OrganizationWorkflowGetSnapshot,
    UpsertOrganizationWorkflowFile {
        path: String,
        content: String,
    },
    DeleteOrganizationWorkflowFile {
        path: String,
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
                "set_member_bluf",
                "Create or update a single member BLUF scoped to this project.",
                json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["member_user_id", "member_name", "markdown"],
                    "properties": {
                        "member_user_id": { "type": "string" },
                        "member_name": { "type": "string" },
                        "markdown": { "type": "string" }
                    }
                }),
            ),
            spec(
                "remove_member_bluf",
                "Remove a member BLUF that should no longer appear in this project snapshot.",
                json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["member_user_id", "member_name"],
                    "properties": {
                        "member_user_id": { "type": "string" },
                        "member_name": { "type": "string" }
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
                "set_member_bluf",
                "Create or update a single member BLUF using concise markdown body content.",
                json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["member_user_id", "member_name", "project_ids", "markdown"],
                    "properties": {
                        "member_user_id": { "type": "string" },
                        "member_name": { "type": "string" },
                        "project_ids": {
                            "type": "array",
                            "items": { "type": "string" }
                        },
                        "markdown": { "type": "string" }
                    }
                }),
            ),
            spec(
                "remove_member_bluf",
                "Remove a member BLUF that should no longer appear in the organization snapshot.",
                json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["member_user_id", "member_name"],
                    "properties": {
                        "member_user_id": { "type": "string" },
                        "member_name": { "type": "string" }
                    }
                }),
            ),
        ]
    }

    pub(crate) fn organization_memory_specs() -> Vec<DynamicToolSpec> {
        organization_workflow_specs(
            "Read the current DB-backed organization memory files before deciding what to change.",
            "Create or replace one organization memory file. Paths are relative to the organization memory root, for example `MEMORY.md` or `memory_summary.md`.",
            "Delete one organization memory file by relative path when it is stale or no longer needed.",
        )
    }

    pub(crate) fn organization_skills_specs() -> Vec<DynamicToolSpec> {
        organization_workflow_specs(
            "Read the current DB-backed organization skill files before deciding what to change.",
            "Create or replace one organization skill file. Paths are relative to the organization skills root, for example `code-review/SKILL.md`.",
            "Delete one organization skill file by relative path when it is stale or no longer needed.",
        )
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
            "set_member_bluf" => {
                let args: SetProjectMemberBlufArgs =
                    serde_json::from_value(params.arguments.clone())
                        .context("invalid set_member_bluf arguments")?;
                Ok(Self::SetMemberBluf {
                    member_user_id: args.member_user_id,
                    member_name: args.member_name,
                    project_ids: Vec::new(),
                    markdown: args.markdown,
                })
            }
            "remove_member_bluf" => {
                let args: RemoveMemberBlufArgs = serde_json::from_value(params.arguments.clone())
                    .context("invalid remove_member_bluf arguments")?;
                Ok(Self::RemoveMemberBluf {
                    member_user_id: args.member_user_id,
                    member_name: args.member_name,
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
            "set_member_bluf" => {
                let args: SetMemberBlufArgs = serde_json::from_value(params.arguments.clone())
                    .context("invalid set_member_bluf arguments")?;
                Ok(Self::SetMemberBluf {
                    member_user_id: args.member_user_id,
                    member_name: args.member_name,
                    project_ids: args.project_ids,
                    markdown: args.markdown,
                })
            }
            "remove_member_bluf" => {
                let args: RemoveMemberBlufArgs = serde_json::from_value(params.arguments.clone())
                    .context("invalid remove_member_bluf arguments")?;
                Ok(Self::RemoveMemberBluf {
                    member_user_id: args.member_user_id,
                    member_name: args.member_name,
                })
            }
            other => anyhow::bail!("unknown organization summary tool: {other}"),
        }
    }

    pub(crate) fn parse_organization_workflow_documents(
        params: &DynamicToolCallParams,
    ) -> Result<Self> {
        match params.tool.as_str() {
            "get_snapshot" => Ok(Self::OrganizationWorkflowGetSnapshot),
            "upsert_file" => {
                let args: UpsertWorkflowFileArgs = serde_json::from_value(params.arguments.clone())
                    .context("invalid upsert_file arguments")?;
                Ok(Self::UpsertOrganizationWorkflowFile {
                    path: args.path,
                    content: args.content,
                })
            }
            "delete_file" => {
                let args: DeleteWorkflowFileArgs = serde_json::from_value(params.arguments.clone())
                    .context("invalid delete_file arguments")?;
                Ok(Self::DeleteOrganizationWorkflowFile { path: args.path })
            }
            other => anyhow::bail!("unknown organization workflow document tool: {other}"),
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

fn organization_workflow_specs(
    get_snapshot_description: &str,
    upsert_file_description: &str,
    delete_file_description: &str,
) -> Vec<DynamicToolSpec> {
    vec![
        spec("get_snapshot", get_snapshot_description, empty_schema()),
        spec(
            "upsert_file",
            upsert_file_description,
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["path", "content"],
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                }
            }),
        ),
        spec(
            "delete_file",
            delete_file_description,
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["path"],
                "properties": {
                    "path": { "type": "string" }
                }
            }),
        ),
    ]
}
