use std::{convert::Infallible, sync::Arc, time::Duration};

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
    CreateRoomRequest, CreateRoomResponse, FeedResponse, IngestResponse, ProgressNote,
    StoredProgressNote,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::broadcast;

use crate::store::{Db, LOCAL_ROOM_ID};

// ── Shared state ────────────────────────────────────────────

#[derive(Clone)]
pub struct NoteEvent {
    pub room_id: String,
    pub note: StoredProgressNote,
}

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Db>,
    pub note_events: broadcast::Sender<NoteEvent>,
    pub base_url: String,
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
        <li>Run the install command in each developer's repo</li>
        <li>AI agents (<code>Claude Code</code>, <code>Codex</code>) automatically report progress as they work</li>
        <li>Watch it all on a live dashboard</li>
      </ol>
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
        <div class="field-label">Install command</div>
        <div class="field-value" id="res-install"></div>
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
      document.getElementById('res-install').textContent = data.join_command;
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
        dashboard_url: format!("{}/r/{}", state.base_url, room.room_id),
        join_command: format!(
            "curl -sSf \"{}/r/{}/install?secret={}\" | sh",
            state.base_url, room.room_id, room.secret
        ),
        room_id: room.room_id,
        secret: room.secret,
    };
    Ok((StatusCode::CREATED, Json(resp)))
}

// ── Room-scoped routes ──────────────────────────────────────

