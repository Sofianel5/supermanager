# Hook Reporting Design

Supermanager reports work through Claude Code and Codex hooks rather than a separate MCP reporting tool.

## Implemented direction

- Claude Code uses project-local command hooks in `.claude/settings.local.json`
- Codex uses repo-local command hooks in `.codex/hooks.json`
- Both tools run hooks for `UserPromptSubmit` and `Stop`
- Both tools call `supermanager hook-report --client ...`
- The CLI reads the raw hook payload from `stdin`, resolves repo metadata, and posts metadata plus the raw payload to `POST /r/{room_id}/hooks/turn`
- The server stores the raw hook event, serves it directly in the feed, and forwards raw JSON updates to the summary model

## Why command hooks for both

Claude Code supports direct HTTP hooks, but Supermanager uses command hooks for both tools so the CLI can compute repo-specific metadata before posting:

- `repo_root` from `git rev-parse --show-toplevel`
- `branch` from `git branch --show-current`
- `employee_name` from git config or the local shell user

Using one native transport path keeps the server endpoint simple and makes Claude and Codex behave the same way without generated helper scripts or duplicated hook-schema logic in the CLI.

## Payload shape

`supermanager hook-report` POSTs JSON like this:

```json
{
  "employee_name": "Jane Doe",
  "client": "codex",
  "repo_root": "/Users/jane/project",
  "branch": "feature/hooks",
  "payload": {
    "hook_event_name": "Stop",
    "session_id": "abc123",
    "turn_id": "turn_456",
    "cwd": "/Users/jane/project",
    "last_assistant_message": "Implemented the hook-based reporting flow and updated the docs."
  }
}
```

## Installed files

`supermanager join` now manages these local files:

- `.claude/settings.local.json`
- `.codex/config.toml`
- `.codex/hooks.json`
- `$HOME/.supermanager/repos.json`

It also removes the old repo-local supermanager MCP config and injected instruction blocks if they exist.

## Server behavior

When a hook event arrives:

1. The room code is resolved
2. The raw event is stored in `hook_events`
3. SSE subscribers receive the stored hook event
4. The background summary job reruns

The summary model receives raw JSON lines containing the stored hook metadata plus the original hook payload.
