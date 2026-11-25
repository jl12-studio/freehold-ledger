#!/usr/bin/env bash
# Point 8: Self-carrying upgrades
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT" || exit 1

rc=0
out="$(cargo test -q -p ledger-core --test upgrades 2>&1)" || { echo "$out" | tail -3; rc=1; }
grep -q '^## Upgrading the app' docs/AGENT-GUIDE.md || { echo "docs/AGENT-GUIDE.md has no Upgrading the app section"; rc=1; }
[ $rc -eq 0 ] && echo "every earlier format upgrades, newer files are refused, the agent guide has the upgrade steps"
exit $rc
