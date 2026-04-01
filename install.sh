#!/usr/bin/env sh
# Local development install script.
# This configures Claude Code and Codex to talk to a local coordination server
# running at http://127.0.0.1:8787 (the "__local" room).
#
# For hosted rooms, use:
#   curl -sSf https://your-server/r/{room_id}/install?secret=your_secret | sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
INSTRUCTIONS_FILE="$SCRIPT_DIR/INSTRUCTIONS.md"

echo "Installing supermanager (local dev)..."

install_claude_settings() {
  SETTINGS_FILE="${HOME}/.claude/settings.json"
  mkdir -p "$(dirname "$SETTINGS_FILE")"
  [ -f "$SETTINGS_FILE" ] || printf '{\n  "permissions": {\n    "allow": []\n  }\n}\n' > "$SETTINGS_FILE"

  python3 - "$SETTINGS_FILE" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
data = json.loads(path.read_text())
permissions = data.setdefault("permissions", {})
allow = permissions.setdefault("allow", [])
entry = "mcp__supermanager__submit_progress"

if entry not in allow:
    allow.append(entry)

path.write_text(json.dumps(data, indent=2) + "\n")
PY
}

install_codex_config() {
  CONFIG_FILE="${HOME}/.codex/config.toml"
  mkdir -p "$(dirname "$CONFIG_FILE")"
  touch "$CONFIG_FILE"

  python3 - "$CONFIG_FILE" <<'PY'
import sys
from pathlib import Path

path = Path(sys.argv[1])
lines = path.read_text().splitlines()
out = []
skip_marker = False
server_section = "[mcp_servers.supermanager]"
tool_section = "[mcp_servers.supermanager.tools.submit_progress]"
inside_server = False
inside_tool = False
seen_server = False
seen_tool = False
set_tool_approval = False

for line in lines:
    stripped = line.strip()

    if stripped == "# supermanager":
        skip_marker = True
        continue
    if skip_marker:
        if stripped == "# /supermanager":
            skip_marker = False
        continue

    if stripped.startswith("[") and stripped.endswith("]"):
        if inside_tool and not set_tool_approval:
            out.append('approval_mode = "approve"')
            set_tool_approval = True
        inside_server = stripped == server_section
        inside_tool = stripped == tool_section
        if stripped == server_section:
            seen_server = True
        if stripped == tool_section:
            seen_tool = True
        out.append(line)
        continue

    if inside_tool and stripped.startswith("approval_mode"):
        out.append('approval_mode = "approve"')
        set_tool_approval = True
    else:
        out.append(line)

if inside_tool and not set_tool_approval:
    out.append('approval_mode = "approve"')

if not seen_server:
    if out and out[-1] != "":
        out.append("")
    out.append(server_section)
    out.append('url = "http://127.0.0.1:8787/mcp"')

if not seen_tool:
    if out and out[-1] != "":
        out.append("")
    out.append(tool_section)
    out.append('approval_mode = "approve"')

path.write_text("\n".join(out) + "\n")
PY
}

install_codex_agents_instructions() {
  AGENTS_FILE="${HOME}/.codex/AGENTS.md"
  mkdir -p "$(dirname "$AGENTS_FILE")"
  touch "$AGENTS_FILE"
  marker="<!-- supermanager -->"
  tmp_file="$(mktemp)"

  awk -v marker="$marker" '
    BEGIN { skip = 0 }
    $0 == marker { skip = 1; next }
    skip && $0 == "<!-- /supermanager -->" { skip = 0; next }
    !skip { print }
  ' "$AGENTS_FILE" > "$tmp_file"

  mv "$tmp_file" "$AGENTS_FILE"
  [ -s "$AGENTS_FILE" ] && printf '\n' >> "$AGENTS_FILE"
  printf '%s\n' "$marker" >> "$AGENTS_FILE"
  cat "$INSTRUCTIONS_FILE" >> "$AGENTS_FILE"
  printf '\n%s\n' "<!-- /supermanager -->" >> "$AGENTS_FILE"
  echo "  Updated instructions in ${AGENTS_FILE}"
}

# --- Claude Code ---
if command -v claude >/dev/null 2>&1; then
  echo "Configuring Claude Code..."
  claude mcp add --transport http supermanager "http://127.0.0.1:8787/mcp" 2>/dev/null || true
  claude plugins add "$SCRIPT_DIR/plugins/claude-reporter" 2>/dev/null || true
  install_claude_settings
  echo "  Done."
else
  echo "  Claude Code not found, skipping."
fi

# --- Codex ---
if command -v codex >/dev/null 2>&1; then
  echo "Configuring Codex..."
  codex mcp add supermanager --url "http://127.0.0.1:8787/mcp" 2>/dev/null || true
  install_codex_config
  install_codex_agents_instructions
  echo "  Done."
else
  echo "  Codex not found, skipping."
fi

echo ""
echo "Make sure the coordination server is running:"
echo "  cargo run -p coordination-server"
