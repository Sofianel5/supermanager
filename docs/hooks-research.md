# Hooks Research

How to use Claude Code and Codex hooks to automatically report progress — replacing the current approach of relying on agents to call `submit_progress` via MCP.

## Why hooks?

The current install script injects instructions into CLAUDE.md/AGENTS.md telling agents to call `submit_progress`. This has problems:
- Agents can skip/forget the instruction
- Each call costs tokens
- Reporting quality depends on the agent's judgment

Hooks are **guaranteed, zero-token, can't be skipped**.

---

## Claude Code Hooks

### Config location

Hooks go in `settings.json` under the `"hooks"` key:

| Scope | File | Shared? |
|-------|------|---------|
| User | `~/.claude/settings.json` | No |
| Project | `.claude/settings.json` | Yes (git) |
| Local | `.claude/settings.local.json` | No |

### Hook types

| Type | Description |
|------|-------------|
| `command` | Shell script. Gets JSON on stdin. |
| `http` | Claude Code POSTs JSON directly to a URL. No script needed. |
| `prompt` | Sends to a fast LLM for yes/no decision. |
| `agent` | Spawns a Claude subagent. |

### Available events (25 total)

Most useful for supermanager:

| Event | What it sees | Use case |
|-------|-------------|----------|
| `SessionStart` | `source` (startup/resume) | Report "started working" |
| `UserPromptSubmit` | `prompt` (user's message) | Report what was asked |
| `PostToolUse` | `tool_name`, `tool_input`, `tool_response` | Report each action |
| `Stop` | `last_assistant_message` | Report agent's summary |

Full list: SessionStart, InstructionsLoaded, UserPromptSubmit, PreToolUse, PermissionRequest, PostToolUse, PostToolUseFailure, Notification, SubagentStart, SubagentStop, TaskCreated, TaskCompleted, Stop, StopFailure, TeammateIdle, ConfigChange, CwdChanged, FileChanged, PreCompact, PostCompact, Elicitation, ElicitationResult, WorktreeCreate, WorktreeRemove, SessionEnd.

### Common input fields (all hooks)

```json
{
  "session_id": "abc123",
  "transcript_path": "~/.claude/projects/.../00893aaf.jsonl",
  "cwd": "/Users/b/Documents/supermanager",
  "permission_mode": "default",
  "hook_event_name": "Stop"
}
```

Conditional: `agent_id`, `agent_type` (present inside subagent calls).

### Event-specific payloads

**Stop:**
```json
{
  "stop_hook_active": true,
  "last_assistant_message": "I've completed the refactoring. Here's a summary..."
}
```

**UserPromptSubmit:**
```json
{
  "prompt": "fix the login bug on the dashboard"
}
```

**PostToolUse:**
```json
{
  "tool_name": "Edit",
  "tool_input": { "file_path": "/src/auth.ts", "old_string": "...", "new_string": "..." },
  "tool_response": { "filePath": "/src/auth.ts", "success": true },
  "tool_use_id": "toolu_01ABC123..."
}
```

**SessionStart:**
```json
{
  "source": "startup"
}
```

### Example: HTTP hook (no script needed)

```json
{
  "hooks": {
    "Stop": [{
      "hooks": [{
        "type": "http",
        "url": "https://server/r/{room_id}/hooks/stop?secret={secret}&employee=Bryan+Chiang",
        "timeout": 5
      }]
    }],
    "UserPromptSubmit": [{
      "hooks": [{
        "type": "http",
        "url": "https://server/r/{room_id}/hooks/prompt?secret={secret}&employee=Bryan+Chiang",
        "timeout": 5
      }]
    }]
  }
}
```

Claude Code POSTs the full hook JSON payload directly to the URL. The coordination server would need new endpoints to accept this format.

### Example: Command hook (bash + curl)

```bash
#!/bin/bash
# .claude/hooks/supermanager-stop.sh
INPUT=$(cat)
MSG=$(echo "$INPUT" | python3 -c "import sys,json; print(json.load(sys.stdin).get('last_assistant_message','')[:500])")
EMPLOYEE=$(git config user.name)
REPO=$(git remote get-url origin 2>/dev/null || echo "unknown")
BRANCH=$(git branch --show-current 2>/dev/null || echo "unknown")

curl -s -X POST "https://server/r/{room_id}/submit_progress?secret={secret}" \
  -H "Content-Type: application/json" \
  -d "{\"employee_name\":\"$EMPLOYEE\",\"repo\":\"$REPO\",\"branch\":\"$BRANCH\",\"progress_text\":\"$MSG\"}" &
```

Works with existing `submit_progress` API. No new endpoints needed.

---

## Codex Hooks

### Config location

Hooks use a **separate `hooks.json` file** (not inside config.toml):

| Scope | File |
|-------|------|
| User | `~/.codex/hooks.json` |
| Repo | `<repo>/.codex/hooks.json` |

**Important:** Hooks are off by default. Must set `features.codex_hooks = true` in `config.toml`.

### Hook types

**Command only.** No HTTP/prompt/agent types.

### Available events (5 total)

| Event | What it sees |
|-------|-------------|
| `SessionStart` | `source` (startup/resume) |
| `UserPromptSubmit` | `prompt` |
| `PreToolUse` | `tool_name`, `tool_input` |
| `PostToolUse` | `tool_name`, `tool_input`, `tool_response` |
| `Stop` | `last_assistant_message` |

### Common input fields (all hooks)

```json
{
  "session_id": "string",
  "transcript_path": "string | null",
  "cwd": "string",
  "hook_event_name": "string",
  "model": "string",
  "turn_id": "string"
}
```

Note: Codex includes `model` and `turn_id` which Claude Code does not.

### Example: hooks.json

```json
{
  "hooks": {
    "Stop": [{
      "hooks": [{
        "type": "command",
        "command": ".codex/hooks/supermanager-stop.sh",
        "timeout": 10
      }]
    }]
  }
}
```

---

## Comparison

| | Claude Code | Codex |
|---|---|---|
| Config file | `settings.json` (under `"hooks"`) | Standalone `hooks.json` |
| Project scope | `.claude/settings.json` | `.codex/hooks.json` |
| Hook types | command, http, prompt, agent | command only |
| Events | 25 | 5 |
| Enabled by default | Yes | No |
| Has `model` field | No | Yes |
| Has `permission_mode` | No (wait, yes) | No |
| HTTP hooks | Yes (no script needed) | No (must use bash + curl) |

---

## Recommended approach

### For Claude Code: Use `http` hooks

No script files needed. Install script just merges JSON into `.claude/settings.json`:

```python
python3 -c "
import json, os
path = '.claude/settings.json'
os.makedirs('.claude', exist_ok=True)
cfg = {}
if os.path.exists(path):
    with open(path) as f:
        cfg = json.load(f)
cfg['hooks'] = {
    'Stop': [{'hooks': [{'type': 'http', 'url': 'URL_HERE', 'timeout': 5}]}],
    'UserPromptSubmit': [{'hooks': [{'type': 'http', 'url': 'URL_HERE', 'timeout': 5}]}],
    'SessionStart': [{'matcher': 'startup', 'hooks': [{'type': 'http', 'url': 'URL_HERE', 'timeout': 5}]}]
}
with open(path, 'w') as f:
    json.dump(cfg, f, indent=2)
"
```

Requires new server endpoints that accept the hook JSON format.

### For Codex: Use `command` hooks

Need a script file + hooks.json + feature flag:

1. Drop `.codex/hooks/supermanager-stop.sh`
2. Create `.codex/hooks.json`
3. Enable `features.codex_hooks = true` in config.toml

### Server-side changes needed

For the HTTP hook approach, the coordination server needs endpoints like:

- `POST /r/{room_id}/hooks/stop` — accepts Stop payload, extracts `last_assistant_message`
- `POST /r/{room_id}/hooks/prompt` — accepts UserPromptSubmit payload, extracts `prompt`
- `POST /r/{room_id}/hooks/session-start` — accepts SessionStart payload

These map the hook payloads to `submit_progress` internally. Employee name, repo, branch come from query params baked in at install time.

---

## Hook return values (bidirectional communication)

Hooks aren't just fire-and-forget — they can return JSON that gets injected back into the Claude Code conversation. This enables the server to push context into the agent's thread.

### Common output fields (all hooks)

```json
{
  "continue": true,
  "stopReason": "Message shown to user",
  "suppressOutput": false,
  "systemMessage": "Injected as a system message Claude sees"
}
```

### Event-specific outputs

**UserPromptSubmit / PostToolUse** — inject `additionalContext`:
```json
{
  "hookSpecificOutput": {
    "hookEventName": "UserPromptSubmit",
    "additionalContext": "Manager note: focus on the auth refactor today"
  }
}
```

**PreToolUse** — can allow/deny/modify tool calls:
```json
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "deny",
    "permissionDecisionReason": "Blocked by policy",
    "updatedInput": { "command": "npm run lint" },
    "additionalContext": "Extra info for Claude"
  }
}
```

**Stop** — can force Claude to keep going:
```json
{
  "decision": "block",
  "reason": "You haven't run tests yet"
}
```

### How hooks return values

| Hook type | How to return | Format |
|-----------|---------------|--------|
| `command` | Write JSON to stdout | Exit code 0 = parse stdout as JSON |
| `http` | HTTP response body | 2xx + JSON body = parse fields |

Exit code 2 (command) or non-2xx (http) = blocking error, stderr/body shown to user.

### Supermanager use case: inject manager context

This is the killer feature. A `UserPromptSubmit` hook could:

1. Fire on every user message
2. Hit the coordination server
3. Server responds with the manager's latest summary/priorities
4. That gets injected as `additionalContext` — Claude sees it as context automatically

```json
// Hook config
{
  "hooks": {
    "UserPromptSubmit": [{
      "hooks": [{
        "type": "http",
        "url": "https://server/r/{room_id}/hooks/prompt-context?secret={secret}&employee=Bryan+Chiang",
        "timeout": 5
      }]
    }]
  }
}
```

```json
// Server response
{
  "hookSpecificOutput": {
    "hookEventName": "UserPromptSubmit",
    "additionalContext": "Manager priorities: 1) Ship auth refactor by Friday 2) Don't touch the payments module"
  }
}
```

This means:
- Manager updates priorities on the dashboard
- Every agent conversation automatically sees them
- No MCP call needed, no tokens spent on fetching
- Agent doesn't need to remember to check `get_manager_summary`

Could also inject the team feed, recent progress from other agents, or blockers — whatever the server wants to push.

### Codex limitation

Codex only supports `command` hooks, so the script would need to curl the server, capture the response, and echo JSON to stdout. Same result, just requires a script file.

---

### What this replaces

With hooks in place, the CLAUDE.md/AGENTS.md instructions to call `submit_progress` become optional — hooks handle the guaranteed baseline reporting. The MCP `submit_progress` tool can still exist for agents that want to send richer/curated updates beyond what the hooks capture.
