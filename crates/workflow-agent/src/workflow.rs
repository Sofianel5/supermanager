use std::path::PathBuf;

use anyhow::Context;
use codex_app_server_protocol::{AskForApproval, DynamicToolSpec, SandboxMode};

use crate::{
    prompt::{
        ORGANIZATION_MEMORY_CONSOLIDATE_SYSTEM_PROMPT, ORGANIZATION_SKILLS_SYSTEM_PROMPT,
        ORGANIZATION_SUMMARY_SYSTEM_PROMPT, PROJECT_MEMORY_CONSOLIDATE_SYSTEM_PROMPT,
        PROJECT_MEMORY_EXTRACT_SYSTEM_PROMPT, PROJECT_SKILLS_SYSTEM_PROMPT,
        PROJECT_SUMMARY_SYSTEM_PROMPT,
    },
    tools::SummaryTool,
};

const SUMMARY_MODEL: &str = "gpt-5.4-mini";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum WorkflowKind {
    ProjectSummary,
    OrganizationSummary,
    ProjectMemoryExtract,
    ProjectMemoryConsolidate,
    ProjectSkills,
    OrganizationMemoryConsolidate,
    OrganizationSkills,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkflowScope {
    Project,
    Organization,
}

impl WorkflowKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::ProjectSummary => "project_summary",
            Self::OrganizationSummary => "organization_summary",
            Self::ProjectMemoryExtract => "project_memory_extract",
            Self::ProjectMemoryConsolidate => "project_memory_consolidate",
            Self::ProjectSkills => "project_skills",
            Self::OrganizationMemoryConsolidate => "organization_memory_consolidate",
            Self::OrganizationSkills => "organization_skills",
        }
    }

    /// Whether the workflow is tracked per-project or per-organization.
    pub(crate) fn scope(self) -> WorkflowScope {
        match self {
            Self::ProjectSummary
            | Self::ProjectMemoryExtract
            | Self::ProjectMemoryConsolidate
            | Self::ProjectSkills => WorkflowScope::Project,
            Self::OrganizationSummary
            | Self::OrganizationMemoryConsolidate
            | Self::OrganizationSkills => WorkflowScope::Organization,
        }
    }

    pub(crate) fn directory_name(self) -> &'static str {
        match self {
            Self::ProjectSummary => "project-summary",
            Self::OrganizationSummary => "organization-summary",
            Self::ProjectMemoryExtract => "project-memory-extract",
            Self::ProjectMemoryConsolidate => "project-memory-consolidate",
            Self::ProjectSkills => "project-skills",
            Self::OrganizationMemoryConsolidate => "organization-memory-consolidate",
            Self::OrganizationSkills => "organization-skills",
        }
    }

    pub(crate) fn system_prompt(self) -> &'static str {
        match self {
            Self::ProjectSummary => PROJECT_SUMMARY_SYSTEM_PROMPT,
            Self::OrganizationSummary => ORGANIZATION_SUMMARY_SYSTEM_PROMPT,
            Self::ProjectMemoryExtract => PROJECT_MEMORY_EXTRACT_SYSTEM_PROMPT,
            Self::ProjectMemoryConsolidate => PROJECT_MEMORY_CONSOLIDATE_SYSTEM_PROMPT,
            Self::ProjectSkills => PROJECT_SKILLS_SYSTEM_PROMPT,
            Self::OrganizationMemoryConsolidate => ORGANIZATION_MEMORY_CONSOLIDATE_SYSTEM_PROMPT,
            Self::OrganizationSkills => ORGANIZATION_SKILLS_SYSTEM_PROMPT,
        }
    }

    pub(crate) fn model(self) -> &'static str {
        SUMMARY_MODEL
    }

    pub(crate) fn sandbox(self) -> SandboxMode {
        SandboxMode::ReadOnly
    }

    pub(crate) fn approval_policy(self) -> AskForApproval {
        AskForApproval::Never
    }

    pub(crate) fn dynamic_tools(self) -> Option<Vec<DynamicToolSpec>> {
        match self {
            Self::ProjectSummary => Some(SummaryTool::project_specs()),
            Self::OrganizationSummary => Some(SummaryTool::organization_specs()),
            Self::ProjectMemoryExtract => Some(SummaryTool::project_memory_extract_specs()),
            Self::ProjectMemoryConsolidate => Some(SummaryTool::project_memory_consolidate_specs()),
            Self::ProjectSkills => Some(SummaryTool::project_skills_specs()),
            Self::OrganizationMemoryConsolidate => {
                Some(SummaryTool::organization_memory_consolidate_specs())
            }
            Self::OrganizationSkills => Some(SummaryTool::organization_skills_specs()),
        }
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
    pub(crate) required_decisions: Vec<WorkflowDecisionRequirement>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum WorkflowDecisionRequirement {
    ProjectEvent { source_event_id: String },
    OrganizationWindow { source_window_key: String },
}

impl WorkflowDecisionRequirement {
    pub(crate) fn label(&self) -> String {
        match self {
            Self::ProjectEvent { source_event_id } => format!("event {source_event_id}"),
            Self::OrganizationWindow { source_window_key } => {
                format!("window {source_window_key}")
            }
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum WorkflowCursor {
    Seq(i64),
    ReceivedAt {
        received_at: String,
        // Secondary key resolves ties when a batch hits its row limit at a
        // `received_at` value shared by multiple rows. `None` means the batch
        // advanced past every row at that `received_at` (or never hit the
        // limit), so no tiebreaker is needed for the next sweep.
        secondary: Option<WorkflowCursorSecondary>,
    },
}

#[derive(Clone, Debug)]
pub(crate) enum WorkflowCursorSecondary {
    Seq(i64),
    SessionId(String),
}

pub(crate) struct WorkflowPaths {
    pub(crate) codex_home: PathBuf,
    workflow_threads_dir: PathBuf,
}

impl WorkflowPaths {
    pub(crate) fn new(data_dir: PathBuf) -> Self {
        Self {
            codex_home: data_dir.join("codex"),
            workflow_threads_dir: data_dir.join("workflow-threads"),
        }
    }

    pub(crate) async fn initialize(&self) -> anyhow::Result<()> {
        for path in [&self.codex_home, &self.workflow_threads_dir] {
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

    pub(crate) async fn prepare_cwd(&self, target: &WorkflowTarget) -> anyhow::Result<PathBuf> {
        let cwd = self.thread_state_dir(target).join("cwd");

        tokio::fs::create_dir_all(&cwd)
            .await
            .with_context(|| format!("failed to create workflow cwd: {}", cwd.display()))?;

        Ok(cwd)
    }
}
