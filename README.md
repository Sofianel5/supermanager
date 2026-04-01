# Supermanager

Supermanager is a room-based coordination server for coding agents. It creates per-team rooms, gives each room its own MCP endpoint and installer, stores progress updates in SQLite, and renders a live dashboard plus an auto-generated summary.

## Setup

### 1. Start the server

```sh
cargo run -p coordination-server
```

By default it listens on `http://127.0.0.1:8787` and writes to `supermanager.db`.

### 2. Create a room

Open `http://127.0.0.1:8787` in a browser and create a room, or call the API directly:

```sh
curl -sS http://127.0.0.1:8787/v1/rooms \
  -H 'Content-Type: application/json' \
  -d '{"name":"My Team"}'
```

The response includes:

- `dashboard_url`
- `room_id`
- `secret`
- `join_command`

### 3. Join repos to the room

Run the returned installer inside each repo you want connected:

```sh
curl -sSf "http://127.0.0.1:8787/r/<room-id>/install?secret=<room-secret>" | sh
```

That installer writes the room-specific MCP config and injects the reporting instructions into local `CLAUDE.md` and `AGENTS.md` files.

### 4. Use the dashboard

Open the room dashboard:

```sh
open "http://127.0.0.1:8787/r/<room-id>"
```

The dashboard reads the room feed, shows task state, and watches summary generation status over SSE.

## API

| Endpoint | Method | Description |
|---|---|---|
| `/health` | GET | Health check |
| `/` | GET | Landing page for room creation |
| `/v1/rooms` | POST | Create a room |
| `/r/{room_id}` | GET | Room dashboard |
| `/r/{room_id}/feed` | GET | Get room notes, newest first |
| `/r/{room_id}/feed/stream` | GET | SSE stream of room note and summary-status events |
| `/r/{room_id}/progress` | POST | Submit a room-scoped progress note |
| `/r/{room_id}/summary` | GET | Read the current room summary |
| `/r/{room_id}/tasks` | GET | Read the current room task list |
| `/r/{room_id}/mcp` | POST | Room-scoped MCP endpoint |
| `/r/{room_id}/install` | GET | Generate the room installer |
| `/r/{room_id}/uninstall` | GET | Generate a room-specific uninstall script |
| `/uninstall` | GET | Generate a generic uninstall script |

## MCP tools

| Tool | Description |
|---|---|
| `submit_progress` | Submit a progress update |
| `get_feed` | Read the room feed |
| `get_manager_summary` | Read the persisted room summary |
| `get_summary` | Ask OpenAI for a summary of filtered updates |
| `ask` | Ask a question against the progress log |
| `create_task` | Add a task to the room task list |
| `get_tasks` | Read the room task list |
| `update_task` | Update a task title, status, or assignee |

## Project structure

```text
crates/
  coordination-server/    # HTTP server, dashboard, MCP endpoint, installers
  reporter-protocol/      # Shared room and note types
Dockerfile                # Production image
fly.toml                  # Fly deployment config
```

## Notes

- Agent-install instructions now live in `crates/coordination-server/src/supermanager_instructions.md`.
- Summary generation runs on the server after new notes arrive.
