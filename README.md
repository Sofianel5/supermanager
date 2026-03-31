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

### 2. Install the plugins

```
./install.sh
```

This configures both Claude Code and Codex (whichever are installed):

- Registers the MCP server
- Installs the Claude Code plugin (which includes agent instructions via `CLAUDE.md`)
- Appends agent instructions to `~/.codex/AGENTS.md` for Codex

### 3. Use it

Start a Claude Code or Codex session and give it a task. The agent will automatically report progress to the server. Check the feed:

```
curl http://127.0.0.1:8787/v1/feed | jq
```

## API

| Endpoint | Method | Description |
|---|---|---|
| `/health` | GET | Health check |
| `/v1/progress` | POST | Submit a progress note |
| `/v1/feed` | GET | Get all notes, newest first |
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
  claude-reporter/        # Claude Code plugin (CLAUDE.md + MCP config)
  codex-reporter/         # Codex plugin (MCP config)
INSTRUCTIONS.md           # Agent instructions (single source of truth)
install.sh                # One-step setup for both Claude Code and Codex
```

## Customizing agent instructions

Edit `INSTRUCTIONS.md` at the repo root. Both plugins read from this file:

- The Claude Code plugin imports it via `@../../INSTRUCTIONS.md` in its `CLAUDE.md`
- The install script copies it into `~/.codex/AGENTS.md` for Codex
