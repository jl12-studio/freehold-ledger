#!/usr/bin/env bash
# Freehold Standard Point 5: A declared conflict story.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT" || exit 1

grep -q '^## .*Conflict story' DATA-MODEL.md || { echo "DATA-MODEL.md has no Conflict story section"; exit 1; }
echo "DATA-MODEL.md declares its conflict story"
