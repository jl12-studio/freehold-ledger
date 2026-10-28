#!/usr/bin/env bash
# Proves D15/S-SEC-1: the built app and its bundled ledger-mcp open no TCP LISTEN
# and no UDP sockets. Builds nothing; needs the .app from
# `pnpm --dir app tauri build --bundles app`. Always uses a scratch ledger.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP="$ROOT/target/release/bundle/macos/Freehold Ledger.app"
APP_BIN="$APP/Contents/MacOS/freehold-ledger-app"
MCP_BIN="$APP/Contents/MacOS/ledger-mcp"

if [[ ! -x "$APP_BIN" || ! -x "$MCP_BIN" ]]; then
  echo "MISSING_BUNDLE: build it first with: pnpm --dir app tauri build --bundles app" >&2
  exit 2
fi

SCRATCH="$(mktemp -d)"
APP_PID=""
MCP_PID=""
cleanup() {
  exec 3>&- 2>/dev/null || true
  for pid in $APP_PID $MCP_PID; do
    kill "$pid" 2>/dev/null || true
  done
  sleep 0.3
  for pid in $APP_PID $MCP_PID; do
    kill -9 "$pid" 2>/dev/null || true
  done
  rm -rf "$SCRATCH"
}
trap cleanup EXIT INT TERM

export FREEHOLD_LEDGER_DB="$SCRATCH/ledger.sqlite3"

"$APP_BIN" >"$SCRATCH/app.log" 2>&1 &
APP_PID=$!

mkfifo "$SCRATCH/mcp.in"
"$MCP_BIN" <"$SCRATCH/mcp.in" >"$SCRATCH/mcp.out" 2>"$SCRATCH/mcp.err" &
MCP_PID=$!
exec 3>"$SCRATCH/mcp.in"   # hold stdin open
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"check-no-listeners","version":"0"}}}' >&3

sleep 3

kill -0 "$APP_PID" 2>/dev/null || { echo "app exited early:" >&2; cat "$SCRATCH/app.log" >&2; exit 1; }
kill -0 "$MCP_PID" 2>/dev/null || { echo "ledger-mcp exited early:" >&2; cat "$SCRATCH/mcp.err" >&2; exit 1; }

if ! grep -q '"freehold-ledger"' "$SCRATCH/mcp.out"; then
  echo "initialize response lacks \"freehold-ledger\":" >&2
  cat "$SCRATCH/mcp.out" >&2
  exit 1
fi

FOUND=0
for pid in "$APP_PID" "$MCP_PID"; do
  tcp="$(lsof -nP -a -p "$pid" -iTCP -sTCP:LISTEN 2>/dev/null || true)"
  udp="$(lsof -nP -a -p "$pid" -iUDP 2>/dev/null || true)"
  if [[ -n "$tcp" ]]; then echo "TCP LISTEN for PID $pid:"; echo "$tcp"; FOUND=1; fi
  if [[ -n "$udp" ]]; then echo "UDP socket for PID $pid:"; echo "$udp"; FOUND=1; fi
done

if [[ "$FOUND" -ne 0 ]]; then exit 1; fi
echo NO_LISTENERS
