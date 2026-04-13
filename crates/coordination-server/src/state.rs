use std::{fs, path::PathBuf, sync::Arc};

use anyhow::Context;
use reporter_protocol::StoredHookEvent;
use tokio::sync::broadcast;

use crate::agent::RoomSummaryAgent;
use crate::agent::summarize::SummaryStatusEvent;
use crate::auth::AuthConfig;
use crate::store::Db;

#[derive(Clone)]
pub struct HookFeedEvent {
    pub room_id: String,
    pub event: StoredHookEvent,
}

#[derive(Clone, Debug)]
pub struct StoragePaths {
    pub data_dir: PathBuf,
    pub codex_home: PathBuf,
    pub rooms_dir: PathBuf,
}

impl StoragePaths {
    pub fn new(data_dir: PathBuf) -> Self {
        let codex_home = data_dir.join("codex");
        let rooms_dir = data_dir.join("rooms");
        Self {
            data_dir,
            codex_home,
            rooms_dir,
        }
    }

    pub fn initialize(&self) -> anyhow::Result<()> {
        for path in [&self.data_dir, &self.codex_home, &self.rooms_dir] {
            fs::create_dir_all(path)
                .with_context(|| format!("failed to create storage dir {}", path.display()))?;
        }
        Ok(())
    }

    pub fn check_ready(&self) -> anyhow::Result<()> {
        for path in [&self.data_dir, &self.codex_home, &self.rooms_dir] {
            if !path.is_dir() {
                anyhow::bail!("storage dir missing or not a directory: {}", path.display());
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Db>,
    pub agent: RoomSummaryAgent,
    pub hook_events: broadcast::Sender<HookFeedEvent>,
    pub summary_events: broadcast::Sender<SummaryStatusEvent>,
    pub storage: StoragePaths,
    pub public_api_url: String,
    pub public_app_url: String,
    pub auth: AuthConfig,
}
