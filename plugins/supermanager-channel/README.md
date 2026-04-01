# supermanager-channel

Claude Code channel plugin for a centralized supermanager observer session.

This observer is meant to keep the server-backed manager summary document up to date for the human manager. The summary lives on the coordination server as `data/manager-summary.md` and is updated through the `get_manager_summary` and `update_manager_summary` tools exposed by the `supermanager` MCP server.

Requirements:

- Node.js 18+
- Claude Code signed in with `claude.ai`

Run Claude Code with this plugin enabled and channels allowed:

```sh
/absolute/path/to/supermanager/run-observer.sh
```

Equivalent manual launch:

```sh
claude \
  --plugin-dir /absolute/path/to/plugins/supermanager-channel \
  --allowedTools "mcp__supermanager__get_feed,mcp__supermanager__get_manager_summary,mcp__supermanager__update_manager_summary" \
  --dangerously-load-development-channels server:supermanager_channel
```

Set `SUPERMANAGER_SSE_URL` if the coordination server is not at `http://127.0.0.1:8787/v1/feed/stream`.
