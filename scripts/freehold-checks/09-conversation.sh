#!/usr/bin/env bash
# Freehold Standard Point 9: Usable by conversation.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT" || exit 1

[ -f docs/demo/scene-transcript.md ] || { echo "docs/demo/scene-transcript.md is missing"; exit 1; }
out="$(cargo test -q -p ledger-mcp --test stdio s_mcp_2 2>&1)"; rc=$?
if [ $rc -ne 0 ]; then echo "$out" | tail -3; exit 1; fi
if [ "${FREEHOLD_CHECK_FULL:-}" = "1" ]; then
  bash scripts/scene-check.sh || exit 1
fi
echo "the founding scene runs by conversation (s_mcp_2 passes)"
