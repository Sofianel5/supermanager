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
struct StageRawArgs {
    session_id: String,
    markdown: String,
}

#[derive(Debug, Deserialize)]
struct DeleteRawArgs {
    session_id: String,
}

#[derive(Debug, Deserialize)]
struct UpsertSkillArgs {
    name: String,
    body: String,
}

#[derive(Debug, Deserialize)]
struct DeleteSkillArgs {
    name: String,
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
    WorkflowGetSnapshot,
    StageRawProjectMemory {
        session_id: String,
        markdown: String,
    },
    DeleteRawProjectMemory {
        session_id: String,
    },
    SetHandbook {
        markdown: String,
    },
    SetMemorySummary {
        markdown: String,
    },
    UpsertSkill {
        name: String,
        body: String,
    },
    DeleteSkill {
        name: String,
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

    pub(crate) fn project_memory_extract_specs() -> Vec<DynamicToolSpec> {
        vec![
            spec(
                "get_snapshot",
                "Read the current project memory snapshot — the durable handbook, the summary, and every raw staging entry already present for this project.",
                empty_schema(),
            ),
            spec(
                "stage_raw",
                "Stage the raw memory candidate for THIS transcript under its session id. Replaces any existing staged candidate for the same session.",
                stage_raw_schema(),
            ),
        ]
    }

    pub(crate) fn project_memory_consolidate_specs() -> Vec<DynamicToolSpec> {
        vec![
            spec(
                "get_snapshot",
                "Read the current project memory snapshot — the durable handbook, the summary, and every raw staging entry for this project.",
                empty_schema(),
            ),
            spec(
                "set_handbook",
                "Replace the project handbook (the full MEMORY payload). Send the complete new handbook, not a patch.",
                markdown_only_schema(),
            ),
            spec(
                "set_memory_summary",
                "Replace the short navigational memory summary for this project. Send the complete new summary, not a patch.",
                markdown_only_schema(),
            ),
            spec(
                "delete_raw",
                "Delete one raw staging entry by session id once it has been promoted or aged out.",
                delete_by_session_id_schema(),
            ),
        ]
    }

    pub(crate) fn project_skills_specs() -> Vec<DynamicToolSpec> {
        vec![
            spec(
                "get_snapshot",
                "Read the current project skills before deciding what to change. Each entry has a `name` and a `body` markdown payload.",
                empty_schema(),
            ),
            spec(
                "upsert_skill",
                "Create or replace one project skill by name. `body` is the full SKILL.md payload including frontmatter.",
                upsert_skill_schema(),
            ),
            spec(
                "delete_skill",
                "Delete one project skill by name when it is stale or no longer needed.",
                delete_by_name_schema(),
            ),
        ]
    }

    pub(crate) fn organization_memory_consolidate_specs() -> Vec<DynamicToolSpec> {
        vec![
            spec(
                "get_snapshot",
                "Read the current organization memory snapshot — the org-level handbook and summary plus read-only per-project handbooks and summaries.",
                empty_schema(),
            ),
            spec(
                "set_handbook",
                "Replace the organization handbook (the full org-wide MEMORY payload). Send the complete new handbook, not a patch.",
                markdown_only_schema(),
            ),
            spec(
                "set_memory_summary",
                "Replace the short navigational memory summary for the organization. Send the complete new summary, not a patch.",
                markdown_only_schema(),
            ),
        ]
    }

    pub(crate) fn organization_skills_specs() -> Vec<DynamicToolSpec> {
        vec![
            spec(
                "get_snapshot",
                "Read the current organization skills snapshot — org-level skills plus read-only per-project skills.",
                empty_schema(),
            ),
            spec(
                "upsert_skill",
                "Create or replace one organization-level skill by name. `body` is the full SKILL.md payload including frontmatter.",
                upsert_skill_schema(),
            ),
            spec(
                "delete_skill",
                "Delete one organization-level skill by name when it is stale or no longer needed.",
                delete_by_name_schema(),
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

    pub(crate) fn parse_project_memory_extract(params: &DynamicToolCallParams) -> Result<Self> {
        match params.tool.as_str() {
            "get_snapshot" => Ok(Self::WorkflowGetSnapshot),
            "stage_raw" => {
                let args: StageRawArgs = serde_json::from_value(params.arguments.clone())
                    .context("invalid stage_raw arguments")?;
                Ok(Self::StageRawProjectMemory {
                    session_id: args.session_id,
                    markdown: args.markdown,
                })
            }
            other => anyhow::bail!("unknown project memory extract tool: {other}"),
        }
    }

    pub(crate) fn parse_project_memory_consolidate(
        params: &DynamicToolCallParams,
    ) -> Result<Self> {
        match params.tool.as_str() {
            "get_snapshot" => Ok(Self::WorkflowGetSnapshot),
            "set_handbook" => {
                let args: SetMarkdownArgs = serde_json::from_value(params.arguments.clone())
                    .context("invalid set_handbook arguments")?;
                Ok(Self::SetHandbook {
                    markdown: args.markdown,
                })
            }
            "set_memory_summary" => {
                let args: SetMarkdownArgs = serde_json::from_value(params.arguments.clone())
                    .context("invalid set_memory_summary arguments")?;
                Ok(Self::SetMemorySummary {
                    markdown: args.markdown,
                })
            }
            "delete_raw" => {
                let args: DeleteRawArgs = serde_json::from_value(params.arguments.clone())
                    .context("invalid delete_raw arguments")?;
                Ok(Self::DeleteRawProjectMemory {
                    session_id: args.session_id,
                })
            }
            other => anyhow::bail!("unknown project memory consolidate tool: {other}"),
        }
    }

    pub(crate) fn parse_skills(params: &DynamicToolCallParams) -> Result<Self> {
        match params.tool.as_str() {
            "get_snapshot" => Ok(Self::WorkflowGetSnapshot),
            "upsert_skill" => {
                let args: UpsertSkillArgs = serde_json::from_value(params.arguments.clone())
                    .context("invalid upsert_skill arguments")?;
                Ok(Self::UpsertSkill {
                    name: args.name,
                    body: args.body,
                })
            }
            "delete_skill" => {
                let args: DeleteSkillArgs = serde_json::from_value(params.arguments.clone())
                    .context("invalid delete_skill arguments")?;
                Ok(Self::DeleteSkill { name: args.name })
            }
            other => anyhow::bail!("unknown skills tool: {other}"),
        }
    }

    pub(crate) fn parse_organization_memory_consolidate(
        params: &DynamicToolCallParams,
    ) -> Result<Self> {
        match params.tool.as_str() {
            "get_snapshot" => Ok(Self::WorkflowGetSnapshot),
            "set_handbook" => {
                let args: SetMarkdownArgs = serde_json::from_value(params.arguments.clone())
                    .context("invalid set_handbook arguments")?;
                Ok(Self::SetHandbook {
                    markdown: args.markdown,
                })
            }
            "set_memory_summary" => {
                let args: SetMarkdownArgs = serde_json::from_value(params.arguments.clone())
                    .context("invalid set_memory_summary arguments")?;
                Ok(Self::SetMemorySummary {
                    markdown: args.markdown,
                })
            }
            other => anyhow::bail!("unknown organization memory consolidate tool: {other}"),
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

fn stage_raw_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["session_id", "markdown"],
        "properties": {
            "session_id": { "type": "string" },
            "markdown": { "type": "string" }
        }
    })
}

fn delete_by_session_id_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["session_id"],
        "properties": { "session_id": { "type": "string" } }
    })
}

fn upsert_skill_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["name", "body"],
        "properties": {
            "name": { "type": "string" },
            "body": { "type": "string" }
        }
    })
}

fn delete_by_name_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["name"],
        "properties": { "name": { "type": "string" } }
    })
}
