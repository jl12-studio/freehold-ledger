#!/usr/bin/env bash
# Proves S-SCN-1 (D14, D16): the founding scene, by conversation. Drives five turns of one
# real, headless Claude Code session against the ledger-mcp that ships inside the built app,
# on a scratch ledger, then checks the ledger file with sqlite3 and writes
# docs/demo/scene-transcript.md. Needs the bundle from `pnpm --dir app tauri build --bundles app`
# and a logged-in `claude`. Never touches the real or acceptance ledger.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP="$ROOT/target/release/bundle/macos/Freehold Ledger.app"
MCP_BIN="$APP/Contents/MacOS/ledger-mcp"
TRANSCRIPT="$ROOT/docs/demo/scene-transcript.md"
TURN_TIMEOUT="${SCENE_TURN_TIMEOUT:-240}"

if [[ ! -x "$MCP_BIN" ]]; then
  echo "MISSING_BUNDLE: build it first with: pnpm --dir app tauri build --bundles app" >&2
  exit 2
fi
for tool in claude sqlite3 jq perl; do
  command -v "$tool" >/dev/null 2>&1 || { echo "MISSING_TOOL: $tool" >&2; exit 2; }
done

D="$(mktemp -d)"
LEDGER="$D/ledger.sqlite3"
KEEP=1
cleanup() {
  if [[ "$KEEP" -eq 0 ]]; then rm -rf "$D"; else echo "Scratch files kept for inspection: $D"; fi
}
trap cleanup EXIT

# The ledger lives only at $LEDGER: never inherit a developer's FREEHOLD_LEDGER_DB.
unset FREEHOLD_LEDGER_DB

jq -n --arg cmd "$MCP_BIN" --arg db "$LEDGER" \
  '{mcpServers: {"freehold-ledger": {command: $cmd, env: {FREEHOLD_LEDGER_DB: $db}}}}' >"$D/mcp.json"

TURNS=(
  'I bought a $300 device, add it to the books so it'"'"'s ready when I file taxes.'
  'It'"'"'s a laptop I use for client work, and I'"'"'ll keep it a few years.'
  'I got paid $1,200 from Upwork today for a website build.'
  'What did I spend this month, by category?'
  'Oops, that Upwork payment was actually $1,250.'
)

TODAY="$(date +%F)"
CLAUDE_VERSION="$(claude --version 2>/dev/null | head -1)"
SESSION_ID=""

# Run from the scratch dir so no project files or CLAUDE.md of this repo shape the session.
cd "$D"
for i in 1 2 3 4 5; do
  prompt="${TURNS[$((i - 1))]}"
  out="$D/turn$i.jsonl"
  args=(-p "$prompt" --mcp-config "$D/mcp.json" --strict-mcp-config
        --allowedTools "mcp__freehold-ledger" --output-format stream-json --verbose)
  if [[ -n "$SESSION_ID" ]]; then args+=(--resume "$SESSION_ID"); fi
  echo "== Turn $i: $prompt"
  rc=0
  perl -e 'alarm shift; exec @ARGV' "$TURN_TIMEOUT" claude "${args[@]}" >"$out" 2>"$D/turn$i.err" </dev/null || rc=$?
  if [[ "$rc" -ne 0 ]]; then
    echo "claude exited $rc on turn $i (see $D/turn$i.err)" >&2
    head -c 2000 "$D/turn$i.err" >&2 || true
  fi
  SESSION_ID="$(jq -rs '[.[] | select(.session_id != null) | .session_id] | last // empty' "$out" 2>/dev/null || true)"
  if [[ -z "$SESSION_ID" ]]; then echo "no session_id in turn $i output" >&2; break; fi
  echo "   tools: $(jq -rs '[.[] | select(.type=="assistant") | .message.content[]? | select(.type=="tool_use") | .name | sub("^mcp__freehold-ledger__";"")] | join(", ")' "$out")"
done
cd "$ROOT"

# ---------- helpers over the saved streams ----------
final_text() { jq -rs '[.[] | select(.type=="result")] | last | .result // ""' "$D/turn$1.jsonl" 2>/dev/null || true; }
tool_names() { jq -rs '[.[] | select(.type=="assistant") | .message.content[]? | select(.type=="tool_use") | .name]' "$D/turn$1.jsonl" 2>/dev/null || echo '[]'; }
model_name() { jq -rs '[.[] | select(.type=="system" and .subtype=="init") | .model] | first // "unknown"' "$D/turn1.jsonl" 2>/dev/null || echo unknown; }
q() { sqlite3 -readonly "$LEDGER" "$1" 2>/dev/null || echo "ERR"; }

# ---------- assertions ----------
FAILS=0
RESULTS=""
check() { # name, ok(0/1), detail
  local verdict="PASS"
  if [[ "$2" -ne 0 ]]; then verdict="FAIL"; FAILS=$((FAILS + 1)); fi
  local line="$verdict  $1 ($3)"
  echo "$line"
  RESULTS+="- $line"$'\n'
}

