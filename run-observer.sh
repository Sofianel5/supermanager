#!/usr/bin/env sh
# Run the supermanager observer (Claude with the SSE channel plugin).
#
# For local dev, no extra env vars are needed (defaults to localhost:8787).
#
# For hosted rooms:
#   SUPERMANAGER_ROOM_URL=https://supermanager.fly.dev/r/bright-fox-42
#   SUPERMANAGER_ROOM_SECRET=sm_sec_abc123
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname "$0")" && pwd)

exec claude \
  --plugin-dir "$SCRIPT_DIR/plugins/supermanager-channel" \
  --allowedTools "mcp__supermanager__get_feed,mcp__supermanager__get_manager_summary,mcp__supermanager__update_manager_summary" \
  --dangerously-load-development-channels server:supermanager_channel \
  "$@"
