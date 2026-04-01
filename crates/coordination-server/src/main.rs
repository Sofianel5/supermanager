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
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let db = Arc::new(Db::open(&cli.db_path)?);
    db.ensure_local_room()?;

    let (note_events, _) = broadcast::channel(256);

    let state = AppState {
        db,
        note_events,
        base_url: cli.base_url,
    };

    let app = Router::new()
        // ── Room management ──────────────────────────────
        .route("/v1/rooms", post(api::create_room))
        // ── Room-scoped routes ───────────────────────────
        .route("/r/{room_id}", get(api::dashboard))
        .route("/r/{room_id}/feed", get(api::get_feed))
        .route("/r/{room_id}/feed/stream", get(api::stream_feed))
        .route("/r/{room_id}/progress", post(api::ingest_progress))
        .route(
            "/r/{room_id}/summary",
            get(api::get_manager_summary).put(api::update_manager_summary),
        )
        .route("/r/{room_id}/mcp", post(api::handle_mcp))
        .route("/r/{room_id}/install", get(api::install_script))
        // ── Legacy (backwards-compat) routes ─────────────
        .route("/v1/progress", post(api::legacy_ingest_progress))
        .route("/v1/feed", get(api::legacy_get_feed))
        .route("/v1/feed/stream", get(api::legacy_stream_feed))
        .route(
            "/v1/manager-summary",
            get(api::legacy_get_manager_summary).put(api::legacy_update_manager_summary),
        )
        .route("/mcp", post(api::legacy_handle_mcp))
        // ── Health ───────────────────────────────────────
        .route("/health", get(api::health))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(cli.bind).await?;
    println!("coordination-server listening on http://{}", cli.bind);
    axum::serve(listener, app).await?;
    Ok(())
}
