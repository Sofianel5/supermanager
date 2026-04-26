mod agent;
mod coordinator;
mod db;
mod event;
mod prompt;
mod tools;
mod workflow;

use std::{path::PathBuf, sync::Arc, time::Duration};

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
    coordinator::WorkflowCoordinator,
    db::SummaryDb,
    workflow::WorkflowPaths,
};

const CLIENT_NAME: &str = "supermanager_workflow_agent";

#[derive(Parser, Debug)]
#[command(author, version, about = "Run the Supermanager workflow worker")]
struct Cli {
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,
    #[arg(long, env = "SUPERMANAGER_DATA_DIR")]
    data_dir: PathBuf,
    #[arg(
        long,
        env = "SUPERMANAGER_ORGANIZATION_SUMMARY_REFRESH_INTERVAL_SECONDS",
        default_value_t = 300
    )]
    organization_summary_refresh_interval_seconds: u64,
    #[arg(
        long,
        env = "SUPERMANAGER_PROJECT_SUMMARY_POLL_INTERVAL_SECONDS",
        default_value_t = 300
    )]
    project_summary_poll_interval_seconds: u64,
    #[arg(
        long,
        env = "SUPERMANAGER_PROJECT_MEMORY_EXTRACT_INTERVAL_SECONDS",
        default_value_t = 600
    )]
    project_memory_extract_interval_seconds: u64,
    #[arg(
        long,
        env = "SUPERMANAGER_PROJECT_MEMORY_CONSOLIDATE_INTERVAL_SECONDS",
        default_value_t = 900
    )]
    project_memory_consolidate_interval_seconds: u64,
    #[arg(
        long,
        env = "SUPERMANAGER_PROJECT_SKILLS_INTERVAL_SECONDS",
        default_value_t = 900
    )]
    project_skills_interval_seconds: u64,
    #[arg(
        long,
        env = "SUPERMANAGER_ORGANIZATION_MEMORY_CONSOLIDATE_INTERVAL_SECONDS",
        default_value_t = 1_800
    )]
    organization_memory_consolidate_interval_seconds: u64,
    #[arg(
        long,
        env = "SUPERMANAGER_ORGANIZATION_SKILLS_INTERVAL_SECONDS",
        default_value_t = 1_800
    )]
    organization_skills_interval_seconds: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let workflow_paths = WorkflowPaths::new(cli.data_dir);
    workflow_paths.initialize().await?;

    let db = SummaryDb::connect(&cli.database_url).await?;

    let config = Config::load_default_with_cli_overrides_for_codex_home(
        workflow_paths.codex_home.clone(),
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
        let workflow_paths = workflow_paths;
        async move {
            AgentLoop::new(client, command_rx, event_tx, db, workflow_paths)
                .run()
                .await
        }
    });

    let mut coordinator = WorkflowCoordinator::new(
        db.clone(),
        command_tx.clone(),
        event_rx,
        Duration::from_secs(cli.organization_summary_refresh_interval_seconds),
        Duration::from_secs(cli.project_summary_poll_interval_seconds),
        Duration::from_secs(cli.project_memory_extract_interval_seconds),
        Duration::from_secs(cli.project_memory_consolidate_interval_seconds),
        Duration::from_secs(cli.project_skills_interval_seconds),
        Duration::from_secs(cli.organization_memory_consolidate_interval_seconds),
        Duration::from_secs(cli.organization_skills_interval_seconds),
    );
    let coordinator_result = coordinator.run().await;

    let _ = command_tx.send(AgentCommand::Shutdown).await;
    let agent_result = agent_task
        .await
        .context("workflow agent task join failed")?;

    db.close().await;

    coordinator_result?;
    agent_result?;

    Ok(())
}