pub async fn ingest_progress(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<SecretQuery>,
    Json(note): Json<ProgressNote>,
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
        .insert_note(&room_id, &note)
        .map_err(internal_error)?;
    let note_id = stored.note_id;
    let received_at = stored.received_at.clone();

    let _ = state.note_events.send(NoteEvent {
        room_id,
        note: stored,
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(IngestResponse {
            note_id,
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
    let notes = state
        .db
        .get_notes(&room_id)
        .map_err(internal_error)?;
    Ok(Json(FeedResponse { notes }))
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
        .map(|note_id| state.db.get_notes_after(&room_id, note_id))
        .transpose()
        .map_err(internal_error)?
        .unwrap_or_default();

    let mut receiver = state.note_events.subscribe();
    let target_room = room_id.clone();

    let event_stream = stream! {
        for note in replay {
            yield Ok(progress_event(&note));
        }

        loop {
            match receiver.recv().await {
                Ok(evt) => {
                    if evt.room_id == target_room {
                        yield Ok(progress_event(&evt.note));
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
    let summary = state
        .db
        .get_summary(&room_id)
        .map_err(internal_error)?;
    Ok((
        [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
        summary,
    ))
}

pub async fn update_manager_summary(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<SecretQuery>,
    body: String,
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

    state
        .db
        .set_summary(&room_id, &body)
        .map_err(internal_error)?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Dashboard ───────────────────────────────────────────────

pub async fn dashboard(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let room = state
        .db
        .get_room(&room_id)
        .map_err(internal_error)?;
    match room {
        Some(r) => {
            let base = &state.base_url;
            let html = build_dashboard_html(&r.name, &r.room_id, base);
            Ok((
                [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
                html,
            ))
        }
        None => Err((StatusCode::NOT_FOUND, format!("room not found: {room_id}"))),
    }
}

fn build_dashboard_html(name: &str, room_id: &str, base_url: &str) -> String {
    let safe_name = html_escape(name);
    let safe_id = html_escape(room_id);
    let safe_base = html_escape(base_url);
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
.note-text{{color:var(--text-secondary);font-family:var(--sans);font-size:0.88rem;white-space:pre-wrap;line-height:1.6;}}
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

  <div class="panel">
    <div class="panel-head">
      <span class="panel-title">Connect Agents</span>
    </div>
    <div class="panel-body">
      <p class="join-label">Run this in each repo to connect AI coding agents to this room. The room creator has the full command with the secret &mdash; ask them for it.</p>
      <code id="join-cmd" class="join-cmd">curl -sSf {safe_base}/r/{safe_id}/install?secret=YOUR_SECRET | sh</code>
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
  var summaryEl = document.getElementById('summary');
  var notes = [];
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

  function buildNote(n, i) {{
    var card = el('div', 'note');
    card.style.animationDelay = (i * 0.04) + 's';

    var top = el('div', 'note-row-top');
    top.appendChild(el('span', 'note-author', n.employee_name));
    var time = el('span', 'note-time', timeAgo(n.received_at));
    time.title = fullTime(n.received_at);
    top.appendChild(time);
    card.appendChild(top);

    var repoLine = el('div', 'note-repo-line');
    repoLine.appendChild(el('span', 'note-repo', n.repo));
    if (n.branch) {{
      repoLine.appendChild(el('span', 'note-sep', '/'));
      repoLine.appendChild(el('span', 'note-branch', n.branch));
    }}
    card.appendChild(repoLine);
    card.appendChild(el('div', 'note-text', n.progress_text));
    return card;
  }}

  function renderFeed() {{
    feed.textContent = '';
    if (notes.length === 0) {{
      feed.appendChild(el('span', 'empty', 'No updates yet.'));
      toggleBtn.style.display = 'none';
    }} else {{
      var visible = expanded ? notes : notes.slice(0, FEED_LIMIT);
      visible.forEach(function(n, i) {{ feed.appendChild(buildNote(n, i)); }});
      if (notes.length > FEED_LIMIT) {{
        toggleBtn.style.display = 'block';
        var hidden = notes.length - FEED_LIMIT;
        toggleBtn.textContent = expanded ? 'Show less' : 'Show ' + hidden + ' more';
      }} else {{
        toggleBtn.style.display = 'none';
      }}
    }}
    var label = notes.length === 1 ? '1 update' : notes.length + ' updates';
    countEl.textContent = label;
  }}

  toggleBtn.addEventListener('click', function() {{
    expanded = !expanded;
    renderFeed();
  }});

  setInterval(function() {{
    var times = feed.querySelectorAll('.note-time');
    var all = notes;
    times.forEach(function(t, i) {{
      if (all[i]) t.textContent = timeAgo(all[i].received_at);
    }});
  }}, 30000);

  var base = '{safe_base}/r/{safe_id}';
  fetch(base + '/feed')
    .then(function(r) {{ return r.json(); }})
    .then(function(data) {{
      if (data.notes && data.notes.length > 0) {{ notes = data.notes; }}
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
  setInterval(loadSummary, 30000);

  var es = new EventSource(base + '/feed/stream');
  es.onopen = function() {{
    liveText.textContent = 'live';
    statusEl.className = 'live-dot connected';
  }};
  es.addEventListener('progress_note', function(e) {{
    try {{
      var note = JSON.parse(e.data);
      notes.unshift(note);
      renderFeed();
    }} catch(err) {{}}
  }});
  es.onerror = function() {{
    liveText.textContent = 'reconnecting';
    statusEl.className = 'live-dot error';
  }};

  var joinCmd = document.getElementById('join-cmd');
  joinCmd.addEventListener('click', function() {{
    navigator.clipboard.writeText(joinCmd.textContent).then(function() {{
      joinCmd.classList.add('copied');
      setTimeout(function() {{ joinCmd.classList.remove('copied'); }}, 2000);
    }});
  }});
}})();
</script>
</body>
</html>"##,
    )
}

// ── Install script ──────────────────────────────────────────

pub async fn install_script(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Query(query): Query<SecretQuery>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let secret = query
        .secret
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or((StatusCode::FORBIDDEN, "missing secret query param".to_owned()))?;

    let valid = state
        .db
        .verify_room_secret(&room_id, secret)
        .map_err(internal_error)?;
    if !valid {
        return Err((StatusCode::FORBIDDEN, "invalid secret".to_owned()));
    }

    let base = &state.base_url;
    let mcp_url = format!("{base}/r/{room_id}/mcp?secret={secret}");
    let dashboard_url = format!("{base}/r/{room_id}");

    let script = format!(
        r##"#!/bin/sh
set -e

# ── Supermanager agent installer ────────────────────────────
# Room:      {room_id}
# Dashboard: {dashboard_url}

echo ""
echo "  ┌─────────────────────────────────────────────────┐"
echo "  │  supermanager installer                         │"
echo "  │  Room: {room_id}"
echo "  └─────────────────────────────────────────────────┘"
echo ""
echo "  NOTE: This configures the CURRENT DIRECTORY only (project-scoped)."
echo ""

# ── Detect employee name ────────────────────────────────────
EMPLOYEE_NAME=""
if command -v git >/dev/null 2>&1; then
  EMPLOYEE_NAME="$(git config user.name 2>/dev/null || true)"
fi
if [ -z "$EMPLOYEE_NAME" ]; then
  EMPLOYEE_NAME="$(whoami 2>/dev/null || true)"
fi
if [ -z "$EMPLOYEE_NAME" ]; then
  echo "  ERROR: Could not detect your name."
  echo "  Please run:  git config --global user.name \"Your Name\""
  exit 1
fi
echo "  Employee: $EMPLOYEE_NAME"
echo ""

# ── Configure Claude Code (project-scoped) ──────────────────
echo "  [1/4] Configuring Claude Code..."
if command -v claude >/dev/null 2>&1; then
  # Remove any global entry first
  claude mcp remove supermanager 2>/dev/null || true
  # Add as project-scoped (writes to .mcp.json in current directory)
  claude mcp add --scope project --transport http supermanager "{mcp_url}"
  echo "        MCP configured in $(pwd)/.mcp.json"
else
  # Merge into .mcp.json without clobbering other servers
  echo "        Claude CLI not found — updating .mcp.json directly."
  if command -v python3 >/dev/null 2>&1; then
    python3 -c "
import json, os
path = '.mcp.json'
cfg = {{}}
if os.path.exists(path):
    with open(path) as f:
        cfg = json.load(f)
cfg.setdefault('mcpServers', {{}})['supermanager'] = {{
    'type': 'http',
    'url': '{mcp_url}'
}}
with open(path, 'w') as f:
    json.dump(cfg, f, indent=2)
"
    echo "        Updated .mcp.json in $(pwd)"
  else
    echo "        ERROR: Neither claude CLI nor python3 found. Cannot configure MCP."
    echo "        Please install Claude Code or python3 and re-run."
    exit 1
  fi
fi
echo ""

# ── Auto-approve submit_progress in Claude settings ─────────
CLAUDE_SETTINGS="$HOME/.claude/settings.json"
if [ -f "$CLAUDE_SETTINGS" ]; then
  if command -v python3 >/dev/null 2>&1; then
    python3 -c "
import json, sys
try:
    with open('$CLAUDE_SETTINGS') as f:
        cfg = json.load(f)
except:
    cfg = {{}}
perms = cfg.setdefault('permissions', {{}})
allow = perms.setdefault('allow', [])
tool_entry = 'mcp__supermanager__submit_progress'
if tool_entry not in allow:
    allow.append(tool_entry)
with open('$CLAUDE_SETTINGS', 'w') as f:
    json.dump(cfg, f, indent=2)
print('        Auto-approved submit_progress in Claude settings.')
"
  fi
fi

# ── Configure Codex (project-scoped) ────────────────────────
echo "  [2/4] Configuring Codex..."
if command -v python3 >/dev/null 2>&1; then
  python3 -c "
import json, os
path = '.codex-mcp.json'
cfg = {{}}
if os.path.exists(path):
    with open(path) as f:
        cfg = json.load(f)
cfg.setdefault('mcpServers', {{}})['supermanager'] = {{
    'type': 'http',
    'url': '{mcp_url}'
}}
with open(path, 'w') as f:
    json.dump(cfg, f, indent=2)
"
  echo "        Updated .codex-mcp.json in $(pwd)"
else
  echo "        WARNING: python3 not found — skipping Codex config."
fi
echo ""

# ── Remove old global Codex config if present ────────────────
CODEX_CFG="$HOME/.codex/config.toml"
if [ -f "$CODEX_CFG" ] && grep -q "mcp_servers.supermanager" "$CODEX_CFG" 2>/dev/null; then
  if command -v python3 >/dev/null 2>&1; then
    python3 -c "
import re
with open('$CODEX_CFG') as f:
    text = f.read()
text = re.sub(r'\[mcp_servers\.supermanager\][^\[]*', '', text)
with open('$CODEX_CFG', 'w') as f:
    f.write(text)
"
    echo "        Cleaned old global Codex config."
  fi
fi

# ── Inject instructions into CLAUDE.md and AGENTS.md ────────
echo "  [3/4] Injecting agent instructions..."
SUPERMANAGER_INSTRUCTIONS=$(echo '{agent_instructions}' | sed "s/SUPERMANAGER_EMPLOYEE_NAME/$EMPLOYEE_NAME/g")

for INSTRUCTIONS_FILE in CLAUDE.md AGENTS.md; do
  if [ -f "$INSTRUCTIONS_FILE" ] && grep -q '<!-- supermanager:start -->' "$INSTRUCTIONS_FILE"; then
    # Replace existing block
    if command -v python3 >/dev/null 2>&1; then
      python3 -c "
import re
with open('$INSTRUCTIONS_FILE') as f:
    text = f.read()
text = re.sub(
    r'<!-- supermanager:start -->.*?<!-- supermanager:end -->',
    '''$SUPERMANAGER_INSTRUCTIONS''',
    text,
    flags=re.DOTALL,
)
with open('$INSTRUCTIONS_FILE', 'w') as f:
    f.write(text)
"
      echo "        Updated supermanager block in $INSTRUCTIONS_FILE"
    fi
  else
    # Append
    printf '\n%s\n' "$SUPERMANAGER_INSTRUCTIONS" >> "$INSTRUCTIONS_FILE"
    echo "        Added supermanager block to $INSTRUCTIONS_FILE"
  fi
done

# ── Done ────────────────────────────────────────────────────
echo ""
echo "  [4/4] Done!"
echo ""
echo "  ┌─────────────────────────────────────────────────┐"
echo "  │  Setup complete!                                │"
echo "  │                                                 │"
echo "  │  Dashboard: {dashboard_url}"
echo "  │  Directory: $(pwd)"
echo "  │                                                 │"
echo "  │  Agents here will now report progress.          │"
echo "  │  Run this command in other repos to connect     │"
echo "  │  them too.                                      │"
echo "  └─────────────────────────────────────────────────┘"
echo ""
"##,
        room_id = room_id,
        mcp_url = mcp_url,
        dashboard_url = dashboard_url,
        agent_instructions = include_str!("supermanager_instructions.md"),
    );

    Ok((
        [(header::CONTENT_TYPE, "text/x-shellscript; charset=utf-8")],
        script,
    ))
}

pub async fn uninstall_script_global() -> impl IntoResponse {
    uninstall_response(None)
}

pub async fn uninstall_script(
    Path(room_id): Path<String>,
) -> impl IntoResponse {
    uninstall_response(Some(&room_id))
}

fn uninstall_response(room_id: Option<&str>) -> ([(header::HeaderName, &'static str); 1], String) {
    let room_line = match room_id {
        Some(id) => format!("echo \"  Room: {id}\""),
        None => String::new(),
    };

    let script = format!(
        r##"#!/bin/sh
set -e

# ── Supermanager agent uninstaller ─────────────────────────

echo ""
echo "  supermanager uninstaller"
{room_line}
echo ""

# ── Remove Claude Code MCP ─────────────────────────────────
echo "  [1/4] Removing Claude Code MCP..."
if command -v claude >/dev/null 2>&1; then
  claude mcp remove supermanager 2>/dev/null || true
  echo "        Removed supermanager MCP from Claude."
fi
if [ -f .mcp.json ]; then
  if command -v python3 >/dev/null 2>&1; then
    python3 -c "
import json
with open('.mcp.json') as f:
    cfg = json.load(f)
servers = cfg.get('mcpServers', {{}})
if 'supermanager' in servers:
    del servers['supermanager']
if servers:
    with open('.mcp.json', 'w') as f:
        json.dump(cfg, f, indent=2)
    print('        Removed supermanager from .mcp.json')
else:
    import os
    os.remove('.mcp.json')
    print('        Deleted .mcp.json (was only supermanager)')
"
  fi
fi
echo ""

# ── Remove auto-approve from Claude settings ───────────────
echo "  [2/4] Removing auto-approve..."
CLAUDE_SETTINGS="\$HOME/.claude/settings.json"
if [ -f "\$CLAUDE_SETTINGS" ]; then
  if command -v python3 >/dev/null 2>&1; then
    python3 -c "
import json
with open('\$CLAUDE_SETTINGS') as f:
    cfg = json.load(f)
perms = cfg.get('permissions', {{}})
allow = perms.get('allow', [])
entries = [e for e in allow if 'supermanager' in str(e)]
for e in entries:
    allow.remove(e)
with open('\$CLAUDE_SETTINGS', 'w') as f:
    json.dump(cfg, f, indent=2)
if entries:
    print('        Removed ' + str(len(entries)) + ' auto-approve entries.')
else:
    print('        No auto-approve entries found.')
"
  fi
fi
echo ""

# ── Remove Codex config ────────────────────────────────────
echo "  [3/4] Removing Codex MCP..."
if [ -f .codex-mcp.json ]; then
  rm -f .codex-mcp.json
  echo "        Deleted .codex-mcp.json"
else
  echo "        No .codex-mcp.json found."
fi
echo ""

# ── Remove instructions from CLAUDE.md and AGENTS.md ──────
echo "  [4/4] Removing agent instructions..."
for INSTRUCTIONS_FILE in CLAUDE.md AGENTS.md; do
  if [ -f "$INSTRUCTIONS_FILE" ] && grep -q '<!-- supermanager:start -->' "$INSTRUCTIONS_FILE"; then
    if command -v python3 >/dev/null 2>&1; then
      python3 -c "
import re
with open('$INSTRUCTIONS_FILE') as f:
    text = f.read()
text = re.sub(
    r'\n?<!-- supermanager:start -->.*?<!-- supermanager:end -->\n?',
    '',
    text,
    flags=re.DOTALL,
)
with open('$INSTRUCTIONS_FILE', 'w') as f:
    f.write(text.strip() + '\n')
print('        Removed supermanager block from $INSTRUCTIONS_FILE')
"
    fi
  else
    echo "        No supermanager block in $INSTRUCTIONS_FILE"
  fi
done

echo ""
echo "  Uninstall complete! Agents here will no longer report progress."
echo ""
"##,
        room_line = room_line,
    );

    (
        [(header::CONTENT_TYPE, "text/x-shellscript; charset=utf-8")],
        script,
    )
}

// ── Room-scoped MCP ─────────────────────────────────────────

pub async fn handle_mcp(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Query(query): Query<SecretQuery>,
    headers: HeaderMap,
    Json(req): Json<Value>,
) -> impl IntoResponse {
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");

    let secret = extract_secret(&headers, &query);

    let result = match method {
        "initialize" => {
            let client_version = req
                .pointer("/params/protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or("2025-03-26");
            json!({
                "protocolVersion": client_version,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "supermanager", "version": "0.1.0" },
                "instructions": "CRITICAL: Your FIRST tool call in every conversation must be submit_progress to report that you are starting work. Call submit_progress again when you make meaningful progress, change direction, hit a blocker, or finish. Keep summaries concise. Never skip this — even when other tools need to be called, include submit_progress in parallel.\n\nFor employee_name: use `git config user.name` to resolve the human's name. If you are a subagent (spawned by another agent, not directly by the user), append ' (subagent)' — e.g. 'Bryan Chiang (subagent)'. Never use 'Claude', 'user', 'assistant', or your own name.\n\nWhen calling get_summary or ask: always pass your current git branch in the `branch` parameter so results are scoped to the relevant work. Omit `branch` only if the user explicitly asks about all branches."
            })
        }
        _ if method.starts_with("notifications/") => {
            return StatusCode::ACCEPTED.into_response();
        }
        "tools/list" => json!({
            "tools": [
                {
                    "name": "submit_progress",
                    "description": "Submit a progress update to the coordination server. Use this to report what you accomplished.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "employee_name": {
                                "type": "string",
                                "description": "Name of the human user you are working for — never use 'Claude' or your own name"
                            },
                            "repo": {
                                "type": "string",
                                "description": "Repository URL or identifier"
                            },
                            "branch": {
                                "type": "string",
                                "description": "Git branch name"
                            },
                            "progress_text": {
                                "type": "string",
                                "description": "A concise summary of what was accomplished"
                            }
                        },
                        "required": ["employee_name", "repo", "branch", "progress_text"]
                    }
                },
                {
                    "name": "get_feed",
                    "description": "Get the feed of all progress updates from all employees.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {}
                    }
                },
                {
                    "name": "get_manager_summary",
                    "description": "Read the manager-facing Markdown summary document that lives on the coordination server.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {}
                    }
                },
                {
                    "name": "update_manager_summary",
                    "description": "Replace the manager-facing Markdown summary document on the coordination server.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "content_markdown": {
                                "type": "string",
                                "description": "Full Markdown contents for the manager summary document."
                            }
                        },
                        "required": ["content_markdown"]
                    }
                },
                {
                    "name": "get_summary",
                    "description": "Get an AI-generated summary of recent progress updates. Always pass your current git branch to scope results. Supports filtering by time window, message count, employee name, branch, or since a specific person's last update.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "limit": {
                                "type": "integer",
                                "description": "Max number of messages to summarize (default: 20)"
                            },
                            "minutes": {
                                "type": "integer",
                                "description": "Only include messages from the last N minutes"
                            },
                            "employee_name": {
                                "type": "string",
                                "description": "Filter to only this person's updates"
                            },
                            "branch": {
                                "type": "string",
                                "description": "Filter to only updates from this git branch"
                            },
                            "since_last_update_by": {
                                "type": "string",
                                "description": "Include all updates since this person's most recent update"
                            }
                        }
                    }
                },
                {
                    "name": "ask",
                    "description": "Ask a question about progress updates and get a focused, cited answer. Always pass your current git branch to scope results. Searches the raw log so you don't need it in context.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "question": {
                                "type": "string",
                                "description": "The question to answer from the progress log"
                            },
                            "limit": {
                                "type": "integer",
                                "description": "How far back to search in messages (default: 50)"
                            },
                            "minutes": {
                                "type": "integer",
                                "description": "Only search messages from the last N minutes"
                            },
                            "employee_name": {
                                "type": "string",
                                "description": "Only search this person's updates"
                            },
                            "branch": {
                                "type": "string",
                                "description": "Only search updates from this git branch"
                            },
                            "since_last_update_by": {
                                "type": "string",
                                "description": "Only search updates since this person's most recent update"
                            }
                        },
                        "required": ["question"]
                    }
                }
            ]
        }),
        "tools/call" => {
            let tool_name = req
                .pointer("/params/name")
                .and_then(Value::as_str)
                .unwrap_or("");
            match tool_name {
                "submit_progress" => {
                    match verify_mcp_secret(&state, &room_id, &secret) {
                        Ok(()) => mcp_submit_progress(&state, &room_id, &req),
                        Err(msg) => mcp_error(msg),
                    }
                }
                "update_manager_summary" => {
                    match verify_mcp_secret(&state, &room_id, &secret) {
                        Ok(()) => mcp_update_manager_summary(&state, &room_id, &req),
                        Err(msg) => mcp_error(msg),
                    }
                }
                // Public tools — no secret required
                "get_feed" => mcp_get_feed(&state, &room_id),
                "get_manager_summary" => mcp_get_manager_summary(&state, &room_id),
                "get_summary" => mcp_get_summary(&state, &room_id, &req).await,
                "ask" => mcp_ask(&state, &room_id, &req).await,
                _ => json!({
                    "isError": true,
                    "content": [{ "type": "text", "text": format!("Unknown tool: {tool_name}") }]
                }),
            }
        }
        _ => {
            return Json(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("Unknown method: {method}") }
            }))
            .into_response();
        }
    };

    Json(json!({ "jsonrpc": "2.0", "id": id, "result": result })).into_response()
}

