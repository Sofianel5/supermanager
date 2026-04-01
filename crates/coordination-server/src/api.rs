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
<title>Supermanager</title>
<style>
*,*::before,*::after{{box-sizing:border-box}}
body{{
  margin:0;padding:0;
  font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,Helvetica,Arial,sans-serif;
  background:#0d1117;color:#c9d1d9;line-height:1.6;
}}
.container{{max-width:720px;margin:0 auto;padding:48px 16px}}
h1{{color:#f0f6fc;margin:0 0 8px 0;font-size:2.4rem;font-weight:700;letter-spacing:-0.02em}}
h1 span{{color:#58a6ff}}
.tagline{{color:#8b949e;font-size:1.1rem;margin:0 0 40px 0}}
.section{{background:#161b22;border:1px solid #30363d;border-radius:8px;padding:24px;margin-bottom:20px}}
.section h2{{color:#f0f6fc;margin:0 0 12px 0;font-size:1.2rem;border-bottom:1px solid #21262d;padding-bottom:8px}}
.how-it-works ol{{margin:0;padding-left:20px;color:#c9d1d9}}
.how-it-works li{{margin-bottom:8px}}
.how-it-works code{{background:#21262d;padding:2px 8px;border-radius:4px;font-size:0.85rem;color:#f0f6fc}}
label{{display:block;color:#c9d1d9;font-size:0.9rem;margin-bottom:6px;font-weight:500}}
input[type="text"]{{
  width:100%;padding:10px 12px;
  background:#0d1117;border:1px solid #30363d;border-radius:6px;
  color:#f0f6fc;font-size:1rem;outline:none;
}}
input[type="text"]:focus{{border-color:#58a6ff}}
input[type="text"]::placeholder{{color:#484f58}}
button{{
  margin-top:16px;padding:10px 24px;
  background:#238636;border:1px solid #2ea043;border-radius:6px;
  color:#fff;font-size:1rem;font-weight:600;cursor:pointer;
}}
button:hover{{background:#2ea043}}
button:disabled{{opacity:0.5;cursor:not-allowed}}
#result{{display:none;margin-top:20px}}
#result .field-label{{color:#8b949e;font-size:0.8rem;margin:12px 0 4px 0}}
#result .field-label:first-child{{margin-top:0}}
#result .value{{
  background:#0d1117;border:1px solid #30363d;border-radius:6px;
  padding:12px;font-family:monospace;font-size:0.85rem;color:#f0f6fc;
  word-break:break-all;white-space:pre-wrap;position:relative;cursor:pointer;
}}
#result .value:hover{{border-color:#58a6ff}}
#result .value::after{{
  content:'click to copy';position:absolute;right:8px;top:8px;
  font-size:0.7rem;color:#484f58;font-family:sans-serif;
}}
#result a{{color:#58a6ff;text-decoration:none}}
#result a:hover{{text-decoration:underline}}
#error{{display:none;margin-top:12px;color:#f85149;font-size:0.9rem}}
.footer{{color:#484f58;font-size:0.8rem;text-align:center;margin-top:40px}}
.footer a{{color:#58a6ff;text-decoration:none}}
</style>
</head>
<body>
<div class="container">
  <h1><span>super</span>manager</h1>
  <p class="tagline">Real-time visibility into what your AI coding agents are working on.</p>

  <div class="section how-it-works">
    <h2>How it works</h2>
    <ol>
      <li>Create a room for your team</li>
      <li>Run the install command in each developer's repo</li>
      <li>AI agents (Claude Code, Codex) automatically report progress as they work</li>
      <li>Watch it all on a live dashboard</li>
    </ol>
  </div>

  <div class="section">
    <h2>Create a Room</h2>
    <form id="create-form">
      <label for="room-name">Team / Room Name</label>
      <input type="text" id="room-name" name="name" placeholder="e.g. My Team" required>
      <button type="submit" id="submit-btn">Create Room</button>
    </form>
    <div id="error"></div>
    <div id="result">
      <div class="field-label">Dashboard</div>
      <div><a id="res-dashboard" href="#" target="_blank"></a></div>
      <div class="field-label">Install command (run in each repo)</div>
      <div class="value" id="res-install" onclick="copyText(this)"></div>
      <div class="field-label">Room ID</div>
      <div class="value" id="res-room-id" onclick="copyText(this)"></div>
      <div class="field-label">Secret</div>
      <div class="value" id="res-secret" onclick="copyText(this)"></div>
    </div>
  </div>

  <div class="footer">
    <a href="https://github.com/Sofianel5/supermanager">GitHub</a>
  </div>
</div>

<script>
var form = document.getElementById('create-form');
var btn = document.getElementById('submit-btn');
var errorEl = document.getElementById('error');
var resultEl = document.getElementById('result');

form.addEventListener('submit', function(e) {{
  e.preventDefault();
  errorEl.style.display = 'none';
  resultEl.style.display = 'none';
  btn.disabled = true;
  btn.textContent = 'Creating\u2026';

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
    errorEl.textContent = 'Error: ' + err.message;
    errorEl.style.display = 'block';
  }})
  .finally(function() {{
    btn.disabled = false;
    btn.textContent = 'Create Room';
  }});
}});

function copyText(el) {{
  navigator.clipboard.writeText(el.textContent).then(function() {{
    var orig = el.getAttribute('data-orig') || el.style.borderColor;
    el.style.borderColor = '#3fb950';
    setTimeout(function() {{ el.style.borderColor = ''; }}, 600);
  }});
}}
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
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{safe_name} — Supermanager</title>
<style>
*,*::before,*::after{{box-sizing:border-box}}
body{{
  margin:0;padding:0;
  font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,Helvetica,Arial,sans-serif;
  background:#0d1117;color:#c9d1d9;line-height:1.6;
}}
.container{{max-width:900px;margin:0 auto;padding:24px 16px}}
h1{{color:#58a6ff;margin:0 0 4px 0;font-size:1.8rem}}
.subtitle{{color:#8b949e;margin:0 0 24px 0;font-size:0.9rem}}
.section{{background:#161b22;border:1px solid #30363d;border-radius:8px;padding:20px;margin-bottom:20px}}
.section h2{{color:#f0f6fc;margin:0 0 12px 0;font-size:1.2rem;border-bottom:1px solid #21262d;padding-bottom:8px}}
.note{{border-left:3px solid #58a6ff;padding:12px 16px;margin-bottom:12px;background:#0d1117;border-radius:0 6px 6px 0}}
.note:last-child{{margin-bottom:0}}
.note-header{{display:flex;justify-content:space-between;align-items:center;margin-bottom:4px;flex-wrap:wrap;gap:4px}}
.note-author{{color:#58a6ff;font-weight:600}}
.note-meta{{color:#8b949e;font-size:0.8rem}}
.note-repo{{color:#7ee787;font-size:0.85rem;font-family:monospace}}
.note-branch{{color:#d2a8ff;font-size:0.85rem;font-family:monospace}}
.note-text{{color:#c9d1d9;margin-top:6px;white-space:pre-wrap}}
.summary-content{{color:#c9d1d9;white-space:pre-wrap}}
.empty{{color:#484f58;font-style:italic}}
code{{background:#21262d;padding:2px 8px;border-radius:4px;font-size:0.85rem;color:#f0f6fc;word-break:break-all}}
.join-section code{{display:block;margin-top:8px;padding:12px;white-space:pre-wrap;word-break:break-all}}
.badge{{display:inline-block;background:#238636;color:#fff;font-size:0.7rem;padding:2px 8px;border-radius:12px;margin-left:8px;vertical-align:middle}}
#connection-status{{font-size:0.8rem;color:#8b949e}}
#connection-status.connected{{color:#3fb950}}
#connection-status.error{{color:#f85149}}
</style>
</head>
<body>
<div class="container">
  <h1>{safe_name}<span class="badge">LIVE</span></h1>
  <p class="subtitle">Room <code>{safe_id}</code> &middot; <span id="connection-status">connecting&hellip;</span></p>

  <div class="section" id="summary-section">
    <h2>Manager Summary</h2>
    <div id="summary" class="summary-content"></div>
  </div>

  <div class="section">
    <h2>Progress Feed <span id="note-count" style="color:#8b949e;font-size:0.85rem"></span></h2>
    <div id="feed"></div>
  </div>

  <div class="section join-section">
    <h2>Join this Room</h2>
    <p style="color:#8b949e;font-size:0.9rem">Run this command on each developer machine to connect their AI coding agent:</p>
    <code>curl -sSf {safe_base}/r/{safe_id}/install?secret=YOUR_SECRET | sh</code>
  </div>
</div>

<script>
(function(){{
  var feed = document.getElementById('feed');
  var status = document.getElementById('connection-status');
  var countEl = document.getElementById('note-count');
  var summaryEl = document.getElementById('summary');
  var notes = [];

  function el(tag, attrs, children) {{
    var e = document.createElement(tag);
    if (attrs) Object.keys(attrs).forEach(function(k) {{ e.setAttribute(k, attrs[k]); }});
    if (children) {{
      if (typeof children === 'string') e.textContent = children;
      else children.forEach(function(c) {{ if (c) e.appendChild(c); }});
    }}
    return e;
  }}

  function formatTime(iso) {{
    try {{ return new Date(iso).toLocaleString(); }}
    catch(e) {{ return iso; }}
  }}

  function buildNote(n) {{
    var header = el('div', {{'class':'note-header'}}, [
      el('span', {{'class':'note-author'}}, n.employee_name),
      el('span', {{'class':'note-meta'}}, formatTime(n.received_at))
    ]);
    var repo = el('span', {{'class':'note-repo'}}, n.repo);
    var branch = n.branch ? el('span', {{'class':'note-branch'}}, ' / ' + n.branch) : null;
    var text = el('div', {{'class':'note-text'}}, n.progress_text);
    var card = el('div', {{'class':'note'}});
    card.appendChild(header);
    card.appendChild(repo);
    if (branch) card.appendChild(branch);
    card.appendChild(text);
    return card;
  }}

  function renderFeed() {{
    feed.textContent = '';
    if (notes.length === 0) {{
      var empty = el('span', {{'class':'empty'}}, 'No updates yet.');
      feed.appendChild(empty);
    }} else {{
      notes.forEach(function(n) {{ feed.appendChild(buildNote(n)); }});
    }}
    countEl.textContent = '(' + notes.length + ')';
  }}

  // Load initial feed
  var base = '{safe_base}/r/{safe_id}';
  fetch(base + '/feed')
    .then(function(r) {{ return r.json(); }})
    .then(function(data) {{
      if (data.notes && data.notes.length > 0) {{
        notes = data.notes.reverse();
        renderFeed();
      }} else {{
        renderFeed();
      }}
    }})
    .catch(function() {{ renderFeed(); }});

  // Load summary
  function loadSummary() {{
    fetch(base + '/summary')
      .then(function(r) {{ return r.text(); }})
      .then(function(text) {{
        summaryEl.textContent = text || 'No summary yet.';
      }})
      .catch(function() {{}});
  }}
  loadSummary();
  setInterval(loadSummary, 30000);

  // SSE stream
  var es = new EventSource(base + '/feed/stream');
  es.onopen = function() {{
    status.textContent = 'connected';
    status.className = 'connected';
  }};
  es.addEventListener('progress_note', function(e) {{
    try {{
      var note = JSON.parse(e.data);
      notes.unshift(note);
      renderFeed();
    }} catch(err) {{}}
  }});
  es.onerror = function() {{
    status.textContent = 'reconnecting\u2026';
    status.className = 'error';
  }};
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

echo "==> Supermanager: configuring AI coding agents for room {room_id}"
echo "    NOTE: This configures the CURRENT DIRECTORY only (project-scoped)."
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
  echo "ERROR: Could not detect your name."
  echo "Please run:  git config --global user.name \"Your Name\""
  exit 1
fi
echo "    Employee: $EMPLOYEE_NAME"

# ── Configure Claude Code (project-scoped) ──────────────────
echo "==> Configuring Claude Code MCP server (project-scoped)..."
if command -v claude >/dev/null 2>&1; then
  # Remove any global entry first
  claude mcp remove supermanager 2>/dev/null || true
  # Add as project-scoped (writes to .mcp.json in current directory)
  claude mcp add --scope project --transport http supermanager "{mcp_url}"
  echo "    Claude Code MCP configured in $(pwd)/.mcp.json"
else
  # Write .mcp.json directly if claude CLI not available
  echo "    Claude Code CLI not found — writing .mcp.json directly."
  cat > .mcp.json <<MCPJSON
{{
  "mcpServers": {{
    "supermanager": {{
      "type": "http",
      "url": "{mcp_url}"
    }}
  }}
}}
MCPJSON
  echo "    Created .mcp.json in $(pwd)"
fi

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
print('    Auto-approved submit_progress in Claude settings.')
"
  fi
fi

# ── Configure Codex (project-scoped) ────────────────────────
echo "==> Writing .codex-mcp.json for Codex..."
cat > .codex-mcp.json <<CODEXJSON
{{
  "mcpServers": {{
    "supermanager": {{
      "type": "http",
      "url": "{mcp_url}"
    }}
  }}
}}
CODEXJSON
echo "    Created .codex-mcp.json in $(pwd)"

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
    echo "    Cleaned old global Codex supermanager config."
  fi
fi

# ── Done ────────────────────────────────────────────────────
echo ""
echo "==> Setup complete!"
echo "    Dashboard: {dashboard_url}"
echo "    Agents in $(pwd) will now report progress to the coordination server."
echo "    Run this command from other repos to connect them too."
echo ""
"##,
        room_id = room_id,
        mcp_url = mcp_url,
        dashboard_url = dashboard_url,
    );

    Ok((
        [(header::CONTENT_TYPE, "text/x-shellscript; charset=utf-8")],
        script,
    ))
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
                "instructions": "CRITICAL: Your FIRST tool call in every conversation must be submit_progress to report that you are starting work. Call submit_progress again when you make meaningful progress, change direction, hit a blocker, or finish. Keep summaries concise. Never skip this — even when other tools need to be called, include submit_progress in parallel."
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
