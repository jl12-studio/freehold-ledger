#!/usr/bin/env bash
# Runs the Freehold Standard self-check: one line per point, from scripts/freehold-checks/NN-*.sh.
# Usage: bash scripts/freehold-check.sh [--json]
# Check exit codes: 0 PASS, 3 LATER GRADE, 4 NEEDS BUILD, anything else FAIL.
# Exit 0 only when no point is FAIL, NOT YET or NEEDS BUILD.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CHECKS="$ROOT/scripts/freehold-checks"
JSON=0
[ "${1:-}" = "--json" ] && JSON=1

TITLES=(
  ""
  "Free, and owned by the person"
  "Deployable anywhere"
  "Speaks MCP natively"
  "Hub-and-spoke sync, self-managed"
  "A declared conflict story"
  "Agent-operable administration"
  "Secure by default"
  "Self-carrying upgrades"
  "Usable by conversation"
  "Teaches while it works"
)

bad=0
rows=()
for n in 1 2 3 4 5 6 7 8 9 10; do
  num="$(printf '%02d' "$n")"
  title="${TITLES[$n]}"
  file=""
  for f in "$CHECKS"/"$num"-*.sh; do
    [ -f "$f" ] && file="$f" && break
  done
  if [ -z "$file" ]; then
    result="NOT YET"
    detail="no check file for this point yet"
    bad=1
  else
    out="$(bash "$file" 2>&1)"; rc=$?
    detail="$(printf '%s\n' "$out" | grep -v '^[[:space:]]*$' | tail -n 1)"
    case $rc in
      0) result="PASS" ;;
      3) result="LATER GRADE" ;;
      4) result="NEEDS BUILD"; bad=1 ;;
      *) result="FAIL"; bad=1 ;;
    esac
  fi
  rows+=("$n"$'\t'"$title"$'\t'"$result"$'\t'"$detail")
done

if [ "$JSON" = "1" ]; then
  printf '%s\n' "${rows[@]}" | jq -R -s '
    split("\n") | map(select(length > 0) | split("\t")
      | {point: (.[0] | tonumber), title: .[1], result: .[2], detail: (.[3] // "")})'
else
  for row in "${rows[@]}"; do
    IFS=$'\t' read -r n title result detail <<<"$row"
    printf '%2s  %-34s %-12s %s\n' "$n" "$title" "$result" "$detail"
  done
fi

exit "$bad"