/// Verify that a secret is present and valid for the given room.
/// Returns Ok(()) on success or Err(message) on failure.
fn verify_mcp_secret(state: &AppState, room_id: &str, secret: &Option<String>) -> Result<(), &'static str> {
    match secret {
        Some(s) => {
            let valid = state
                .db
                .verify_room_secret(room_id, s)
                .unwrap_or(false);
            if valid { Ok(()) } else { Err("Unauthorized: invalid secret") }
        }
        None => Err("Unauthorized: secret required"),
    }
}

fn mcp_error(msg: &str) -> Value {
    json!({
        "isError": true,
        "content": [{ "type": "text", "text": msg }]
    })
}

fn mcp_submit_progress(state: &AppState, room_id: &str, req: &Value) -> Value {
    let args = req.pointer("/params/arguments");
    let str_arg = |field| {
        args.and_then(|a| a.get(field))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned()
    };

    let note = ProgressNote {
        employee_name: str_arg("employee_name"),
        repo: str_arg("repo"),
        branch: Some(str_arg("branch")),
        progress_text: str_arg("progress_text"),
    };

    match state.db.insert_note(room_id, &note) {
        Ok(stored) => {
            let note_id = stored.note_id;
            let _ = state.note_events.send(NoteEvent {
                room_id: room_id.to_owned(),
                note: stored,
            });
            json!({
                "content": [{ "type": "text", "text": format!("Progress submitted (note_id: {note_id})") }]
            })
        }
        Err(e) => json!({
            "isError": true,
            "content": [{ "type": "text", "text": format!("Failed to submit: {e}") }]
        }),
    }
}

