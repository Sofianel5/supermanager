mod auth;
mod local;
mod mcp;
mod orgs;
mod rooms;
mod support;
mod types;
mod update;

pub use auth::{login, logout, resolve_home_dir};
pub use local::{copy_to_clipboard, leave_repo, list_rooms, report_hook_turn};
pub use mcp::install_mcp;
pub use orgs::{
    configure_organizations_interactive, create_organization_interactive, list_organizations,
};
pub use rooms::{create_room, join_repo};
pub use support::DEFAULT_SERVER_URL;
pub use types::{
    ConfigFileUpdate, ConfigFileUpdateStatus, ConfigureOrganizationsConfig,
    ConfigureOrganizationsOutcome, CreateOrganizationConfig, CreateOrganizationOutcome,
    CreateRoomConfig, CreateRoomOutcome, InstallMcpConfig, InstallMcpOutcome, JoinConfig,
    JoinOutcome, LeaveOutcome, ListOrganizationEntry, ListOrganizationsConfig,
    ListOrganizationsOutcome, ListRoomEntry, ListRoomsOutcome, LoginConfig, LoginOutcome,
};
pub use update::{SelfUpdateOutcome, maybe_auto_update, run_self_update};
