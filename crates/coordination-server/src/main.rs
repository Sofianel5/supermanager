mod api;
mod store;

use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use anyhow::Result;
use axum::{
    Router,
    routing::{get, post},
};
use clap::Parser;
use tokio::sync::broadcast;

use api::AppState;
use store::Db;

#[derive(Parser, Debug)]
#[command(author, version, about = "Ingest progress notes and serve a live feed")]
struct Cli {
    #[arg(long, default_value = "127.0.0.1:8787")]
    bind: SocketAddr,
    #[arg(long, default_value = "supermanager.db")]
    db_path: PathBuf,
    #[arg(long, default_value = "http://127.0.0.1:8787")]
    base_url: String,
    #[arg(
        long,
        default_value = "cargo install --git https://github.com/Sofianel5/supermanager.git supermanager"
    )]
    cli_install_command: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let db = Arc::new(Db::open(&cli.db_path)?);

    let (hook_events, _) = broadcast::channel(256);
    let (summary_events, _) = broadcast::channel(64);

    let state = AppState {
        db,
        hook_events,
        summary_events,
        base_url: cli.base_url,
        cli_install_command: cli.cli_install_command,
        http: reqwest::Client::new(),
        openai_api_key: std::env::var("OPENAI_API_KEY").ok(),
    };

    let app = Router::new()
        // ── Room management ──────────────────────────────
        .route("/v1/rooms", post(api::create_room))
        // ── Room-scoped routes ───────────────────────────
        .route("/r/{room_id}", get(api::dashboard))
        .route("/r/{room_id}/feed", get(api::get_feed))
        .route("/r/{room_id}/feed/stream", get(api::stream_feed))
        .route("/r/{room_id}/hooks/turn", post(api::ingest_hook_turn))
        .route("/r/{room_id}/summary", get(api::get_manager_summary))
        .route("/r/{room_id}/tasks", get(api::get_tasks_http))
        // ── Landing page ────────────────────────────────
        .route("/", get(api::landing_page))
        // ── Health ───────────────────────────────────────
        .route("/health", get(api::health))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(cli.bind).await?;
    println!("coordination-server listening on http://{}", cli.bind);
    axum::serve(listener, app).await?;
    Ok(())
}