fn mcp_get_feed(state: &AppState, room_id: &str) -> Value {
    match state.db.get_notes(room_id) {
        Ok(notes) => json!({
            "content": [{ "type": "text", "text": serde_json::to_string_pretty(&notes).unwrap_or_default() }]
        }),
        Err(e) => json!({
            "isError": true,
            "content": [{ "type": "text", "text": format!("Failed to read feed: {e}") }]
        }),
    }
}

fn mcp_get_manager_summary(state: &AppState, room_id: &str) -> Value {
    match state.db.get_summary(room_id) {
        Ok(summary) => json!({
            "content": [{ "type": "text", "text": summary }]
        }),
        Err(e) => json!({
            "isError": true,
            "content": [{ "type": "text", "text": format!("Failed to read manager summary: {e}") }]
        }),
    }
}

fn mcp_update_manager_summary(state: &AppState, room_id: &str, req: &Value) -> Value {
    let content_markdown = req
        .pointer("/params/arguments/content_markdown")
        .and_then(Value::as_str)
        .unwrap_or("");

    if content_markdown.is_empty() {
        return json!({
            "isError": true,
            "content": [{ "type": "text", "text": "Missing required field: content_markdown" }]
        });
    }

    match state.db.set_summary(room_id, content_markdown) {
        Ok(_) => json!({
            "content": [{
                "type": "text",
                "text": format!("Manager summary updated for room {room_id}")
            }]
        }),
        Err(e) => json!({
            "isError": true,
            "content": [{ "type": "text", "text": format!("Failed to update manager summary: {e}") }]
        }),
    }
}

