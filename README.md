# Supermanager

Real-time visibility into what your coding agents are doing. Supermanager is a coordination server that collects progress updates from Claude Code and Codex sessions across your team, and serves them as a feed.

## How it works

Coding agents (Claude Code, Codex) are instructed to call `submit_progress` throughout their work — when they start a task, make progress, hit a blocker, or finish. The coordination server stores these updates and serves them as a feed.

The agents connect to the server via MCP (Model Context Protocol). No polling, no cron jobs — the agent reports as it works.

## Setup

### 1. Start the server

```
cargo run -p coordination-server
```

Listens on `http://127.0.0.1:8787` by default. Use `--bind` to change.

### 2. Install the employee plugins

```
./install.sh
```

This configures both Claude Code and Codex (whichever are installed):

- Registers the MCP server
- Installs the employee-side Claude Code plugin (which includes agent instructions via `CLAUDE.md`)
- Appends agent instructions to `~/.codex/AGENTS.md` for Codex

### 3. Run the centralized Claude observer with channels

The centralized observer is a separate Claude plugin from the employee plugin. It subscribes to the coordination server's SSE feed and forwards each incoming note into a Claude Code session through a Claude channel.

Requirements:

- Node.js 18+ on the machine running the centralized Claude session
- Claude Code signed in with `claude.ai`, since channels currently require that mode

```sh
claude \
  --plugin-dir "$PWD/plugins/supermanager-channel" \
  --dangerously-load-development-channels server:supermanager_channel
```

By default the plugin reads from `http://127.0.0.1:8787/v1/feed/stream`. Override that with `SUPERMANAGER_SSE_URL` if needed.

### 4. Use it

Start a Claude Code or Codex session and give it a task. The agent will automatically report progress to the server. Check the feed:

```
curl http://127.0.0.1:8787/v1/feed | jq
```

Or tail the live SSE stream:

```sh
curl -N http://127.0.0.1:8787/v1/feed/stream
```

## API

| Endpoint | Method | Description |
|---|---|---|
| `/health` | GET | Health check |
| `/v1/progress` | POST | Submit a progress note |
| `/v1/feed` | GET | Get all notes, newest first |
| `/v1/feed/stream` | GET | Server-Sent Events stream of new notes, with replay support via `Last-Event-ID` |
| `/mcp` | POST | MCP endpoint for agent tool calls |

## MCP Tools

| Tool | Description |
|---|---|
| `submit_progress` | Submit a progress update (employee_name, repo, branch, progress_text) |
| `get_feed` | Get all progress updates |

## Project structure

```
crates/
  coordination-server/    # HTTP + MCP server
  reporter-protocol/      # Shared types (ProgressNote, etc.)
plugins/
  claude-reporter/        # Employee Claude Code plugin (CLAUDE.md + MCP config)
  codex-reporter/         # Codex plugin (MCP config)
  supermanager-channel/   # Centralized Claude channel plugin backed by SSE
INSTRUCTIONS.md           # Agent instructions (single source of truth)
install.sh                # One-step setup for both Claude Code and Codex
```

## Customizing agent instructions

Edit `INSTRUCTIONS.md` at the repo root. Both plugins read from this file:

- The Claude Code plugin imports it via `@../../INSTRUCTIONS.md` in its `CLAUDE.md`
- The install script copies it into `~/.codex/AGENTS.md` for Codex
