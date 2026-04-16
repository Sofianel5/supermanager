use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

pub struct JoinConfig {
    pub server_url: String,
    pub organization_slug: Option<String>,
    pub room_id: String,
    pub repo_dir: PathBuf,
    pub home_dir: PathBuf,
}

pub struct JoinOutcome {
    pub room_id: String,
    pub employee_name: String,
    pub dashboard_url: String,
    pub repo_dir: PathBuf,
}

pub struct CreateRoomConfig {
    pub home_dir: PathBuf,
    pub organization_slug: Option<String>,
    pub server_url: String,
    pub name: Option<String>,
    pub cwd: PathBuf,
}

pub struct LoginConfig {
    pub home_dir: PathBuf,
    pub server_url: String,
}

pub struct LoginOutcome {
    pub server_url: String,
    pub active_org_slug: String,
}

pub struct ListOrganizationsConfig {
    pub home_dir: PathBuf,
    pub server_url: String,
}

pub struct ListOrganizationsOutcome {
    pub active_org_slug: Option<String>,
    pub organizations: Vec<ListOrganizationEntry>,
}

pub struct ListOrganizationEntry {
    pub organization_name: String,
    pub organization_slug: String,
}

pub struct CreateOrganizationConfig {
    pub home_dir: PathBuf,
    pub server_url: String,
}

pub struct CreateOrganizationOutcome {
    pub organization_name: String,
    pub organization_slug: String,
}

pub struct ConfigureOrganizationsConfig {
    pub home_dir: PathBuf,
    pub server_url: String,
}

pub enum ConfigureOrganizationsOutcome {
    Selected {
        created_new: bool,
        organization_name: String,
        organization_slug: String,
    },
    InviteRequested,
}

pub struct InstallMcpConfig {
    pub home_dir: PathBuf,
    pub server_url: Option<String>,
}

pub struct InstallMcpOutcome {
    pub server_url: String,
    pub mcp_url: String,
    pub file_updates: Vec<ConfigFileUpdate>,
}

pub struct ConfigFileUpdate {
    pub path: String,
    pub status: ConfigFileUpdateStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigFileUpdateStatus {
    Created,
    Updated,
    Unchanged,
}

pub struct CreateRoomOutcome {
    pub room_id: String,
    pub room_name: String,
    pub dashboard_url: String,
    pub join_command: String,
    pub repo_dir: PathBuf,
}

pub struct LeaveOutcome {
    pub repo_dir: PathBuf,
    pub removed_paths: Vec<String>,
}

pub struct ListRoomsOutcome {
    pub rooms: Vec<ListRoomEntry>,
}

pub struct ListRoomEntry {
    pub organization_slug: String,
    pub room_id: String,
    pub server_url: String,
    pub repo_dirs: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct RepoRoomConfig {
    pub(crate) api_key: String,
    pub(crate) api_key_id: String,
    pub(crate) organization_slug: String,
    pub(crate) server_url: String,
    pub(crate) room_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct AuthState {
    pub(crate) access_token: String,
    pub(crate) active_org_slug: Option<String>,
    pub(crate) server_url: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct HomeRepoConfig {
    #[serde(default)]
    pub(crate) repos: BTreeMap<String, RepoRoomConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ViewerResponse {
    pub(crate) active_organization_id: Option<String>,
    pub(crate) organizations: Vec<ViewerOrganization>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ViewerOrganization {
    pub(crate) organization_id: String,
    pub(crate) organization_name: String,
    pub(crate) organization_slug: String,
}
