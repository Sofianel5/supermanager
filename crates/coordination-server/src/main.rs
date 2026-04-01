mod api;
mod store;

use std::{net::SocketAddr, path::PathBuf};

use anyhow::Result;
use axum::{
    Router,
    routing::{get, post},
};
use clap::Parser;
use tokio::sync::broadcast;

use api::AppState;

#[derive(Parser, Debug)]
#[command(author, version, about = "Ingest progress notes and serve a live feed")]
struct Cli {
    #[arg(long, default_value = "127.0.0.1:8787")]
    bind: SocketAddr,
    #[arg(long, default_value = "data")]
    data_dir: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    store::initialize(&cli.data_dir)?;
    let (note_events, _) = broadcast::channel(256);

    let state = AppState {
        data_dir: cli.data_dir,
        note_events,
    };

    let app = Router::new()
        .route("/health", get(api::health))
        .route("/v1/progress", post(api::ingest_progress))
        .route("/v1/feed", get(api::get_feed))
        .route("/v1/feed/stream", get(api::stream_feed))
        .route("/mcp", post(api::handle_mcp))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(cli.bind).await?;
    println!("coordination-server listening on http://{}", cli.bind);
    axum::serve(listener, app).await?;
    Ok(())
}
