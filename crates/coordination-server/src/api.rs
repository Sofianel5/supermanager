use std::{convert::Infallible, sync::Arc, time::Duration};

use anyhow::{Context, bail};
use async_stream::stream;
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{
        IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
};
use reporter_protocol::{
    CreateRoomRequest, CreateRoomResponse, FeedResponse, HookTurnReport, IngestResponse,
    StoredHookEvent,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::broadcast;

use crate::store::Db;

// ── Shared state ────────────────────────────────────────────

#[derive(Clone)]
pub struct HookFeedEvent {
    pub room_id: String,
    pub event: StoredHookEvent,
}

#[derive(Clone)]
pub struct SummaryStatusEvent {
    pub room_id: String,
    pub status: String, // "generating", "ready", "error"
}

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Db>,
    pub hook_events: broadcast::Sender<HookFeedEvent>,
    pub summary_events: broadcast::Sender<SummaryStatusEvent>,
    pub base_url: String,
    pub cli_install_command: String,
    pub http: reqwest::Client,
    pub openai_api_key: Option<String>,
}

// ── Query params ────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SecretQuery {
    pub secret: Option<String>,
}

// ── Helper: extract secret from header or query ─────────────

fn extract_secret(headers: &HeaderMap, query: &SecretQuery) -> Option<String> {
    // Try Authorization: Bearer <secret> first
    if let Some(auth) = headers.get(header::AUTHORIZATION) {
        if let Ok(val) = auth.to_str() {
            if let Some(token) = val.strip_prefix("Bearer ") {
                let token = token.trim();
                if !token.is_empty() {
                    return Some(token.to_owned());
                }
            }
        }
    }
    // Fall back to query param
    query.secret.clone().filter(|s| !s.is_empty())
}

// ── Health ──────────────────────────────────────────────────

pub async fn health() -> &'static str {
    "ok"
}

// ── Landing page ───────────────────────────────────────────

