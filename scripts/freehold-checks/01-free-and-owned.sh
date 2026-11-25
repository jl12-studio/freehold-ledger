#!/usr/bin/env bash
# Freehold Standard Point 1: Free, and owned by the person.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT" || exit 1

grep -q "GNU AFFERO GENERAL PUBLIC LICENSE" LICENSE || { echo "LICENSE is not the AGPL"; exit 1; }
grep -q 'AGPL-3.0-or-later' Cargo.toml || { echo "Cargo.toml does not say AGPL-3.0-or-later"; exit 1; }
# A line holding the documentation placeholder /Users/you/ is not a real home path.
leaks="$(git grep -n "/Users/" -- . ':!app/src/__tests__' ':!app/src-tauri/src/commands.rs' | grep -v '/Users/you/')"
if [ -n "$leaks" ]; then
  echo "absolute home path in tracked file: $(echo "$leaks" | head -1 | cut -d: -f1,2)"
  exit 1
fi
echo "AGPL-3.0-or-later, no home paths in tracked files"
