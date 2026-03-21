#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT_DIR"

WEB_PORT="${WEB_PORT:-3000}"
UI_PORT="${UI_PORT:-5173}"
UI_HOST="${UI_HOST:-0.0.0.0}"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/.audit-work}"
CORS_ORIGIN="${CORS_ORIGIN:-*}"

backend_pid=""
frontend_pid=""

cleanup() {
  local status=$?
  trap - EXIT INT TERM

  if [[ -n "$frontend_pid" ]] && kill -0 "$frontend_pid" 2>/dev/null; then
    kill "$frontend_pid" 2>/dev/null || true
  fi

  if [[ -n "$backend_pid" ]] && kill -0 "$backend_pid" 2>/dev/null; then
    kill "$backend_pid" 2>/dev/null || true
  fi

  wait "$frontend_pid" 2>/dev/null || true
  wait "$backend_pid" 2>/dev/null || true

  exit "$status"
}

trap cleanup EXIT INT TERM

if [[ ! -d "$ROOT_DIR/ui/node_modules" ]]; then
  echo "[http-ui] Installing frontend dependencies..."
  (
    cd "$ROOT_DIR/ui"
    npm install
  )
fi

echo "[http-ui] Starting backend: http://localhost:$WEB_PORT"
cargo run -p audit-agent-web -- \
  --port "$WEB_PORT" \
  --work-dir "$WORK_DIR" \
  --cors-origin "$CORS_ORIGIN" &
backend_pid=$!

echo "[http-ui] Starting frontend: http://localhost:$UI_PORT (bind $UI_HOST)"
(
  cd "$ROOT_DIR/ui"
  VITE_TRANSPORT=http \
  npm run dev -- --host "$UI_HOST" --port "$UI_PORT" --strictPort
) &
frontend_pid=$!

echo "[http-ui] Open in Windows browser: http://localhost:$UI_PORT/wizard"
wait -n "$backend_pid" "$frontend_pid"
