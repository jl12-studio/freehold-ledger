#!/usr/bin/env bash
# Point 6: Agent-operable administration
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT" || exit 1

rc=0
out="$(cargo test -q -p ledger-core --test admin 2>&1)" || { echo "$out" | tail -3; rc=1; }
out="$(cargo test -q -p ledger-mcp --test stdio admin_ 2>&1)" || { echo "$out" | tail -3; rc=1; }
[ $rc -eq 0 ] && echo "backup, verify and restore work from core and over MCP"
exit $rc