/// Shared: resolve filter args → fetch notes → format context string.
/// Returns `Ok((context, filter_desc))` or `Err(mcp error Value)`.
fn resolve_notes_context(
    state: &AppState,
    room_id: &str,
    args: &Value,
    default_limit: u32,
) -> Result<(String, String), Value> {
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(default_limit as u64) as u32;
    let minutes = args.get("minutes").and_then(Value::as_u64);
    let employee_name = args.get("employee_name").and_then(Value::as_str);
    let branch = args.get("branch").and_then(Value::as_str);
    let since_last_update_by = args.get("since_last_update_by").and_then(Value::as_str);

    // Resolve time cutoff
    let after_time = if let Some(person) = since_last_update_by {
        state.db.get_last_update_time(room_id, person)
            .map_err(|e| mcp_error(&format!("Failed to look up last update by {person}: {e}")))?
    } else if let Some(mins) = minutes {
        let cutoff = time::OffsetDateTime::now_utc()
            - time::Duration::minutes(mins as i64);
        Some(
            cutoff
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
        )
    } else {
        None
    };

    let notes = state.db.get_notes_filtered(
        room_id,
        after_time.as_deref(),
        employee_name,
        branch,
        limit,
    ).map_err(|e| mcp_error(&format!("Failed to fetch notes: {e}")))?;

    if notes.is_empty() {
        return Err(json!({
            "content": [{ "type": "text", "text": "No progress updates found matching the filter." }]
        }));
    }

    let mut context = String::new();
    for n in &notes {
        context.push_str(&format!(
            "[{}] {} ({}, {}): {}\n",
            n.received_at,
            n.note.employee_name,
            n.note.repo,
            n.note.branch.as_deref().unwrap_or("—"),
            n.note.progress_text,
        ));
    }

    let mut filter_desc = format!("{} most recent updates", notes.len());
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

/// Shared: call OpenAI Responses API and return the text.
async fn call_openai(state: &AppState, instructions: &str, input: &str) -> Value {
    let api_key = match &state.openai_api_key {
        Some(k) => k,
        None => return mcp_error("OPENAI_API_KEY not configured on the server"),
    };

    let body = json!({
        "model": "gpt-5.4-mini",
        "instructions": instructions,
        "input": input,
    });

    let resp = state
        .http
        .post("https://api.openai.com/v1/responses")
        .header("Authorization", format!("Bearer {api_key}"))
        .json(&body)
        .send()
        .await;

    let resp = match resp {
        Ok(r) => r,
        Err(e) => return mcp_error(&format!("OpenAI request failed: {e}")),
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        return mcp_error(&format!("OpenAI returned {status}: {body_text}"));
    }

    let resp_json: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => return mcp_error(&format!("Failed to parse OpenAI response: {e}")),
    };

    let text = resp_json
        .pointer("/output/0/content/0/text")
        .and_then(Value::as_str)
        .unwrap_or("(empty response from OpenAI)");

    json!({
        "content": [{ "type": "text", "text": text }]
    })
}

