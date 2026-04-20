use std::path::PathBuf;

use anyhow::Context;
use codex_app_server_protocol::{AskForApproval, DynamicToolSpec, SandboxMode};

use crate::{
    prompt::{
        ORGANIZATION_MEMORY_SYSTEM_PROMPT, ORGANIZATION_SKILLS_SYSTEM_PROMPT,
        ORGANIZATION_SUMMARY_SYSTEM_PROMPT, PROJECT_SUMMARY_SYSTEM_PROMPT,
    },
    tools::SummaryTool,
};

const SUMMARY_MODEL: &str = "gpt-5.4-mini";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum WorkflowKind {
    ProjectSummary,
    OrganizationSummary,
    OrganizationMemories,
    OrganizationSkills,
}

impl WorkflowKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::ProjectSummary => "project_summary",
            Self::OrganizationSummary => "organization_summary",
            Self::OrganizationMemories => "organization_memories",
            Self::OrganizationSkills => "organization_skills",
        }
    }

    pub(crate) fn directory_name(self) -> &'static str {
        match self {
            Self::ProjectSummary => "project-summary",
            Self::OrganizationSummary => "organization-summary",
            Self::OrganizationMemories => "organization-memories",
            Self::OrganizationSkills => "organization-skills",
        }
    }

    pub(crate) fn system_prompt(self) -> &'static str {
        match self {
            Self::ProjectSummary => PROJECT_SUMMARY_SYSTEM_PROMPT,
            Self::OrganizationSummary => ORGANIZATION_SUMMARY_SYSTEM_PROMPT,
            Self::OrganizationMemories => ORGANIZATION_MEMORY_SYSTEM_PROMPT,
            Self::OrganizationSkills => ORGANIZATION_SKILLS_SYSTEM_PROMPT,
        }
    }

    pub(crate) fn model(self) -> &'static str {
        SUMMARY_MODEL
    }

    pub(crate) fn sandbox(self) -> SandboxMode {
        match self {
            Self::ProjectSummary | Self::OrganizationSummary => SandboxMode::ReadOnly,
            Self::OrganizationMemories | Self::OrganizationSkills => SandboxMode::WorkspaceWrite,
        }
    }

    pub(crate) fn approval_policy(self) -> AskForApproval {
        AskForApproval::Never
    }

    pub(crate) fn dynamic_tools(self) -> Option<Vec<DynamicToolSpec>> {
        match self {
            Self::ProjectSummary => Some(SummaryTool::project_specs()),
            Self::OrganizationSummary => Some(SummaryTool::organization_specs()),
            Self::OrganizationMemories | Self::OrganizationSkills => None,
        }
    }

    fn uses_organization_workspace(self) -> bool {
        matches!(
            self,
            Self::OrganizationSummary | Self::OrganizationMemories | Self::OrganizationSkills
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct WorkflowTarget {
    pub(crate) kind: WorkflowKind,
    pub(crate) id: String,
}

impl WorkflowTarget {
    pub(crate) fn new(kind: WorkflowKind, id: impl Into<String>) -> Self {
        Self {
            kind,
            id: id.into(),
        }
    }

    pub(crate) fn label(&self) -> String {
        format!("{} {}", self.kind.as_str(), self.id)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct WorkflowDispatch {
    pub(crate) target: WorkflowTarget,
    pub(crate) input: String,
}

#[derive(Clone, Debug)]
pub(crate) enum WorkflowCursor {
    Seq(i64),
    ReceivedAt(String),
}

pub(crate) struct WorkflowPaths {
    pub(crate) codex_home: PathBuf,
    workflow_threads_dir: PathBuf,
    organization_workspaces_dir: PathBuf,
}

impl WorkflowPaths {
    pub(crate) fn new(data_dir: PathBuf) -> Self {
        Self {
            codex_home: data_dir.join("codex"),
            workflow_threads_dir: data_dir.join("workflow-threads"),
            organization_workspaces_dir: data_dir.join("organization-workspaces"),
        }
    }

    pub(crate) async fn initialize(&self) -> anyhow::Result<()> {
        for path in [
            &self.codex_home,
            &self.workflow_threads_dir,
            &self.organization_workspaces_dir,
        ] {
            tokio::fs::create_dir_all(path)
                .await
                .with_context(|| format!("failed to create {}", path.display()))?;
        }
        Ok(())
    }

    pub(crate) fn thread_state_dir(&self, target: &WorkflowTarget) -> PathBuf {
        self.workflow_threads_dir
            .join(target.kind.directory_name())
            .join(&target.id)
    }

    pub(crate) fn organization_workspace_dir(&self, organization_id: &str) -> PathBuf {
        self.organization_workspaces_dir.join(organization_id)
    }

    pub(crate) async fn prepare_cwd(&self, target: &WorkflowTarget) -> anyhow::Result<PathBuf> {
        let cwd = if target.kind.uses_organization_workspace() {
            self.organization_workspace_dir(&target.id)
        } else {
            self.thread_state_dir(target).join("cwd")
        };

        tokio::fs::create_dir_all(&cwd)
            .await
            .with_context(|| format!("failed to create workflow cwd: {}", cwd.display()))?;

        if target.kind.uses_organization_workspace() {
            for subdir in [cwd.join("memories"), cwd.join(".codex/skills")] {
                tokio::fs::create_dir_all(&subdir).await.with_context(|| {
                    format!("failed to create workflow subdir: {}", subdir.display())
                })?;
            }
        }

        Ok(cwd)
    }
}
