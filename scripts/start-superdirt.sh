#!/usr/bin/env bash
set -euo pipefail

PORT="${1:-57120}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT_PATH="$ROOT_DIR/supercollider/superdirt_startup.scd"

if [[ -n "${SCLANG_BIN:-}" ]]; then
  SCLANG_PATH="$SCLANG_BIN"
elif command -v sclang >/dev/null 2>&1; then
  SCLANG_PATH="$(command -v sclang)"
elif [[ -x "/Applications/SuperCollider.app/Contents/MacOS/sclang" ]]; then
  SCLANG_PATH="/Applications/SuperCollider.app/Contents/MacOS/sclang"
else
  echo "could not locate sclang; set SCLANG_BIN or install SuperCollider" >&2
  exit 1
fi

exec "$SCLANG_PATH" "$SCRIPT_PATH" "$PORT"
