use std::{
    env, fs,
    net::SocketAddr,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow};
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use clap::Parser;
use reporter_protocol::{
    CurrentReportResponse, IngestResponse, ProgressNote, ReportState, StoredProgressNote,
};
use serde_json::{Value, json};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Ingest progress notes and maintain a rolling report"
)]
struct Cli {
    #[arg(long, default_value = "127.0.0.1:8787")]
    bind: SocketAddr,
    #[arg(long, default_value = "data")]
    data_dir: PathBuf,
}

#[derive(Clone)]
struct AppState {
    data_dir: PathBuf,
    openai_api_key: String,
    openai_model: String,
    openai_base_url: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    initialize_data_dirs(&cli.data_dir)?;
    let openai_api_key = env::var("OPENAI_API_KEY").context("OPENAI_API_KEY is required")?;

    let state = AppState {
        data_dir: cli.data_dir,
        openai_api_key,
        openai_model: env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-5".to_owned()),
        openai_base_url: env::var("OPENAI_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1/responses".to_owned()),
    };

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/progress", post(ingest_progress))
        .route("/v1/report/current", get(get_current_report))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(cli.bind).await?;
    println!("coordination-server listening on http://{}", cli.bind);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn healthz() -> &'static str {
    "ok"
}

async fn ingest_progress(
    State(state): State<AppState>,
    Json(note): Json<ProgressNote>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let received_at = now_rfc3339();
    let stored = StoredProgressNote::new(note, received_at.clone());

    let mut report = read_or_initialize_report_state(&state.data_dir).map_err(internal_error)?;
    let markdown = write_report(&state, &report, &stored)
        .await
        .map_err(internal_error)?;

    persist_note(&state.data_dir, &stored).map_err(internal_error)?;
    report.notes_ingested += 1;
    report.updated_at = received_at;
    report.markdown = markdown;
    write_report_state(&state.data_dir, &report).map_err(internal_error)?;

    Ok((
        StatusCode::ACCEPTED,
        Json(IngestResponse {
            note_id: stored.note_id,
            updated_at: report.updated_at,
            notes_ingested: report.notes_ingested,
        }),
    ))
}

async fn get_current_report(
    State(state): State<AppState>,
) -> Result<Json<CurrentReportResponse>, (StatusCode, String)> {
    let report = read_existing_report_state(&state.data_dir).map_err(internal_error)?;
    Ok(Json(CurrentReportResponse {
        updated_at: report.updated_at,
        notes_ingested: report.notes_ingested,
        markdown: report.markdown,
    }))
}

async fn write_report(
    state: &AppState,
    report: &ReportState,
    stored: &StoredProgressNote,
) -> Result<String> {
    let instructions = "You maintain a single rolling Markdown progress report across all employees. Rewrite the report to incorporate the newest progress note. Keep it concise, concrete, and cumulative. Use these sections in order: Summary, Current Focus, Employees, Recent Updates. Under Employees, list active employees and their current work. Under Recent Updates, keep the most recent entries first and always attribute each update to an employee. Do not mention missing data or speculate beyond the provided notes.";
    let input = format!(
        "Current report:\n{}\n\nNew note:\n{}",
        if report.markdown.trim().is_empty() {
            "No report yet."
        } else {
            &report.markdown
        },
        render_note_for_prompt(stored)
    );

    let body = json!({
        "model": state.openai_model,
        "instructions": instructions,
        "input": input,
        "store": false
    });

    let client = reqwest::Client::new();
    let response = client
        .post(&state.openai_base_url)
        .bearer_auth(&state.openai_api_key)
        .json(&body)
        .send()
        .await
        .context("failed to call OpenAI Responses API")?
        .error_for_status()
        .context("OpenAI Responses API returned an error")?;

    let value: Value = response
        .json()
        .await
        .context("invalid OpenAI response JSON")?;
    extract_output_text(&value)
        .ok_or_else(|| anyhow!("OpenAI response did not contain output text"))
}

fn extract_output_text(value: &Value) -> Option<String> {
    value
        .get("output_text")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            value
                .pointer("/output/0/content/0/text")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
}

fn render_note_for_prompt(stored: &StoredProgressNote) -> String {
    format!(
        "employee_name: {}\nreceived_at: {}\nhost: {}\nkind: {}\nworkspace: {}\nbranch: {}\nprogress_text:\n{}",
        stored.note.employee_name,
        stored.received_at,
        stored.note.host,
        stored.note.kind,
        stored.note.workspace,
        stored.note.branch.as_deref().unwrap_or(""),
        stored.note.progress_text
    )
}

fn initialize_data_dirs(data_dir: &Path) -> Result<()> {
    fs::create_dir_all(data_dir.join("notes"))
        .with_context(|| format!("failed to create {}", data_dir.join("notes").display()))?;
    fs::create_dir_all(data_dir.join("report"))
        .with_context(|| format!("failed to create {}", data_dir.join("report").display()))?;
    Ok(())
}

fn persist_note(data_dir: &Path, stored: &StoredProgressNote) -> Result<PathBuf> {
    let notes_dir = data_dir.join("notes");
    let path = notes_dir.join(format!(
        "{}-{}.json",
        safe_fs_segment(&stored.note.employee_name),
        stored.note_id
    ));
    fs::write(&path, serde_json::to_vec_pretty(stored)?)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(path)
}

fn read_or_initialize_report_state(data_dir: &Path) -> Result<ReportState> {
    let path = report_state_path(data_dir);
    if !path.exists() {
        return Ok(ReportState::empty());
    }

    let bytes = fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(serde_json::from_slice(&bytes)
        .with_context(|| format!("invalid report state in {}", path.display()))?)
}

fn read_existing_report_state(data_dir: &Path) -> Result<ReportState> {
    let path = report_state_path(data_dir);
    if !path.exists() {
        return Err(anyhow!("no global report found"));
    }

    let bytes = fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(serde_json::from_slice(&bytes)
        .with_context(|| format!("invalid report state in {}", path.display()))?)
}

fn write_report_state(data_dir: &Path, report: &ReportState) -> Result<()> {
    let report_dir = data_dir.join("report");
    fs::create_dir_all(&report_dir)
        .with_context(|| format!("failed to create {}", report_dir.display()))?;

    let path = report_state_path(data_dir);
    fs::write(&path, serde_json::to_vec_pretty(report)?)
        .with_context(|| format!("failed to write {}", path.display()))?;

    let markdown_path = report_markdown_path(data_dir);
    fs::write(&markdown_path, &report.markdown)
        .with_context(|| format!("failed to write {}", markdown_path.display()))?;
    Ok(())
}

fn report_state_path(data_dir: &Path) -> PathBuf {
    data_dir.join("report").join("current.json")
}

fn report_markdown_path(data_dir: &Path) -> PathBuf {
    data_dir.join("report").join("current.md")
}

fn safe_fs_segment(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => ch,
            _ => '_',
        })
        .collect();

    if cleaned.is_empty() {
        "unknown".to_owned()
    } else {
        cleaned
    }
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| OffsetDateTime::now_utc().unix_timestamp().to_string())
}

fn internal_error(error: anyhow::Error) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}
