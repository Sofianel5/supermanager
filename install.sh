#!/usr/bin/env sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
INSTRUCTIONS_FILE="$SCRIPT_DIR/INSTRUCTIONS.md"

echo "Installing supermanager..."

# --- Claude Code ---
if command -v claude >/dev/null 2>&1; then
  echo "Configuring Claude Code..."
  claude mcp add --transport http supermanager "http://127.0.0.1:8787/mcp" 2>/dev/null || true
  claude plugins add "$SCRIPT_DIR/plugins/claude-reporter" 2>/dev/null || true
  echo "  Done."
else
  echo "  Claude Code not found, skipping."
fi

# --- Codex ---
if command -v codex >/dev/null 2>&1; then
  echo "Configuring Codex..."
  codex mcp add supermanager --url "http://127.0.0.1:8787/mcp" 2>/dev/null || true

  AGENTS_FILE="${HOME}/.codex/AGENTS.md"
  MARKER="<!-- supermanager -->"
  if [ -f "$AGENTS_FILE" ] && grep -q "$MARKER" "$AGENTS_FILE"; then
    echo "  AGENTS.md already configured."
  else
    mkdir -p "$(dirname "$AGENTS_FILE")"
    printf '\n%s\n' "$MARKER" >> "$AGENTS_FILE"
    cat "$INSTRUCTIONS_FILE" >> "$AGENTS_FILE"
    printf '\n%s\n' "<!-- /supermanager -->" >> "$AGENTS_FILE"
    echo "  Added instructions to ${AGENTS_FILE}"
  fi
  echo "  Done."
else
  echo "  Codex not found, skipping."
fi

echo ""
echo "Make sure the coordination server is running:"
echo "  cargo run -p coordination-server"
