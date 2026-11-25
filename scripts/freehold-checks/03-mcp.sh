#!/usr/bin/env bash
# Freehold Standard Point 3: Speaks MCP natively.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT" || exit 1

out="$(cargo test -q -p ledger-mcp --test stdio s_mcp_1 2>&1)"; rc=$?
if [ $rc -ne 0 ]; then echo "$out" | tail -3; exit 1; fi
echo "ledger-mcp lists every tool (s_mcp_1 passes)"
