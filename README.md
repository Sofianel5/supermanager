# Supermanager

Supermanager is a room-based coordination server for coding agents. It creates per-team rooms, installs repo-local Claude Code and Codex hooks through a globally installed `supermanager` CLI, stores turn updates in SQLite, and renders a live dashboard plus an auto-generated summary.

## Setup

### 1. Start the server

```sh
cargo run -p coordination-server
```

By default it listens on `http://127.0.0.1:8787` and writes to `supermanager.db`.

To customize the one-time CLI install command shown in the UI and API responses:

```sh
cargo run -p coordination-server -- \
  --cli-install-command 'cargo install --git https://github.com/your-org/supermanager.git supermanager'
```

### 2. Install the CLI once per machine

From this repo:

```sh
cargo install --path crates/supermanager-cli
```

Or directly from Git:

```sh
cargo install --git https://github.com/Sofianel5/supermanager.git supermanager
```

### 3. Create a room

Open `http://127.0.0.1:8787` in a browser and create a room, or call the API directly:

```sh
curl -sS http://127.0.0.1:8787/v1/rooms \
  -H 'Content-Type: application/json' \
  -d '{"name":"My Team"}'
```

The response includes:

- `install_command`
- `dashboard_url`
- `room_id`
- `secret`
- `join_command`

### 4. Join repos to the room

Run the returned join command inside each repo you want connected:

```sh
supermanager join --server "http://127.0.0.1:8787" --room "<room-id>" --secret "<room-secret>"
```

That command installs repo-local Claude Code and Codex hooks for the current repo only. Claude uses `.claude/settings.local.json`; Codex uses `.codex/hooks.json` and ensures `[features]` contains `codex_hooks = true` in `.codex/config.toml`. Both hooks call the native `supermanager hook-report` subcommand, and room credentials are stored machine-locally in `$HOME/.supermanager/repos.json`.

To remove the repo from supermanager later:

```sh
supermanager leave
```

### 5. Use the dashboard

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
| `/r/{room_id}/feed` | GET | Get raw room hook events, newest first |
| `/r/{room_id}/feed/stream` | GET | SSE stream of hook-event and summary-status events |
| `/r/{room_id}/hooks/turn` | POST | Submit a hook-captured turn event |
| `/r/{room_id}/summary` | GET | Read the current room summary |

## Project structure

```text
crates/
  coordination-server/    # HTTP server, dashboard, room APIs
  reporter-protocol/      # Shared room and hook-event types
  supermanager-cli/       # Global CLI for joining/leaving repos
Dockerfile                # Production image
fly.toml                  # Fly deployment config
```

## Notes

- Summary generation runs on the server after new hook turns arrive.
