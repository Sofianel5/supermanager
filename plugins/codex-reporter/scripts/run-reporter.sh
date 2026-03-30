#!/usr/bin/env sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
PLUGIN_DIR=$(CDPATH= cd -- "${SCRIPT_DIR}/.." && pwd)
REPO_ROOT=$(CDPATH= cd -- "${PLUGIN_DIR}/../.." && pwd)

BIN="${REPORTER_CLI_BIN:-}"
if [ -z "${BIN}" ] && [ -x "${PLUGIN_DIR}/bin/reporter-cli" ]; then
  BIN="${PLUGIN_DIR}/bin/reporter-cli"
fi
if [ -z "${BIN}" ] && [ -x "${REPO_ROOT}/target/debug/reporter-cli" ]; then
  BIN="${REPO_ROOT}/target/debug/reporter-cli"
fi
if [ -z "${BIN}" ] && [ -x "${REPO_ROOT}/target/release/reporter-cli" ]; then
  BIN="${REPO_ROOT}/target/release/reporter-cli"
fi
if [ -z "${BIN}" ]; then
  BIN=$(command -v reporter-cli || true)
fi

if [ -z "${BIN}" ]; then
  echo "reporter-cli binary not found" >&2
  exit 1
fi

KIND="${1:-}"
if [ -z "${KIND}" ]; then
  echo "usage: run-reporter.sh <intent|progress>" >&2
  exit 1
fi

exec "${BIN}" submit-progress --host codex --kind "${KIND}"