async fn mcp_get_summary(state: &AppState, room_id: &str, req: &Value) -> Value {
    let args = req.pointer("/params/arguments").cloned().unwrap_or(json!({}));

    let (context, filter_desc) = match resolve_notes_context(state, room_id, &args, 20) {
        Ok(v) => v,
        Err(e) => return e,
    };

    call_openai(
        state,
        "You are a concise project manager assistant. Summarize progress updates into a clear, actionable briefing. Group by person or theme. Highlight blockers, completions, and key decisions. Be brief.",
        &format!("Summarize these {filter_desc}:\n\n{context}"),
    ).await
}

async fn mcp_ask(state: &AppState, room_id: &str, req: &Value) -> Value {
    let args = req.pointer("/params/arguments").cloned().unwrap_or(json!({}));

    let question = match args.get("question").and_then(Value::as_str) {
        Some(q) => q.to_owned(),
        None => return mcp_error("Missing required field: question"),
    };

    let (context, _filter_desc) = match resolve_notes_context(state, room_id, &args, 50) {
        Ok(v) => v,
        Err(e) => return e,
    };

    call_openai(
        state,
        "You are a project assistant. Answer the question based ONLY on the progress log below. Be specific — cite timestamps and names. If the answer isn't in the log, say so clearly.",
        &format!("Question: {question}\n\nProgress log:\n{context}"),
    ).await
}

