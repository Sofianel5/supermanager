mod agent;
mod coordinator;
mod db;
mod event;
mod prompt;
mod tools;

use std::{fs, path::PathBuf, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use clap::Parser;
use codex_app_server_client::{
    DEFAULT_IN_PROCESS_CHANNEL_CAPACITY, InProcessAppServerClient, InProcessClientStartArgs,
};
use codex_arg0::Arg0DispatchPaths;
use codex_core::{
    config::Config,
    config_loader::{CloudRequirementsLoader, LoaderOverrides},
};
use codex_feedback::CodexFeedback;
use codex_protocol::protocol::SessionSource;
use tokio::sync::mpsc;

use crate::{
    agent::{AgentCommand, AgentLoop},
    coordinator::SummaryCoordinator,
    db::SummaryDb,
};

const CLIENT_NAME: &str = "supermanager_summary_agent";

#[derive(Parser, Debug)]
#[command(author, version, about = "Run the Supermanager summary worker")]
struct Cli {
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,
    #[arg(long, env = "SUPERMANAGER_DATA_DIR")]
    data_dir: PathBuf,
    #[arg(
        long,
        env = "SUPERMANAGER_SUMMARY_REFRESH_INTERVAL_SECONDS",
        default_value_t = 300
    )]
    organization_summary_refresh_interval_seconds: u64,
    #[arg(
        long,
        env = "SUPERMANAGER_ROOM_SUMMARY_POLL_INTERVAL_SECONDS",
        default_value_t = 5
    )]
    room_summary_poll_interval_seconds: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let paths = SummaryPaths::new(cli.data_dir);
    paths.initialize().await?;

    let db = SummaryDb::connect(&cli.database_url).await?;

    let config = Config::load_default_with_cli_overrides_for_codex_home(
        paths.codex_home.clone(),
        Vec::new(),
    )
    .context("failed to load default Codex config")?;
    let client = InProcessAppServerClient::start(InProcessClientStartArgs {
        arg0_paths: Arg0DispatchPaths::default(),
        config: Arc::new(config),
        cli_overrides: Vec::new(),
        loader_overrides: LoaderOverrides::default(),
        cloud_requirements: CloudRequirementsLoader::default(),
        feedback: CodexFeedback::new(),
        config_warnings: Vec::new(),
        session_source: SessionSource::Custom("supermanager".to_owned()),
        enable_codex_api_key_env: true,
        client_name: CLIENT_NAME.to_owned(),
        client_version: env!("CARGO_PKG_VERSION").to_owned(),
        experimental_api: true,
        opt_out_notification_methods: Vec::new(),
        channel_capacity: DEFAULT_IN_PROCESS_CHANNEL_CAPACITY,
    })
    .await
    .context("failed to start in-process Codex app server")?;

    let (command_tx, command_rx) = mpsc::channel::<AgentCommand>(256);
    let (event_tx, event_rx) = mpsc::channel(256);

    let agent_task = tokio::spawn({
        let db = db.clone();
        let summary_threads_dir = paths.summary_threads_dir.clone();
        async move {
            AgentLoop::new(client, command_rx, event_tx, db, summary_threads_dir)
                .run()
                .await
        }
    });

    let mut coordinator = SummaryCoordinator::new(
        db.clone(),
        command_tx.clone(),
        event_rx,
        Duration::from_secs(cli.organization_summary_refresh_interval_seconds),
        Duration::from_secs(cli.room_summary_poll_interval_seconds),
    );
    let coordinator_result = coordinator.run().await;

    let _ = command_tx.send(AgentCommand::Shutdown).await;
    let agent_result = agent_task.await.context("summary agent task join failed")?;

    db.close().await;

    coordinator_result?;
    agent_result?;

    Ok(())
}

struct SummaryPaths {
    codex_home: PathBuf,
    data_dir: PathBuf,
    summary_threads_dir: PathBuf,
}

impl SummaryPaths {
    fn new(data_dir: PathBuf) -> Self {
        Self {
            codex_home: data_dir.join("codex"),
            summary_threads_dir: data_dir.join("summary-threads"),
            data_dir,
        }
    }

    async fn initialize(&self) -> Result<()> {
        for path in [&self.data_dir, &self.codex_home, &self.summary_threads_dir] {
            fs::create_dir_all(path)
                .with_context(|| format!("failed to create {}", path.display()))?;
        }
        Ok(())
    }
}
