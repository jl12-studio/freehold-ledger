#!/usr/bin/env bash
# Freehold Standard Point 10: Teaches while it works.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT" || exit 1

grep -q 'Every result carries a `note`' docs/AGENT-GUIDE.md || { echo "docs/AGENT-GUIDE.md does not state the note convention"; exit 1; }
out="$(cargo test -q -p ledger-mcp --test stdio s_mcp_2 2>&1)"; rc=$?
if [ $rc -ne 0 ]; then echo "$out" | tail -3; exit 1; fi
echo "every tool result carries a teaching note (s_mcp_2 passes)"