pub async fn landing_page(State(state): State<AppState>) -> impl IntoResponse {
    let base = html_escape(&state.base_url);
    let cli_install_command = html_escape(&state.cli_install_command);
    let html = format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>supermanager</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400;500;600;700&family=Outfit:wght@400;700;800&display=swap" rel="stylesheet">
<style>
*,*::before,*::after{{box-sizing:border-box;margin:0;padding:0}}
:root{{--bg-deep:#06080d;--bg-primary:#0a0e17;--bg-surface:#0f1420;--bg-elevated:#141a27;--border:#1a2235;--border-hover:#243049;--text-primary:#e2e8f0;--text-secondary:#7a8ba8;--text-muted:#4a5568;--amber:#f59e0b;--amber-dim:#b27308;--amber-glow:rgba(245,158,11,0.12);--emerald:#10b981;--emerald-dim:#0a7c56;--red:#ef4444;--mono:'JetBrains Mono',monospace;--sans:'Outfit',sans-serif;}}
body{{background:var(--bg-deep);color:var(--text-primary);font-family:var(--sans);line-height:1.6;min-height:100vh;position:relative;overflow-x:hidden;}}
body::before{{content:'';position:fixed;inset:0;z-index:0;pointer-events:none;background:radial-gradient(ellipse 80% 50% at 50% -20%,rgba(245,158,11,0.06) 0%,transparent 60%),radial-gradient(ellipse 60% 40% at 80% 100%,rgba(16,185,129,0.04) 0%,transparent 50%);}}
body::after{{content:'';position:fixed;inset:0;z-index:0;pointer-events:none;opacity:0.035;background-image:url("data:image/svg+xml,%3Csvg width='60' height='60' xmlns='http://www.w3.org/2000/svg'%3E%3Cdefs%3E%3Cpattern id='g' width='60' height='60' patternUnits='userSpaceOnUse'%3E%3Cpath d='M60 0H0v60' fill='none' stroke='%23fff' stroke-width='0.3'/%3E%3C/pattern%3E%3C/defs%3E%3Crect fill='url(%23g)' width='100%25' height='100%25'/%3E%3C/svg%3E");}}
.shell{{position:relative;z-index:1;max-width:720px;margin:0 auto;padding:48px 20px 80px}}
.header{{margin-bottom:40px;text-align:center}}
.logo{{display:inline-block;font-family:var(--mono);font-weight:700;font-size:0.65rem;letter-spacing:0.15em;text-transform:uppercase;color:var(--amber);background:var(--amber-glow);border:1px solid rgba(245,158,11,0.2);padding:4px 10px;border-radius:4px;margin-bottom:14px;text-decoration:none;}}
.logo:hover{{background:rgba(245,158,11,0.18);}}
h1{{font-family:var(--sans);font-weight:800;font-size:2.6rem;letter-spacing:-0.03em;color:var(--text-primary);line-height:1.1;margin-bottom:10px;}}
.tagline{{font-family:var(--sans);font-size:1.05rem;color:var(--text-secondary);}}
.panel{{background:var(--bg-surface);border:1px solid var(--border);border-radius:10px;margin-bottom:24px;overflow:hidden;transition:border-color 0.2s;}}
.panel:hover{{border-color:var(--border-hover)}}
.panel-head{{display:flex;align-items:center;justify-content:space-between;padding:16px 20px;border-bottom:1px solid var(--border);background:var(--bg-elevated);}}
.panel-title{{font-family:var(--mono);font-weight:600;font-size:0.8rem;letter-spacing:0.06em;text-transform:uppercase;color:var(--text-secondary);}}
.panel-body{{padding:20px}}
.steps{{list-style:none;padding:0;counter-reset:step;}}
.steps li{{position:relative;padding-left:36px;margin-bottom:14px;font-family:var(--sans);font-size:0.92rem;color:var(--text-secondary);line-height:1.6;}}
.steps li::before{{content:counter(step);counter-increment:step;position:absolute;left:0;top:1px;width:24px;height:24px;border-radius:50%;background:var(--amber-glow);border:1px solid rgba(245,158,11,0.25);color:var(--amber);font-family:var(--mono);font-size:0.72rem;font-weight:600;display:flex;align-items:center;justify-content:center;}}
.steps li:last-child{{margin-bottom:0}}
.steps code{{font-family:var(--mono);font-size:0.82rem;color:var(--amber);background:var(--bg-deep);padding:2px 6px;border-radius:4px;}}
label{{display:block;font-family:var(--mono);font-weight:500;font-size:0.78rem;letter-spacing:0.06em;text-transform:uppercase;color:var(--text-secondary);margin-bottom:8px;}}
input[type="text"]{{width:100%;padding:12px 14px;background:var(--bg-deep);border:1px solid var(--border);border-radius:6px;color:var(--text-primary);font-family:var(--sans);font-size:1rem;outline:none;transition:border-color 0.2s;}}
input[type="text"]:focus{{border-color:var(--amber)}}
input[type="text"]::placeholder{{color:var(--text-muted)}}
button{{margin-top:16px;padding:12px 28px;background:var(--amber);border:none;border-radius:6px;color:var(--bg-deep);font-family:var(--mono);font-size:0.85rem;font-weight:700;letter-spacing:0.04em;text-transform:uppercase;cursor:pointer;transition:background 0.2s, transform 0.1s;}}
button:hover{{background:#d97706}}
button:active{{transform:scale(0.98)}}
button:disabled{{opacity:0.5;cursor:not-allowed}}
#result{{display:none;margin-top:24px;padding-top:20px;border-top:1px solid var(--border);}}
.field-label{{font-family:var(--mono);font-size:0.7rem;letter-spacing:0.06em;text-transform:uppercase;color:var(--text-muted);margin:16px 0 6px 0;}}
.field-label:first-child{{margin-top:0}}
.field-value{{display:block;font-family:var(--mono);font-size:0.78rem;color:var(--amber);background:var(--bg-deep);border:1px solid var(--border);padding:12px 14px;border-radius:6px;white-space:pre-wrap;word-break:break-all;cursor:pointer;transition:border-color 0.2s;position:relative;}}
.field-value:hover{{border-color:var(--amber-dim)}}
.field-value::after{{content:'click to copy';position:absolute;right:12px;top:50%;transform:translateY(-50%);font-size:0.65rem;color:var(--text-muted);letter-spacing:0.04em;text-transform:uppercase;opacity:0;transition:opacity 0.2s;}}
.field-value:hover::after{{opacity:1}}
.field-value.copied{{border-color:var(--emerald)}}
.field-value.copied::after{{content:'copied!';color:var(--emerald);opacity:1}}
.field-link{{color:var(--cyan);text-decoration:none;font-family:var(--mono);font-size:0.85rem;}}
.field-link:hover{{text-decoration:underline}}
.field-value.val-roomid{{color:var(--emerald)}}
.field-value.val-roomid:hover{{border-color:var(--emerald-dim)}}
.field-value.val-secret{{color:var(--violet)}}
.field-value.val-secret:hover{{border-color:var(--violet)}}
#error{{display:none;margin-top:12px;color:var(--red);font-family:var(--mono);font-size:0.82rem;}}
.footer{{margin-top:40px;padding-top:20px;border-top:1px solid var(--border);text-align:center;font-family:var(--mono);font-size:0.68rem;color:var(--text-muted);letter-spacing:0.06em;}}
.footer a{{color:var(--text-secondary);text-decoration:none}}
.footer a:hover{{color:var(--text-primary)}}
@media(max-width:600px){{.shell{{padding:32px 14px 60px}}h1{{font-size:2rem}}.panel-body{{padding:14px}}}}
</style>
</head>
<body>
<div class="shell">
  <div class="header">
    <a href="/" class="logo">supermanager</a>
    <h1>supermanager</h1>
    <p class="tagline">Real-time visibility into what your AI coding agents are working on.</p>
  </div>

  <div class="panel">
    <div class="panel-head">
      <span class="panel-title">How It Works</span>
    </div>
    <div class="panel-body">
      <ol class="steps">
        <li>Create a room for your team</li>
        <li>Install the <code>supermanager</code> CLI once on each machine</li>
        <li>Run the room join command in each repo you want connected</li>
        <li>AI agents (<code>Claude Code</code>, <code>Codex</code>) automatically report progress as they work</li>
        <li>Watch it all on a live dashboard</li>
      </ol>
    </div>
  </div>

  <div class="panel">
    <div class="panel-head">
      <span class="panel-title">Install The CLI Once</span>
    </div>
    <div class="panel-body">
      <div class="field-value" id="cli-install-hint">{cli_install_command}</div>
    </div>
  </div>

  <div class="panel">
    <div class="panel-head">
      <span class="panel-title">Create a Room</span>
    </div>
    <div class="panel-body">
      <form id="create-form">
        <label for="room-name">Team / Room Name</label>
        <input type="text" id="room-name" name="name" placeholder="e.g. My Team" required>
        <button type="submit" id="submit-btn">Create Room</button>
      </form>
      <div id="error"></div>
      <div id="result">
        <div class="field-label">Dashboard</div>
        <div><a id="res-dashboard" class="field-link" href="#" target="_blank"></a></div>
        <div class="field-label">Install CLI</div>
        <div class="field-value" id="res-cli-install"></div>
        <div class="field-label">Join command</div>
        <div class="field-value" id="res-join"></div>
        <div class="field-label">Room ID</div>
        <div class="field-value val-roomid" id="res-room-id"></div>
        <div class="field-label">Secret</div>
        <div class="field-value val-secret" id="res-secret"></div>
      </div>
    </div>
  </div>

  <div class="footer">supermanager &middot; real-time ai coordination</div>
</div>

<script>
(function(){{
  var form = document.getElementById('create-form');
  var btn = document.getElementById('submit-btn');
  var errorEl = document.getElementById('error');
  var resultEl = document.getElementById('result');

  form.addEventListener('submit', function(e) {{
    e.preventDefault();
    errorEl.style.display = 'none';
    resultEl.style.display = 'none';
    btn.disabled = true;
    btn.textContent = 'CREATING\u2026';

    var name = document.getElementById('room-name').value.trim();
    fetch('{base}/v1/rooms', {{
      method: 'POST',
      headers: {{ 'Content-Type': 'application/json' }},
      body: JSON.stringify({{ name: name }})
    }})
    .then(function(r) {{
      if (!r.ok) throw new Error('Server returned ' + r.status);
      return r.json();
    }})
    .then(function(data) {{
      document.getElementById('res-dashboard').href = data.dashboard_url;
      document.getElementById('res-dashboard').textContent = data.dashboard_url;
      document.getElementById('res-cli-install').textContent = data.install_command;
      document.getElementById('res-join').textContent = data.join_command;
      document.getElementById('res-room-id').textContent = data.room_id;
      document.getElementById('res-secret').textContent = data.secret;
      resultEl.style.display = 'block';
    }})
    .catch(function(err) {{
      errorEl.textContent = err.message;
      errorEl.style.display = 'block';
    }})
    .finally(function() {{
      btn.disabled = false;
      btn.textContent = 'CREATE ROOM';
    }});
  }});

  document.addEventListener('click', function(e) {{
    var el = e.target.closest('.field-value');
    if (!el) return;
    navigator.clipboard.writeText(el.textContent).then(function() {{
      el.classList.add('copied');
      setTimeout(function() {{ el.classList.remove('copied'); }}, 2000);
    }});
  }});
}})();
</script>
</body>
</html>"##,
        base = base,
        cli_install_command = cli_install_command,
    );

    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], html)
}

// ── Room management ─────────────────────────────────────────

pub async fn create_room(
    State(state): State<AppState>,
    Json(req): Json<CreateRoomRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let room = state.db.create_room(&req.name).map_err(internal_error)?;
    let resp = CreateRoomResponse {
        install_command: state.cli_install_command.clone(),
        dashboard_url: format!("{}/r/{}", state.base_url, room.room_id),
        join_command: cli_join_command(&state.base_url, &room.room_id, &room.secret),
        room_id: room.room_id,
        secret: room.secret,
    };
    Ok((StatusCode::CREATED, Json(resp)))
}

// ── Room-scoped routes ──────────────────────────────────────

pub async fn ingest_hook_turn(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<SecretQuery>,
    Json(report): Json<HookTurnReport>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let secret = extract_secret(&headers, &query)
        .ok_or((StatusCode::UNAUTHORIZED, "missing secret".to_owned()))?;

    let valid = state
        .db
        .verify_room_secret(&room_id, &secret)
        .map_err(internal_error)?;
    if !valid {
        return Err((StatusCode::UNAUTHORIZED, "invalid secret".to_owned()));
    }

    let stored = state
        .db
        .insert_hook_event(&room_id, &report)
        .map_err(internal_error)?;
    let event_id = stored.event_id;
    let received_at = stored.received_at.clone();

    let _ = state.hook_events.send(HookFeedEvent {
        room_id: room_id.clone(),
        event: stored,
    });

    spawn_auto_summarize(&state, &room_id);

    Ok((
        StatusCode::ACCEPTED,
        Json(IngestResponse {
            event_id,
            received_at,
        }),
    ))
}

pub async fn get_feed(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<FeedResponse>, (StatusCode, String)> {
    let room = state.db.get_room(&room_id).map_err(internal_error)?;
    if room.is_none() {
        return Err((StatusCode::NOT_FOUND, format!("room not found: {room_id}")));
    }
    let events = state.db.get_hook_events(&room_id).map_err(internal_error)?;
    Ok(Json(FeedResponse { events }))
}

pub async fn stream_feed(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
) -> Result<Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)>
{
    let replay = headers
        .get("last-event-id")
        .and_then(|value| value.to_str().ok())
        .map(|event_id| state.db.get_hook_events_after(&room_id, event_id))
        .transpose()
        .map_err(internal_error)?
        .unwrap_or_default();

    let mut hook_rx = state.hook_events.subscribe();
    let mut summary_rx = state.summary_events.subscribe();
    let target_room = room_id.clone();

    // Send initial summary status
    let initial_status = state
        .db
        .get_summary_status(&room_id)
        .unwrap_or_else(|_| "ready".to_owned());

    let event_stream = stream! {
        // Replay missed events
        for event in replay {
            yield Ok(hook_event(&event));
        }

        // Send current summary status on connect
        yield Ok(Event::default()
            .event("summary_status")
            .data(json!({ "status": initial_status }).to_string()));

        loop {
            tokio::select! {
                hook_result = hook_rx.recv() => {
                    match hook_result {
                        Ok(evt) => {
                            if evt.room_id == target_room {
                                yield Ok(hook_event(&evt.event));
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            yield Ok(Event::default()
                                .event("warning")
                                .data(format!("lagged:{skipped}")));
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
                summary_result = summary_rx.recv() => {
                    match summary_result {
                        Ok(evt) => {
                            if evt.room_id == target_room {
                                yield Ok(Event::default()
                                    .event("summary_status")
                                    .data(json!({ "status": evt.status }).to_string()));
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {}
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    };

    Ok(Sse::new(event_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

pub async fn get_manager_summary(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let summary = state.db.get_summary(&room_id).map_err(internal_error)?;
    Ok((
        [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
        summary,
    ))
}

pub async fn get_tasks_http(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let tasks = state
        .db
        .get_tasks(&room_id, false)
        .map_err(internal_error)?;
    Ok(Json(json!({ "tasks": tasks })))
}

// ── Dashboard ───────────────────────────────────────────────

pub async fn dashboard(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let room = state.db.get_room(&room_id).map_err(internal_error)?;
    match room {
        Some(r) => {
            let base = &state.base_url;
            let html = build_dashboard_html(&r.name, &r.room_id, base, &state.cli_install_command);
            Ok(([(header::CONTENT_TYPE, "text/html; charset=utf-8")], html))
        }
        None => Err((StatusCode::NOT_FOUND, format!("room not found: {room_id}"))),
    }
}

fn build_dashboard_html(
    name: &str,
    room_id: &str,
    base_url: &str,
    cli_install_command: &str,
) -> String {
    let safe_name = html_escape(name);
    let safe_id = html_escape(room_id);
    let safe_base = html_escape(base_url);
    let safe_install_command = html_escape(cli_install_command);
    let safe_join_command = html_escape(&cli_join_command(base_url, room_id, "YOUR_SECRET"));
    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>{safe_name} — supermanager</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400;500;600;700&family=Outfit:wght@400;700;800&display=swap" rel="stylesheet">
<style>
*,*::before,*::after{{box-sizing:border-box;margin:0;padding:0}}
:root{{--bg-deep:#06080d;--bg-primary:#0a0e17;--bg-surface:#0f1420;--bg-elevated:#141a27;--border:#1a2235;--border-hover:#243049;--text-primary:#e2e8f0;--text-secondary:#7a8ba8;--text-muted:#4a5568;--amber:#f59e0b;--amber-dim:#b27308;--amber-glow:rgba(245,158,11,0.12);--emerald:#10b981;--emerald-dim:#0a7c56;--red:#ef4444;--cyan:#22d3ee;--violet:#a78bfa;--mono:'JetBrains Mono',monospace;--sans:'Outfit',sans-serif;}}
html{{scroll-behavior:smooth}}
body{{background:var(--bg-deep);color:var(--text-primary);font-family:var(--sans);line-height:1.6;min-height:100vh;position:relative;overflow-x:hidden;}}
body::before{{content:'';position:fixed;inset:0;z-index:0;pointer-events:none;background:radial-gradient(ellipse 80% 50% at 50% -20%,rgba(245,158,11,0.06) 0%,transparent 60%),radial-gradient(ellipse 60% 40% at 80% 100%,rgba(16,185,129,0.04) 0%,transparent 50%);}}
body::after{{content:'';position:fixed;inset:0;z-index:0;pointer-events:none;opacity:0.035;background-image:url("data:image/svg+xml,%3Csvg width='60' height='60' xmlns='http://www.w3.org/2000/svg'%3E%3Cdefs%3E%3Cpattern id='g' width='60' height='60' patternUnits='userSpaceOnUse'%3E%3Cpath d='M60 0H0v60' fill='none' stroke='%23fff' stroke-width='0.3'/%3E%3C/pattern%3E%3C/defs%3E%3Crect fill='url(%23g)' width='100%25' height='100%25'/%3E%3C/svg%3E");}}
.shell{{position:relative;z-index:1;max-width:920px;margin:0 auto;padding:32px 20px 80px}}
.header{{margin-bottom:40px}}
.header-top{{display:flex;align-items:center;gap:14px;margin-bottom:6px}}
.logo{{font-family:var(--mono);font-weight:700;font-size:0.65rem;letter-spacing:0.15em;text-transform:uppercase;color:var(--amber);background:var(--amber-glow);border:1px solid rgba(245,158,11,0.2);padding:4px 10px;border-radius:4px;white-space:nowrap;text-decoration:none;}}
.logo:hover{{background:rgba(245,158,11,0.18);}}
.room-name{{font-family:var(--sans);font-weight:800;font-size:2rem;letter-spacing:-0.03em;color:var(--text-primary);line-height:1.1;}}
.header-meta{{display:flex;align-items:center;gap:16px;font-family:var(--mono);font-size:0.78rem;color:var(--text-muted);}}
.room-id{{color:var(--text-secondary)}}
.live-dot{{display:inline-flex;align-items:center;gap:6px;font-family:var(--mono);font-weight:600;font-size:0.72rem;letter-spacing:0.08em;text-transform:uppercase;}}
.live-dot .dot{{width:7px;height:7px;border-radius:50%;background:var(--text-muted);box-shadow:0 0 0 0 transparent;transition:all 0.4s ease;}}
.live-dot.connected .dot{{background:var(--emerald);box-shadow:0 0 8px 2px rgba(16,185,129,0.4);animation:pulse 2s ease-in-out infinite;}}
.live-dot.connected{{color:var(--emerald)}}
.live-dot.error .dot{{background:var(--red)}}
.live-dot.error{{color:var(--red)}}
@keyframes pulse{{0%,100%{{box-shadow:0 0 8px 2px rgba(16,185,129,0.4)}}50%{{box-shadow:0 0 14px 4px rgba(16,185,129,0.2)}}}}
.panel{{background:var(--bg-surface);border:1px solid var(--border);border-radius:10px;margin-bottom:24px;overflow:hidden;transition:border-color 0.2s;}}
.panel:hover{{border-color:var(--border-hover)}}
.panel-head{{display:flex;align-items:center;justify-content:space-between;padding:16px 20px;border-bottom:1px solid var(--border);background:var(--bg-elevated);}}
.panel-title{{font-family:var(--mono);font-weight:600;font-size:0.8rem;letter-spacing:0.06em;text-transform:uppercase;color:var(--text-secondary);}}
.panel-badge{{font-family:var(--mono);font-size:0.72rem;font-weight:500;color:var(--text-muted);background:var(--bg-primary);padding:2px 10px;border-radius:20px;border:1px solid var(--border);}}
.panel-body{{padding:20px}}
.summary-content{{font-family:var(--sans);font-size:0.92rem;color:var(--text-secondary);white-space:pre-wrap;line-height:1.7;}}
.empty{{color:var(--text-muted);font-style:italic;font-size:0.88rem}}
.generating{{color:var(--accent);font-style:italic;font-size:0.88rem;animation:pulse 1.5s ease-in-out infinite}}
.task-item{{display:flex;align-items:center;gap:10px;padding:8px 0;border-bottom:1px solid var(--border);font-family:var(--sans);font-size:0.88rem;color:var(--text-secondary)}}
.task-item:last-child{{border-bottom:none}}
.task-status{{font-family:var(--mono);font-size:0.72rem;font-weight:600;padding:2px 8px;border-radius:10px;text-transform:uppercase;letter-spacing:0.03em}}
.task-status.todo{{color:var(--text-muted);border:1px solid var(--border)}}
.task-status.in_progress{{color:var(--accent);border:1px solid var(--accent)}}
.task-status.done{{color:var(--emerald);border:1px solid var(--emerald)}}
.task-assignee{{color:var(--text-muted);font-size:0.78rem}}
.task-list-empty{{color:var(--text-muted);font-style:italic;font-size:0.88rem}}
@keyframes pulse{{0%,100%{{opacity:1}}50%{{opacity:0.5}}}}
.timeline{{position:relative;padding-left:24px}}
.timeline::before{{content:'';position:absolute;left:7px;top:8px;bottom:8px;width:1px;background:var(--border);}}
.note{{position:relative;padding:16px 18px;margin-bottom:16px;background:var(--bg-primary);border:1px solid var(--border);border-radius:8px;transition:border-color 0.2s, transform 0.2s;animation:noteIn 0.35s ease-out both;}}
.note:hover{{border-color:var(--border-hover);transform:translateX(2px)}}
.note::before{{content:'';position:absolute;left:-21px;top:22px;width:9px;height:9px;border-radius:50%;background:var(--bg-surface);border:2px solid var(--amber-dim);z-index:1;}}
.note:first-child::before{{background:var(--amber);border-color:var(--amber);box-shadow:0 0 10px 2px var(--amber-glow);}}
@keyframes noteIn{{from{{opacity:0;transform:translateY(8px) translateX(-4px)}}to{{opacity:1;transform:translateY(0) translateX(0)}}}}
.note-row-top{{display:flex;align-items:baseline;justify-content:space-between;gap:8px;margin-bottom:6px;flex-wrap:wrap;}}
.note-author{{font-family:var(--sans);font-weight:700;font-size:0.95rem;color:var(--text-primary);}}
.note-time{{font-family:var(--mono);font-size:0.72rem;color:var(--text-muted);letter-spacing:0.02em;cursor:default;}}
.note-time:hover{{color:var(--text-secondary)}}
.note-repo-line{{display:flex;align-items:center;gap:6px;font-family:var(--mono);font-size:0.76rem;margin-bottom:8px;flex-wrap:wrap;}}
.note-repo{{color:var(--cyan);font-weight:500}}
.note-branch{{color:var(--violet);font-weight:400}}
.note-sep{{color:var(--text-muted);font-size:0.7rem}}
.note-text{{color:var(--text-secondary);font-family:var(--mono);font-size:0.78rem;white-space:pre-wrap;line-height:1.6;overflow-x:auto;}}
.join-label{{font-family:var(--sans);font-size:0.85rem;color:var(--text-muted);margin-bottom:10px;}}
.join-cmd{{display:block;font-family:var(--mono);font-size:0.78rem;color:var(--amber);background:var(--bg-deep);border:1px solid var(--border);padding:14px 16px;border-radius:6px;white-space:pre-wrap;word-break:break-all;cursor:pointer;transition:border-color 0.2s, background 0.2s;position:relative;}}
.join-cmd:hover{{border-color:var(--amber-dim);background:rgba(245,158,11,0.04)}}
.join-cmd::after{{content:'click to copy';position:absolute;right:12px;top:50%;transform:translateY(-50%);font-size:0.65rem;color:var(--text-muted);letter-spacing:0.04em;text-transform:uppercase;opacity:0;transition:opacity 0.2s;}}
.join-cmd:hover::after{{opacity:1}}
.join-cmd.copied{{border-color:var(--emerald)}}
.join-cmd.copied::after{{content:'copied!';color:var(--emerald);opacity:1}}
.toggle-btn{{display:none;width:100%;margin-top:12px;padding:8px;background:var(--bg-elevated);border:1px solid var(--border);border-radius:6px;color:var(--text-muted);font-family:var(--mono);font-size:0.72rem;letter-spacing:0.06em;text-transform:uppercase;cursor:pointer;transition:color 0.2s,border-color 0.2s;}}
.toggle-btn:hover{{color:var(--text-secondary);border-color:var(--border-hover)}}
.footer{{margin-top:40px;padding-top:20px;border-top:1px solid var(--border);text-align:center;font-family:var(--mono);font-size:0.68rem;color:var(--text-muted);letter-spacing:0.06em;}}
@media(max-width:600px){{.shell{{padding:20px 14px 60px}}.room-name{{font-size:1.5rem}}.panel-body{{padding:14px}}.note{{padding:12px 14px}}.timeline{{padding-left:20px}}}}
</style>
</head>
<body>
<div class="shell">
  <div class="header">
    <div class="header-top">
      <a href="/" class="logo">supermanager</a>
    </div>
    <h1 class="room-name">{safe_name}</h1>
    <div class="header-meta">
      <span class="room-id">{safe_id}</span>
      <div id="connection-status" class="live-dot error">
        <div class="dot"></div>
        <span class="live-text">connecting</span>
      </div>
    </div>
  </div>

  <div class="panel">
    <div class="panel-head">
      <span class="panel-title">Connect Agents</span>
    </div>
    <div class="panel-body">
      <p class="join-label">Install the CLI once on each machine, then run the room join command in every repo you want connected.</p>
      <code id="install-cmd" class="join-cmd">{safe_install_command}</code>
      <p class="join-label" style="margin-top:16px">The room creator has the full join command with the secret. Ask them for it if you only have the room URL.</p>
      <code id="join-cmd" class="join-cmd">{safe_join_command}</code>
    </div>
  </div>

  <div class="panel">
    <div class="panel-head">
      <span class="panel-title">Task List</span>
      <span id="task-count" class="panel-badge">0 tasks</span>
    </div>
    <div class="panel-body">
      <div id="task-list" class="task-list-container"></div>
    </div>
  </div>

  <div class="panel">
    <div class="panel-head">
      <span class="panel-title">Manager Summary</span>
    </div>
    <div class="panel-body">
      <div id="summary" class="summary-content empty">No summary yet.</div>
    </div>
  </div>

  <div class="panel">
    <div class="panel-head">
      <span class="panel-title">Activity Feed</span>
      <span id="note-count" class="panel-badge">0 updates</span>
    </div>
    <div class="panel-body">
      <div id="feed" class="timeline"></div>
      <button id="toggle-feed" class="toggle-btn"></button>
    </div>
  </div>

  <div class="footer">supermanager &middot; real-time ai coordination</div>
</div>

<script>
(function(){{
  var feed = document.getElementById('feed');
  var statusEl = document.getElementById('connection-status');
  var liveText = statusEl.querySelector('.live-text');
  var countEl = document.getElementById('note-count');
  var taskList = document.getElementById('task-list');
  var taskCount = document.getElementById('task-count');
  var summaryEl = document.getElementById('summary');
  var events = [];
  var expanded = false;
  var FEED_LIMIT = 10;
  var toggleBtn = document.getElementById('toggle-feed');

  function el(tag, cls, text) {{
    var e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text) e.textContent = text;
    return e;
  }}

  function timeAgo(iso) {{
    try {{
      var d = new Date(iso);
      var now = Date.now();
      var diff = Math.floor((now - d.getTime()) / 1000);
      if (diff < 5) return 'just now';
      if (diff < 60) return diff + 's ago';
      if (diff < 3600) return Math.floor(diff / 60) + 'm ago';
      if (diff < 86400) return Math.floor(diff / 3600) + 'h ago';
      return Math.floor(diff / 86400) + 'd ago';
    }} catch(e) {{
      return iso;
    }}
  }}

  function fullTime(iso) {{
    try {{ return new Date(iso).toLocaleString(); }}
    catch(e) {{ return iso; }}
  }}

  function formatPayload(payload) {{
    try {{
      return JSON.stringify(payload, null, 2);
    }} catch (e) {{
      return String(payload);
    }}
  }}

  function buildEvent(n, i) {{
    var card = el('div', 'note');
    card.style.animationDelay = (i * 0.04) + 's';

    var top = el('div', 'note-row-top');
    top.appendChild(el('span', 'note-author', n.employee_name));
    var time = el('span', 'note-time', timeAgo(n.received_at));
    time.title = fullTime(n.received_at);
    top.appendChild(time);
    card.appendChild(top);

    var repoLine = el('div', 'note-repo-line');
    repoLine.appendChild(el('span', 'note-repo', n.repo_root));
    if (n.branch) {{
      repoLine.appendChild(el('span', 'note-sep', '/'));
      repoLine.appendChild(el('span', 'note-branch', n.branch));
    }}
    repoLine.appendChild(el('span', 'note-sep', '/'));
    repoLine.appendChild(el('span', 'note-branch', n.client));
    card.appendChild(repoLine);
    card.appendChild(el('pre', 'note-text', formatPayload(n.payload)));
    return card;
  }}

  function renderFeed() {{
    feed.textContent = '';
    if (events.length === 0) {{
      feed.appendChild(el('span', 'empty', 'No updates yet.'));
      toggleBtn.style.display = 'none';
    }} else {{
      var visible = expanded ? events : events.slice(0, FEED_LIMIT);
      visible.forEach(function(n, i) {{ feed.appendChild(buildEvent(n, i)); }});
      if (events.length > FEED_LIMIT) {{
        toggleBtn.style.display = 'block';
        var hidden = events.length - FEED_LIMIT;
        toggleBtn.textContent = expanded ? 'Show less' : 'Show ' + hidden + ' more';
      }} else {{
        toggleBtn.style.display = 'none';
      }}
    }}
    var label = events.length === 1 ? '1 update' : events.length + ' updates';
    countEl.textContent = label;
  }}

  toggleBtn.addEventListener('click', function() {{
    expanded = !expanded;
    renderFeed();
  }});

  setInterval(function() {{
    var times = feed.querySelectorAll('.note-time');
    var all = events;
    times.forEach(function(t, i) {{
      if (all[i]) t.textContent = timeAgo(all[i].received_at);
    }});
  }}, 30000);

  var base = '{safe_base}/r/{safe_id}';
  fetch(base + '/feed')
    .then(function(r) {{ return r.json(); }})
    .then(function(data) {{
      if (data.events && data.events.length > 0) {{ events = data.events; }}
      renderFeed();
    }})
    .catch(function() {{ renderFeed(); }});

  function loadSummary() {{
    fetch(base + '/summary')
      .then(function(r) {{ return r.text(); }})
      .then(function(text) {{
        summaryEl.textContent = text || 'No summary yet.';
        if (!text) summaryEl.className = 'summary-content empty';
        else summaryEl.className = 'summary-content';
      }})
      .catch(function() {{}});
  }}
  loadSummary();

  function loadTasks() {{
    fetch(base + '/tasks')
      .then(function(r) {{ return r.json(); }})
      .then(function(data) {{
        var tasks = data.tasks || [];
        taskCount.textContent = tasks.length + ' task' + (tasks.length !== 1 ? 's' : '');
        if (tasks.length === 0) {{
          taskList.innerHTML = '<div class="task-list-empty">No tasks yet.</div>';
          return;
        }}
        taskList.innerHTML = '';
        tasks.forEach(function(t) {{
          var item = document.createElement('div');
          item.className = 'task-item';
          var badge = document.createElement('span');
          badge.className = 'task-status ' + t.status;
          badge.textContent = t.status.replace('_', ' ');
          item.appendChild(badge);
          var title = document.createElement('span');
          title.textContent = t.title;
          title.style.flex = '1';
          item.appendChild(title);
          if (t.assignee) {{
            var assignee = document.createElement('span');
            assignee.className = 'task-assignee';
            assignee.textContent = '@' + t.assignee;
            item.appendChild(assignee);
          }}
          taskList.appendChild(item);
        }});
      }})
      .catch(function() {{}});
  }}
  loadTasks();

  var es = new EventSource(base + '/feed/stream');
  es.onopen = function() {{
    liveText.textContent = 'live';
    statusEl.className = 'live-dot connected';
  }};
  es.addEventListener('hook_event', function(e) {{
    try {{
      var event = JSON.parse(e.data);
      events.unshift(event);
      renderFeed();
    }} catch(err) {{}}
  }});
  es.addEventListener('summary_status', function(e) {{
    try {{
      var data = JSON.parse(e.data);
      if (data.status === 'generating') {{
        summaryEl.textContent = 'Generating summary...';
        summaryEl.className = 'summary-content generating';
      }} else if (data.status === 'ready') {{
        loadSummary();
      }} else if (data.status === 'error') {{
        summaryEl.textContent = 'Summary generation failed.';
        summaryEl.className = 'summary-content error';
      }}
    }} catch(err) {{}}
  }});
  es.onerror = function() {{
    liveText.textContent = 'reconnecting';
    statusEl.className = 'live-dot error';
  }};

  document.addEventListener('click', function(e) {{
    var copyTarget = e.target.closest('.join-cmd');
    if (!copyTarget) return;
    navigator.clipboard.writeText(copyTarget.textContent).then(function() {{
      copyTarget.classList.add('copied');
      setTimeout(function() {{ copyTarget.classList.remove('copied'); }}, 2000);
    }});
  }});
}})();
</script>
</body>
</html>"##,
    )
}

/// Shared: resolve filter args → fetch hook events → format context string.
fn resolve_hook_context(
    state: &AppState,
    room_id: &str,
    args: &Value,
    default_limit: u32,
) -> anyhow::Result<(String, String)> {
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(default_limit as u64) as u32;
    let minutes = args.get("minutes").and_then(Value::as_u64);
    let employee_name = args.get("employee_name").and_then(Value::as_str);
    let branch = args.get("branch").and_then(Value::as_str);
    let since_last_update_by = args.get("since_last_update_by").and_then(Value::as_str);

    // Resolve time cutoff
    let after_time = if let Some(person) = since_last_update_by {
        state
            .db
            .get_last_hook_event_time(room_id, person)
            .with_context(|| format!("failed to look up last hook event by {person}"))?
    } else if let Some(mins) = minutes {
        let cutoff = time::OffsetDateTime::now_utc() - time::Duration::minutes(mins as i64);
        Some(
            cutoff
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
        )
    } else {
        None
    };

    let events = state
        .db
        .get_hook_events_filtered(room_id, after_time.as_deref(), employee_name, branch, limit)
        .context("failed to fetch hook events")?;

    if events.is_empty() {
        bail!("no hook updates found matching the filter");
    }

    let mut context = String::new();
    for event in &events {
        let line = json!({
            "received_at": event.received_at,
            "employee_name": event.employee_name,
            "client": event.client,
            "repo_root": event.repo_root,
            "branch": event.branch,
            "payload": event.payload,
        });
        context.push_str(&serde_json::to_string(&line).unwrap_or_default());
        context.push('\n');
    }

    let mut filter_desc = format!("{} most recent hook updates", events.len());
    if let Some(name) = employee_name {
        filter_desc = format!("{filter_desc} from {name}");
    }
    if let Some(b) = branch {
        filter_desc = format!("{filter_desc} on branch {b}");
    }
    if let Some(person) = since_last_update_by {
        filter_desc = format!("{filter_desc} (since {person}'s last update)");
    }
    if let Some(mins) = minutes {
        filter_desc = format!("{filter_desc} from the last {mins} minutes");
    }

    Ok((context, filter_desc))
}

/// Shared: call OpenAI Responses API and return the generated text.
async fn call_openai(state: &AppState, instructions: &str, input: &str) -> anyhow::Result<String> {
    let api_key = match &state.openai_api_key {
        Some(k) => k,
        None => bail!("OPENAI_API_KEY not configured on the server"),
    };

    let body = json!({
        "model": "gpt-5.4-mini",
        "instructions": instructions,
        "input": input,
    });

    eprintln!("[call_openai] sending request to OpenAI (model: gpt-5.4-mini)");
    let resp = state
        .http
        .post("https://api.openai.com/v1/responses")
        .header("Authorization", format!("Bearer {api_key}"))
        .json(&body)
        .send()
        .await;

    let resp = match resp {
        Ok(r) => r,
        Err(e) => bail!("OpenAI request failed: {e}"),
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        bail!("OpenAI returned {status}: {body_text}");
    }

    let resp_json: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => bail!("Failed to parse OpenAI response: {e}"),
    };

    Ok(resp_json
        .pointer("/output/0/content/0/text")
        .and_then(Value::as_str)
        .unwrap_or("(empty response from OpenAI)")
        .to_owned())
}

/// Background auto-summarize: triggered after every new hook event.
async fn auto_summarize(state: &AppState, room_id: &str) {
    eprintln!("[auto_summarize] starting for room {room_id}");

    // Mark as generating + broadcast
    let _ = state.db.set_summary_status(room_id, "generating");
    let _ = state.summary_events.send(SummaryStatusEvent {
        room_id: room_id.to_owned(),
        status: "generating".to_owned(),
    });

    // Build context from the most recent hook events
    let args = json!({});
    let (context, filter_desc) = match resolve_hook_context(state, room_id, &args, 100) {
        Ok(v) => v,
        Err(error) => {
            eprintln!("[auto_summarize] no hook events available for room {room_id}: {error}");
            let _ = state.db.set_summary_status(room_id, "ready");
            let _ = state.summary_events.send(SummaryStatusEvent {
                room_id: room_id.to_owned(),
                status: "ready".to_owned(),
            });
            return;
        }
    };

    eprintln!("[auto_summarize] calling OpenAI with {filter_desc}");

    let result = call_openai(
        state,
        "You are a concise project manager assistant. You will receive raw hook updates from coding agents as JSON lines. Each line includes metadata such as employee_name, client, repo_root, branch, received_at, and the original hook payload. Summarize the work into a clear, actionable briefing. Group by person or theme. Highlight blockers, completions, and key decisions. Be brief.",
        &format!("Summarize these {filter_desc}:\n\n{context}"),
    ).await;
    match result {
        Ok(text) if !text.is_empty() => {
            eprintln!(
                "[auto_summarize] success for room {room_id}, {} chars",
                text.len()
            );
            let _ = state.db.set_summary(room_id, &text);
            let _ = state.summary_events.send(SummaryStatusEvent {
                room_id: room_id.to_owned(),
                status: "ready".to_owned(),
            });
        }
        Ok(_) => {
            eprintln!("[auto_summarize] empty response for room {room_id}");
            let _ = state.db.set_summary_status(room_id, "error");
            let _ = state.summary_events.send(SummaryStatusEvent {
                room_id: room_id.to_owned(),
                status: "error".to_owned(),
            });
        }
        Err(error) => {
            eprintln!("[auto_summarize] error for room {room_id}: {error}");
            let _ = state.db.set_summary_status(room_id, "error");
            let _ = state.summary_events.send(SummaryStatusEvent {
                room_id: room_id.to_owned(),
                status: "error".to_owned(),
            });
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────

fn spawn_auto_summarize(state: &AppState, room_id: &str) {
    let bg_state = state.clone();
    let bg_room = room_id.to_owned();
    tokio::spawn(async move {
        auto_summarize(&bg_state, &bg_room).await;
    });
}

fn cli_join_command(base_url: &str, room_id: &str, secret: &str) -> String {
    format!(
        "supermanager join --server \"{}\" --room \"{}\" --secret \"{}\"",
        base_url.trim_end_matches('/'),
        room_id,
        secret,
    )
}

fn internal_error(error: anyhow::Error) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}

fn hook_event(event: &StoredHookEvent) -> Event {
    let data = serde_json::to_string(event).unwrap_or_else(|_| "{}".to_owned());
    Event::default()
        .event("hook_event")
        .id(event.event_id.to_string())
        .data(data)
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
