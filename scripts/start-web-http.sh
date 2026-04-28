#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

WEB_PORT="${WEB_PORT:-3000}"
UI_PORT="${UI_PORT:-5173}"
UI_HOST="${UI_HOST:-0.0.0.0}"
UI_LOG_LEVEL="${UI_LOG_LEVEL:-warn}"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/.audit-work}"
CORS_ORIGIN="${CORS_ORIGIN:-*}"

# In WSL2 the browser runs on Windows and cannot reach the WSL2 backend
# directly.  Route all /api traffic through Vite's built-in proxy instead:
# the browser only ever talks to localhost:UI_PORT (which Windows forwards
# automatically), and Vite proxies /api to the backend on the WSL2 side.
WSL2_IP="$(ip -4 addr show eth0 2>/dev/null | awk '/inet /{print $2}' | cut -d/ -f1 | head -1)"
VITE_API_PROXY_TARGET="${VITE_API_PROXY_TARGET:-http://${WSL2_IP:-localhost}:${WEB_PORT}}"
# Tell the frontend to send API calls to the same origin (Vite proxy handles the rest).
VITE_API_URL="${VITE_API_URL:-http://localhost:${UI_PORT}}"

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

echo "[http-ui] Starting frontend dev server (bind $UI_HOST)"
(
  cd "$ROOT_DIR/ui"
  export VITE_TRANSPORT=http
  export VITE_API_URL
  export VITE_API_PROXY_TARGET
  npm run dev -- \
    --host "$UI_HOST" \
    --port "$UI_PORT" \
    --strictPort \
    --logLevel "$UI_LOG_LEVEL"
) &
frontend_pid=$!

echo "[http-ui] UI:  http://localhost:$UI_PORT/wizard"
echo "[http-ui] API: http://localhost:$WEB_PORT  (proxied via Vite from $VITE_API_PROXY_TARGET)"
wait -n "$backend_pid" "$frontend_pid"
