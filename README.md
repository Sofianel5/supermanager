# Supermanager Progress Reporter

This repo now contains only the narrow MVP:

- a tiny local `reporter-cli` binary that plugins call from hooks
- a `coordination-server` that ingests freeform progress notes
- Claude Code and Codex plugin folders that wire hook payloads into the CLI

## Flow

1. `UserPromptSubmit` sends an `intent` note to the coordination server.
2. `Stop` sends a `progress` note to the coordination server.
3. Every note is tagged with an employee name.
4. The coordination server stores notes as they arrive and rewrites one global rolling Markdown report.

## Workspace layout

```text
crates/
  reporter-cli/
  coordination-server/
plugins/
  claude-reporter/
  codex-reporter/
```

## Local development

Build:

```bash
cargo build
```

Run the coordination server:

```bash
cargo run -p coordination-server
```

Submit a note manually:

```bash
printf '{"prompt":"Ship the reporting MVP"}' \
  | REPORTER_SERVER_URL=http://127.0.0.1:8787 \
    REPORTER_EMPLOYEE_NAME=alice \
    cargo run -p reporter-cli -- submit-progress --host claude --kind intent
```

Read the current global rolling report:

```bash
curl 'http://127.0.0.1:8787/v1/report/current'
```

## Model-backed report updates

The coordination server requires `OPENAI_API_KEY` and calls the Responses API to rewrite the rolling report after each incoming note. If the model call fails, the ingest request fails.

Environment variables:

- `OPENAI_API_KEY`
- `OPENAI_MODEL` default: `gpt-5`
- `OPENAI_BASE_URL` default: `https://api.openai.com/v1/responses`
- `REPORTER_EMPLOYEE_NAME` required on the plugin/client side

## Plugin wrappers

Each wrapper script looks for `reporter-cli` in this order:

1. `REPORTER_CLI_BIN`
2. `plugins/<host>-reporter/bin/reporter-cli`
3. `target/debug/reporter-cli`
4. `target/release/reporter-cli`
5. `reporter-cli` on `PATH`
