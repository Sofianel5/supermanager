mod agent;
mod auth;
mod routes;
mod state;
mod store;
mod util;

use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use anyhow::Result;
use axum::{
    Router,
    http::{HeaderName, HeaderValue, Method, header},
    routing::{get, post},
};
use clap::Parser;
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;

use agent::RoomSummaryAgent;
use auth::AuthConfig;
use state::{AppState, StoragePaths};
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
        auth: AuthConfig::from_env()?,
    };

    let app_origin = HeaderValue::from_str(state.public_app_url.trim_end_matches('/'))?;

    let app = Router::new()
        .route("/v1/auth/config", get(routes::auth_config))
        .route("/v1/me", get(routes::current_user))
        .route("/v1/auth/cli/refresh", post(routes::refresh_cli_token))
        .route("/v1/invites/accept", post(routes::accept_invite))
        // ── Room management ──────────────────────────────
        .route("/v1/rooms", post(routes::create_room))
        // ── Room-scoped routes ───────────────────────────
        .route("/r/{room_id}", get(routes::get_room))
        .route("/r/{room_id}/feed", get(routes::get_feed))
        .route("/r/{room_id}/feed/stream", get(routes::stream_feed))
        .route("/r/{room_id}/hooks/turn", post(routes::ingest_hook_turn))
        .route("/r/{room_id}/summary", get(routes::get_manager_summary))
        .route("/r/{room_id}/invites/link", post(routes::create_link_invite))
        .route("/r/{room_id}/invites/email", post(routes::create_email_invite))
        // ── Health ───────────────────────────────────────
        .route("/health", get(routes::health))
        .layer(
            CorsLayer::new()
                .allow_origin(app_origin)
                .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
                .allow_headers([
                    header::CONTENT_TYPE,
                    header::AUTHORIZATION,
                    HeaderName::from_static("last-event-id"),
                ]),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(cli.bind).await?;
    println!("coordination-server listening on http://{}", cli.bind);
    axum::serve(listener, app).await?;
    Ok(())
}
