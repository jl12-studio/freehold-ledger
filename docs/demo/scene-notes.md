# Scene notes (JIP-474, S-SCN-1)

How the founding scene was proven, what was tuned, and what is still rough. The transcript of the final run is `scene-transcript.md`; `scripts/scene-check.sh` regenerates it.

## Iterations

1. Baseline run (descriptions as shipped in M3): all four checks passed, but the run was rough. Turn 1 recorded the device as `uncategorized` and not an asset, then asked the person three questions it did not need (what device, how long they will keep it, was it today). Turn 2 had to fix the category and asset flag through `correct_transaction`.
2. Fix 1, in `crates/ledger-mcp/src/tools.rs` text only: the `record_expense` description and the server instructions now say to record right away with sensible defaults (today's date, the most likely category), tell the person the choices afterward, treat a kept device, computer, camera or tool as equipment (`is_asset=true`, `depreciation_section_179`, with `supplies` named as the alternative a preparer may prefer), use `uncategorized` only when the person says to decide later, and fix an existing entry with `correct_transaction` when detail arrives later. No core rule changed.
3. Three consecutive runs on the fixed bundle passed all four checks. Turn 1 now records the $300 asset expense in one call, with no questions; turn 2 refines the description in place via `correct_transaction`.

Fix iterations used: 1 of 3.

## Remaining rough edges

- Every turn shows a `ToolSearch` call before the first ledger tool. This is Claude Code loading the schema of a deferred MCP tool, not a ledger-mcp problem, and nothing in the server's descriptions can remove it. Not filed.
- The agent's own wording after turn 1 may still offer the `supplies` alternative or ask for the device type. That is a courtesy question after the entry exists, not a blocker, and the entry is already correct.
- The check passes on the live ledger state, not on exact wording, so the agent may pick `supplies` instead of `depreciation_section_179` on another run and still pass. This is intended: the data model allows either.

No unfixable rough edge needed a follow-up Linear issue.
