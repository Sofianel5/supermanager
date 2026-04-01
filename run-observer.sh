#!/usr/bin/env sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname "$0")" && pwd)

exec claude \
  --plugin-dir "$SCRIPT_DIR/plugins/supermanager-channel" \
  --allowedTools "mcp__supermanager__get_feed,mcp__supermanager__get_manager_summary,mcp__supermanager__update_manager_summary" \
  --dangerously-load-development-channels server:supermanager_channel \
  "$@"
