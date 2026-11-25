#!/usr/bin/env bash
# Rehearses the INSTALL.md steps against the release disk image, without touching
# /Applications or the real ledger: checksum, attach at a scratch mount point, copy the app
# into a scratch Applications folder, detach, verify the signature, start the copied
# ledger-mcp against a scratch ledger and check its initialize reply.
# Usage: bash scripts/install-check.sh [--dmg <path>] [--unsigned]
# Exit codes: 0 INSTALL_OK, 2 MISSING_RELEASE, 1 any other failure.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DMG="$ROOT/dist/release/Freehold-Ledger-universal.dmg"
UNSIGNED=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dmg) DMG="${2:?--dmg needs a path}"; shift 2 ;;
    --unsigned) UNSIGNED=1; shift ;;
    *) echo "unknown argument: $1" >&2; exit 1 ;;
  esac
done

if [[ ! -f "$DMG" ]]; then
  echo "MISSING_RELEASE: run scripts/release.sh first" >&2
  exit 2
fi
DMG_DIR="$(cd "$(dirname "$DMG")" && pwd)"
DMG_NAME="$(basename "$DMG")"
if [[ ! -f "$DMG_DIR/SHA256SUMS" ]]; then
  echo "MISSING_RELEASE: run scripts/release.sh first (no SHA256SUMS beside the disk image)" >&2
  exit 2
fi

SCRATCH="$(mktemp -d)"
MOUNT="$SCRATCH/mount"
APPS="$SCRATCH/Applications"
MCP_PID=""
ATTACHED=0
cleanup() {
  exec 3>&- 2>/dev/null || true
  if [[ -n "$MCP_PID" ]]; then
    kill "$MCP_PID" 2>/dev/null || true
    sleep 0.2
    kill -9 "$MCP_PID" 2>/dev/null || true
  fi
  if [[ "$ATTACHED" -eq 1 ]]; then
    hdiutil detach "$MOUNT" -quiet 2>/dev/null || hdiutil detach "$MOUNT" -force -quiet 2>/dev/null || true
  fi
  rm -rf "$SCRATCH"
}
trap cleanup EXIT INT TERM

echo "1/6 checksum"
(cd "$DMG_DIR" && shasum -a 256 -c SHA256SUMS --ignore-missing 2>&1) || { echo "CHECKSUM_FAILED: nothing was installed" >&2; exit 1; }
if ! (cd "$DMG_DIR" && grep -q "$DMG_NAME" SHA256SUMS); then
  echo "CHECKSUM_FAILED: SHA256SUMS has no line for $DMG_NAME" >&2
  exit 1
fi

echo "2/6 attach (scratch mount point)"
mkdir -p "$MOUNT" "$APPS"
hdiutil attach "$DMG" -nobrowse -readonly -noverify -mountpoint "$MOUNT" -quiet
ATTACHED=1

echo "3/6 copy into a scratch Applications folder"
[[ -d "$MOUNT/Freehold Ledger.app" ]] || { echo "no Freehold Ledger.app on the disk image" >&2; exit 1; }
ditto "$MOUNT/Freehold Ledger.app" "$APPS/Freehold Ledger.app"

echo "4/6 detach"
hdiutil detach "$MOUNT" -quiet
ATTACHED=0

APP="$APPS/Freehold Ledger.app"
if [[ "$UNSIGNED" -eq 1 ]]; then
  echo "5/6 signature skipped (--unsigned)"
else
  echo "5/6 signature"
  codesign --verify --deep --strict "$APP"
fi

echo "6/6 ledger-mcp initialize"
MCP_BIN="$APP/Contents/MacOS/ledger-mcp"
[[ -x "$MCP_BIN" ]] || { echo "ledger-mcp is missing or not executable in the copied app" >&2; exit 1; }
export FREEHOLD_LEDGER_DB="$SCRATCH/ledger.sqlite3"
mkfifo "$SCRATCH/mcp.in"
"$MCP_BIN" <"$SCRATCH/mcp.in" >"$SCRATCH/mcp.out" 2>"$SCRATCH/mcp.err" &
MCP_PID=$!
exec 3>"$SCRATCH/mcp.in"   # hold stdin open
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"check-no-listeners","version":"0"}}}' >&3
for _ in $(seq 1 50); do
  grep -q '"freehold-ledger"' "$SCRATCH/mcp.out" 2>/dev/null && break
  kill -0 "$MCP_PID" 2>/dev/null || break
  sleep 0.2
done
if ! grep -q '"freehold-ledger"' "$SCRATCH/mcp.out"; then
  echo "initialize response lacks \"freehold-ledger\":" >&2
  cat "$SCRATCH/mcp.out" "$SCRATCH/mcp.err" >&2
  exit 1
fi

echo INSTALL_OK
