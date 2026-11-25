#!/usr/bin/env bash
# Builds ledger-mcp in release and copies it where Tauri's externalBin expects it.
#
#   build-sidecar.sh                         host triple (as before)
#   build-sidecar.sh universal-apple-darwin  Apple Silicon + Intel joined with lipo
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOST="$(rustc -vV | sed -n 's/^host: //p')"
TARGET="${1:-}"
# Tauri hands hook commands the triple it builds for, host triple included:
# that is the plain build, so it keeps the host path.
if [[ "$TARGET" == "$HOST" ]]; then TARGET=""; fi

cd "$ROOT"
mkdir -p app/src-tauri/binaries

if [[ -z "$TARGET" ]]; then
  cargo build --release -p ledger-mcp
  cp target/release/ledger-mcp "app/src-tauri/binaries/ledger-mcp-$HOST"
  echo "$ROOT/app/src-tauri/binaries/ledger-mcp-$HOST"
elif [[ "$TARGET" == "universal-apple-darwin" ]]; then
  for t in aarch64-apple-darwin x86_64-apple-darwin; do
    if ! rustup target list --installed | grep -qx "$t"; then
      rustup target add "$t"
    fi
    cargo build --release -p ledger-mcp --target "$t"
    # Tauri's per-architecture compile steps look for the plain triple name too.
    cp "target/$t/release/ledger-mcp" "app/src-tauri/binaries/ledger-mcp-$t"
  done
  lipo -create \
    "target/aarch64-apple-darwin/release/ledger-mcp" \
    "target/x86_64-apple-darwin/release/ledger-mcp" \
    -output "app/src-tauri/binaries/ledger-mcp-universal-apple-darwin"
  echo "$ROOT/app/src-tauri/binaries/ledger-mcp-universal-apple-darwin"
else
  TRIPLE="$TARGET"
  if ! rustup target list --installed | grep -qx "$TRIPLE"; then
    rustup target add "$TRIPLE"
  fi
  cargo build --release -p ledger-mcp --target "$TRIPLE"
  cp "target/$TRIPLE/release/ledger-mcp" "app/src-tauri/binaries/ledger-mcp-$TRIPLE"
  echo "$ROOT/app/src-tauri/binaries/ledger-mcp-$TRIPLE"
fi
