# supermanager-channel

Claude Code channel plugin for a centralized supermanager observer session.

Requirements:

- Node.js 18+
- Claude Code signed in with `claude.ai`

Run Claude Code with this plugin enabled and channels allowed:

```sh
claude \
  --plugin-dir /absolute/path/to/plugins/supermanager-channel \
  --dangerously-load-development-channels server:supermanager_channel
```

Set `SUPERMANAGER_SSE_URL` if the coordination server is not at `http://127.0.0.1:8787/v1/feed/stream`.
