mod auth;
mod context;
mod local;
mod mcp;
mod orgs;
mod projects;
mod support;
mod types;
mod update;

pub use auth::{login, logout, resolve_home_dir};
pub use context::{sync_repo_context, sync_repo_context_from_hook};
pub use local::{copy_to_clipboard, leave_repo, list_projects, report_hook_turn};
pub use mcp::install_mcp;
pub use orgs::{
    configure_organizations_interactive, create_organization_interactive, list_organizations,
};
pub use projects::{create_project, join_repo};
pub use support::DEFAULT_SERVER_URL;
pub use types::{
    ConfigFileUpdate, ConfigFileUpdateStatus, ConfigureOrganizationsConfig,
    ConfigureOrganizationsOutcome, CreateOrganizationConfig, CreateOrganizationOutcome,
    CreateProjectConfig, CreateProjectOutcome, InstallMcpConfig, InstallMcpOutcome, JoinConfig,
    JoinOutcome, LeaveOutcome, ListOrganizationEntry, ListOrganizationsConfig,
    ListOrganizationsOutcome, ListProjectEntry, ListProjectsOutcome, LoginConfig, LoginOutcome,
    SyncContextConfig, SyncContextOutcome,
};
pub use update::{SelfUpdateOutcome, maybe_auto_update, run_self_update};
