mod agent;
mod event;
mod ipc;
mod prompt;
mod tools;

use std::{fs, path::PathBuf, sync::Arc};

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
    agent::AgentLoop,
    ipc::{PendingToolCalls, read_host_messages, write_agent_messages},
};

const CLIENT_NAME: &str = "supermanager_summary_agent";

#[derive(Parser, Debug)]
#[command(author, version, about = "Run the Supermanager Codex summary agent")]
struct Cli {
    #[arg(long)]
    codex_home: PathBuf,
    #[arg(long)]
    organizations_dir: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    fs::create_dir_all(&cli.codex_home)
        .with_context(|| format!("failed to create codex home {}", cli.codex_home.display()))?;
    fs::create_dir_all(&cli.organizations_dir).with_context(|| {
        format!(
            "failed to create organizations dir {}",
            cli.organizations_dir.display()
        )
    })?;

    let config = Config::load_default_with_cli_overrides_for_codex_home(cli.codex_home, Vec::new())
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

    let (command_tx, command_rx) = mpsc::channel(256);
    let (output_tx, output_rx) = mpsc::channel(256);
    let pending_tool_calls: PendingToolCalls = Arc::default();

    let stdin_task = tokio::spawn(read_host_messages(
        command_tx.clone(),
        pending_tool_calls.clone(),
    ));
    let stdout_task = tokio::spawn(write_agent_messages(output_rx));

    let loop_state = AgentLoop::new(
        client,
        command_rx,
        output_tx.clone(),
        cli.organizations_dir,
        pending_tool_calls,
    );

    let run_result = loop_state.run().await;
    drop(output_tx);

    let stdin_result = stdin_task.await.context("stdin task join failed")?;
    let stdout_result = stdout_task.await.context("stdout task join failed")?;

    run_result?;
    stdin_result?;
    stdout_result?;
    Ok(())
}
