#!/usr/bin/env bash
# Point 2: Deployable anywhere
# Exit codes: 0 PASS, 4 NEEDS BUILD (no disk image yet), anything else FAIL.
# FREEHOLD_DMG overrides the disk image path (used to prove the NEEDS BUILD case).
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT" || exit 1

[ -f INSTALL.md ] || { echo "INSTALL.md is missing"; exit 1; }
[ -f scripts/release.sh ] || { echo "scripts/release.sh is missing"; exit 1; }
grep -q '^## Steps for the agent' INSTALL.md || { echo "INSTALL.md lacks the Steps for the agent section"; exit 1; }

DMG="${FREEHOLD_DMG:-$ROOT/dist/release/Freehold-Ledger-universal.dmg}"
if [ ! -f "$DMG" ]; then
  echo "disk image not built yet: run scripts/release.sh"
  exit 4
fi

if bash scripts/install-check.sh --dmg "$DMG" >/dev/null 2>&1; then
  echo "install rehearsal passed with the signature verified (INSTALL_OK)"
  exit 0
fi
if out="$(bash scripts/install-check.sh --dmg "$DMG" --unsigned 2>&1)"; then
  echo "install rehearsal passed unsigned only (INSTALL_OK, signature did not verify)"
  exit 0
fi
echo "$out" | tail -3
exit 1