// ── Legacy (non-room) routes ────────────────────────────────
// These delegate to the "__local" default room for backwards compat.

pub async fn legacy_ingest_progress(
    State(state): State<AppState>,
    Json(note): Json<ProgressNote>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let stored = state
        .db
        .insert_note(LOCAL_ROOM_ID, &note)
        .map_err(internal_error)?;
    let note_id = stored.note_id;
    let received_at = stored.received_at.clone();

    let _ = state.note_events.send(NoteEvent {
        room_id: LOCAL_ROOM_ID.to_owned(),
        note: stored,
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(IngestResponse {
            note_id,
            received_at,
        }),
    ))
}

pub async fn legacy_get_feed(
    State(state): State<AppState>,
) -> Result<Json<FeedResponse>, (StatusCode, String)> {
    let notes = state
        .db
        .get_notes(LOCAL_ROOM_ID)
        .map_err(internal_error)?;
    Ok(Json(FeedResponse { notes }))
}

pub async fn legacy_stream_feed(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)>
{
    let replay = headers
        .get("last-event-id")
        .and_then(|value| value.to_str().ok())
        .map(|note_id| state.db.get_notes_after(LOCAL_ROOM_ID, note_id))
        .transpose()
        .map_err(internal_error)?
        .unwrap_or_default();

    let mut receiver = state.note_events.subscribe();

    let event_stream = stream! {
        for note in replay {
            yield Ok(progress_event(&note));
        }

        loop {
            match receiver.recv().await {
                Ok(evt) => {
                    if evt.room_id == LOCAL_ROOM_ID {
                        yield Ok(progress_event(&evt.note));
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
    };

    Ok(Sse::new(event_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

pub async fn legacy_get_manager_summary(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let summary = state
        .db
        .get_summary(LOCAL_ROOM_ID)
        .map_err(internal_error)?;
    Ok((
        [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
        summary,
    ))
}

pub async fn legacy_update_manager_summary(
    State(state): State<AppState>,
    body: String,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    state
        .db
        .set_summary(LOCAL_ROOM_ID, &body)
        .map_err(internal_error)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn legacy_handle_mcp(
    state: State<AppState>,
    headers: HeaderMap,
    json: Json<Value>,
) -> impl IntoResponse {
    handle_mcp(
        state,
        Path(LOCAL_ROOM_ID.to_owned()),
        Query(SecretQuery { secret: None }),
        headers,
        json,
    )
    .await
}

// ── Helpers ─────────────────────────────────────────────────

fn internal_error(error: anyhow::Error) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}

fn progress_event(note: &StoredProgressNote) -> Event {
    let data = serde_json::to_string(note).unwrap_or_else(|_| "{}".to_owned());
    Event::default()
        .event("progress_note")
        .id(note.note_id.to_string())
        .data(data)
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
