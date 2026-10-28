#!/usr/bin/env bash
# Builds ledger-mcp in release and copies it where Tauri's externalBin expects it.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"

cd "$ROOT"
cargo build --release -p ledger-mcp
mkdir -p app/src-tauri/binaries
cp target/release/ledger-mcp "app/src-tauri/binaries/ledger-mcp-$TRIPLE"
echo "$ROOT/app/src-tauri/binaries/ledger-mcp-$TRIPLE"