echo
echo "== Checks against $LEDGER (today = $TODAY)"
NOT_VOIDED="NOT EXISTS (SELECT 1 FROM voids v WHERE v.transaction_id = t.id)"

n="$(q "SELECT count(*) FROM transactions t WHERE kind='expense' AND amount_cents=30000 AND is_asset=1 AND occurred_on='$TODAY' AND $NOT_VOIDED")"
total_exp="$(q "SELECT count(*) FROM transactions t WHERE kind='expense' AND $NOT_VOIDED")"
check "exactly one live \$300.00 asset expense dated today, and no other live expense" \
  "$([[ "$n" == 1 && "$total_exp" == 1 ]] && echo 0 || echo 1)" "matching=$n, live expenses=$total_exp"

n="$(q "SELECT count(*) FROM transactions t JOIN income_sources s ON s.id=t.income_source_id
        WHERE t.kind='income' AND t.amount_cents=125000 AND s.name='Upwork' AND $NOT_VOIDED
          AND t.replaces_id IN (SELECT o.id FROM transactions o JOIN voids v ON v.transaction_id=o.id
                                WHERE o.amount_cents=120000 AND o.kind='income')")"
total_inc="$(q "SELECT count(*) FROM transactions t WHERE kind='income' AND $NOT_VOIDED")"
check "exactly one live \$1,250.00 Upwork income that replaces a voided \$1,200.00 row, and no other live income" \
  "$([[ "$n" == 1 && "$total_inc" == 1 ]] && echo 0 || echo 1)" "matching=$n, live income=$total_inc"

cnt="$(q "SELECT count(*) FROM transactions")"
maxid="$(q "SELECT coalesce(max(id),0) FROM transactions")"
voids="$(q "SELECT count(*) FROM voids")"
check "nothing was deleted: at least 3 rows, ids contiguous, the original is voided not removed" \
  "$([[ "$cnt" =~ ^[0-9]+$ && "$cnt" -ge 3 && "$cnt" == "$maxid" && "$voids" =~ ^[0-9]+$ && "$voids" -ge 1 ]] && echo 0 || echo 1)" \
  "rows=$cnt, max id=$maxid, voids=$voids"

t4="$(final_text 4)"
used_summarize="$(tool_names 4 | jq -r 'any(.[]; endswith("__summarize"))')"
has300=1; [[ "$t4" == *300* ]] && has300=0
check "turn 4 answer mentions 300 and the turn called summarize" \
  "$([[ "$has300" == 0 && "$used_summarize" == "true" ]] && echo 0 || echo 1)" \
  "mentions 300=$([[ $has300 == 0 ]] && echo yes || echo no), summarize called=$used_summarize"

# ---------- transcript ----------
mkdir -p "$ROOT/docs/demo"
{
  echo "# Founding scene, by conversation"
  echo
  echo "Generated by \`scripts/scene-check.sh\` (S-SCN-1). A real headless Claude Code session drove five turns against the \`ledger-mcp\` bundled inside \`Freehold Ledger.app\`, on a scratch ledger."
  echo
  echo "- Date: $TODAY"
  echo "- Claude Code: $CLAUDE_VERSION"
  echo "- Model reported by the session: $(model_name)"
  echo "- Result: $([[ "$FAILS" -eq 0 ]] && echo 'every check passed' || echo "$FAILS check(s) failed")"
  echo
  echo "## Checks"
  echo
  printf '%s' "$RESULTS"
  for i in 1 2 3 4 5; do
    echo
    echo "## Turn $i"
    echo
    echo "**Prompt**"
    echo
    echo "> ${TURNS[$((i - 1))]}"
    echo
    echo "**Tool calls**"
    echo
    if [[ -f "$D/turn$i.jsonl" ]]; then
      calls="$(jq -rs '[.[] | select(.type=="assistant") | .message.content[]? | select(.type=="tool_use")]
                        | map("- `" + (.name | sub("^mcp__freehold-ledger__";"")) + "` " + (.input | tojson)) | join("\n")' "$D/turn$i.jsonl" 2>/dev/null || true)"
      errs="$(jq -rs '[.[] | select(.type=="user") | .message.content[]? | select(.type=="tool_result" and .is_error==true)] | length' "$D/turn$i.jsonl" 2>/dev/null || echo 0)"
      if [[ -n "$calls" ]]; then echo "$calls"; else echo "(none)"; fi
      echo
      echo "Tool calls that returned an error: $errs"
      echo
      echo "**Assistant's final text**"
      echo
      final_text "$i" | sed 's/^/> /'
    else
      echo "(turn did not run)"
    fi
  done
} >"$TRANSCRIPT"

echo
if [[ "$FAILS" -eq 0 ]]; then
  KEEP=0
  echo "SCENE_CHECKS_PASSED (transcript: docs/demo/scene-transcript.md)"
  exit 0
fi
echo "SCENE_CHECKS_FAILED: $FAILS check(s) failed (transcript: docs/demo/scene-transcript.md)" >&2
exit 1
