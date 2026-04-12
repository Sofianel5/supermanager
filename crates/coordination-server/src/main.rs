mod api;
mod store;

use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use anyhow::Result;
use axum::{
    Router,
    http::{HeaderName, Method, header},
    routing::{get, post},
};
use clap::Parser;
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};

use api::{AppState, RoomSummaryAgent, StoragePaths};
use store::Db;

#[derive(Parser, Debug)]
#[command(author, version, about = "Ingest progress notes and serve a live feed")]
struct Cli {
    #[arg(long, default_value = "127.0.0.1:8787")]
    bind: SocketAddr,
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,
    #[arg(long, env = "SUPERMANAGER_DATA_DIR")]
    data_dir: PathBuf,
    #[arg(
        long,
        env = "SUPERMANAGER_PUBLIC_API_URL",
        default_value = "http://127.0.0.1:8787"
    )]
    public_api_url: String,
    #[arg(
        long,
        env = "SUPERMANAGER_PUBLIC_APP_URL",
        default_value = "http://127.0.0.1:5173"
    )]
    public_app_url: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let db = Arc::new(Db::connect(&cli.database_url).await?);
    let storage = StoragePaths::new(cli.data_dir.clone());
    storage.initialize()?;

    let (hook_events, _) = broadcast::channel(256);
    let (summary_events, _) = broadcast::channel(64);
    let agent =
        RoomSummaryAgent::start(db.clone(), summary_events.clone(), storage.clone()).await?;

    let state = AppState {
        db,
        agent,
        hook_events,
        summary_events,
        storage,
        public_api_url: cli.public_api_url,
        public_app_url: cli.public_app_url,
    };

    let app = Router::new()
        // ── Room management ──────────────────────────────
        .route("/v1/rooms", post(api::create_room))
        // ── Room-scoped routes ───────────────────────────
        .route("/r/{room_id}", get(api::get_room))
        .route("/r/{room_id}/feed", get(api::get_feed))
        .route("/r/{room_id}/feed/stream", get(api::stream_feed))
        .route("/r/{room_id}/hooks/turn", post(api::ingest_hook_turn))
        .route("/r/{room_id}/summary", get(api::get_manager_summary))
        // ── Health ───────────────────────────────────────
        .route("/health", get(api::health))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
                .allow_headers([
                    header::CONTENT_TYPE,
                    HeaderName::from_static("last-event-id"),
                ]),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(cli.bind).await?;
    println!("coordination-server listening on http://{}", cli.bind);
    axum::serve(listener, app).await?;
    Ok(())
}
