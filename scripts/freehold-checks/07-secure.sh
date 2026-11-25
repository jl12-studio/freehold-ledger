#!/usr/bin/env bash
# Freehold Standard Point 7: Secure by default.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT" || exit 1

if [ ! -d "target/release/bundle/macos/Freehold Ledger.app" ] && [ ! -d "target/universal-apple-darwin/release/bundle/macos/Freehold Ledger.app" ]; then
  echo "build the app first"
  exit 4
fi
bash scripts/check-no-listeners.sh || exit 1
out="$(cargo test -q -p ledger-core --test guard 2>&1)"; rc=$?
if [ $rc -ne 0 ]; then echo "$out" | tail -3; exit 1; fi
echo "no listeners, card and account numbers refused"
