# Importing existing records: recommendation (JIP-480)

Recommendation: **the agent path is enough, plus one small helper tool** (a batch record with a duplicate check). Do not build a dedicated import tool in v1.

## The two sample shapes

Both files are made up, about 25 rows, in `docs/research/import-samples/`.

| | `bank-export.csv` | `hand-kept-sheet.csv` |
|---|---|---|
| Columns | Date, Description, Amount, Balance | Date, What, Category, Amount |
| Sign convention | Negative = money out, positive = money in | All positive; income is a category word, a refund is a negative expense |
| Descriptions | Merchant strings (`STAPLES #0412`, `UBER *TRIP`) | The person's own words (`Helpers`, `Fun`, `Education`) |
| Planted traps | Duplicate GoDaddy row, `STAPLES #0412 RETURN` refund (+24.99), two quoted descriptions with commas, a Venmo cashout (transfer), bank interest, ambiguous Amazon and meals rows | Duplicate Facebook ads row, `Return: toner cartridge` (-19.99), quoted `"Printer paper, toner"`, a personal `Valentines dinner`, a laptop (asset), a 350 `Education` ticket |

## Measured agent-path results

Method: headless `claude -p` with `--mcp-config` pointing one server (`ledger`) at `target/debug/ledger-mcp`, a fresh scratch ledger per trial, `--allowedTools "mcp__ledger__*" Read`, no code changed. Rows verified with `sqlite3` SELECTs on the scratch ledgers, not from the agent's report. The default model for the headless run was Opus 5.5.

| Trial | Rows landed | Right amount and sign | Duplicate | Refund | Tool calls | Wall time | Cost |
|---|---|---|---|---|---|---|---|
| Bank, run 1 | 19 of 25 | 19 of 19 | GoDaddy recorded twice (flagged for the user) | Skipped, not recorded | about 24 (25 turns) | 67 s | $0.41 |
| Bank, run 2 | 19 of 25 | 19 of 19 | GoDaddy recorded twice (flagged) | Skipped | 25 (16 expense, 3 income, 5 read-only/other) | 1078 s (outlier, API time was 57 s; the shell command stalled) | $0.35 |
| Sheet, run 1 | 22 of 25 | 22 of 22 | Second Facebook ads row caught and skipped | Skipped, offered a correction | 27 (19 expense, 3 income, 5 other) | 54 s | $0.40 |

What the agent did well: every landed row has the right kind, cents and date (negative debits became expenses, positive credits became income); quoted commas were parsed; income sources were reused or created (Upwork, Hartley); the laptop was marked `is_asset` and put under `depreciation_section_179`; skipped personal rows were reported with a reason. Mapped categories were reasonable: no clearly wrong Schedule C line. Judgment calls a person might dispute: software as `office_expense`, hosting as `other_expense`, tax prep software as `legal_professional`, the 50 gift card in full.

Where it fell short:
- **Refunds: 0 of 2 handled.** `amount_cents > 0` (D7) makes a negative expense unrepresentable, so the agent skipped both and proposed `correct_transaction` to shrink the original purchase. That is a sound path, but the user must say yes row by row.
- **Duplicates: 1 of 2 caught.** The sheet duplicate was dropped. The identical bank rows were both recorded because the balance column showed two real debits. Reasonable, but the choice is made per run and the ledger has no help.
- **Inconsistency between runs.** Run 1 skipped Shell gas and recorded bank interest as `other_income`; run 2 did the opposite. Same file, different ledger.
- **Skipped rows are decisions the user must still make** (6 of 25 bank rows, 3 of 25 sheet rows). Nothing in the ledger remembers them.

## Hard parts, with evidence

1. **Column mapping:** not hard. The agent read both headers and mapped them with no instruction. Cost: zero tool calls beyond one Read.
2. **Sign conventions:** handled in all 41 landed rows. The risk is the reverse convention (credit cards export charges as positive); the agent reasoned from the balance column here, which a card export may not have.
3. **Refunds:** the real gap (above). The data model has no negative amount by design.
4. **Duplicate detection:** the agent only sees duplicates inside one file in one session. Importing the same file twice, or two overlapping exports, would silently double the ledger. This is the most likely real-world damage.
5. **Classifying hundreds of rows:** cost and time scale linearly: about $0.016 per row and 2.5 to 3 s per row of agent time (one tool call per row, roughly 1 call per row plus 5 fixed). 500 rows is roughly 25 to 30 minutes and $8 in one session, with a long context; the 25-row files already needed 25 to 28 turns. Batch the file into chunks of about 50 rows.
6. **Append-only and a bulk mistake:** to undo a wrong import the agent must call `void_transaction` once per row. Estimate from the measured numbers (one call about 2.5 s and about $0.015): 200 voids is about 8 to 10 minutes and about $3. Annoying, not dangerous, since voided rows stay visible and excluded from totals. A batch void would turn this into one call.

## Reasoning

The cheapest candidate works. In three real runs, no landed row had a wrong amount or sign, and the agent surfaced every ambiguous row instead of guessing silently, which is the behavior a bookkeeping app wants. A dedicated import tool would have to reimplement what the agent already does (header mapping, merchant to category, "is this personal?"), and a rules-based version would do worse on the hand-kept sheet's own-words categories. Defer is too timid: the duplicate risk is real and cheap to cover. So keep the agent as the importer and add the two guard rails the evidence shows are missing: one call that records many rows with a duplicate warning, and one call that undoes a whole batch. Refund handling stays with `correct_transaction` (no schema change); re-opening D7 is not justified by two sample rows.

## Drafted follow-up issues

No import code was written in JIP-480. These are drafts for the orchestrator; they are not created in Linear.

### Issue A: `record_transactions` batch tool with duplicate warnings

**Purpose:** let the agent record up to 50 rows per call, and warn when a row matches an existing live transaction (same date, kind, cents, case-insensitive description), so re-imports and overlapping exports do not silently double the ledger.

**Tasks:**
1. In `ledger-core`, add a `record_batch` that takes up to 50 expense/income entries, validates all of them first, then inserts all in one SQLite transaction (all or nothing). Each entry reports its new id, or a possible duplicate (id of the match).
2. Default behavior: rows flagged as possible duplicates are NOT inserted unless the entry sets `allow_duplicate: true`; they come back in a `skipped_duplicates` list.
3. Add a batch label (free text, for example the source file name) stored per row in a new table `batches (id, label, recorded_at)` and `transaction_batches (transaction_id, batch_id)`, so the batch can be found later. `transactions` stays untouched (D12).
4. In `ledger-mcp`, expose `record_transactions`. Every success carries a one-sentence `note`; every input error is `isError: true` with a plain message (D14).
5. Update DATA-MODEL.md and README tool list.

**Acceptance criteria:**
* Recording 3 valid entries in one call lands 3 rows; one invalid entry among them lands none and names the bad entry.
* Recording the same entry twice (same call or a later call) reports the second as a possible duplicate and does not insert it; with `allow_duplicate: true` it inserts.
* Voided transactions do not count as duplicates.
* The 25-row bank sample imported in 1 to 2 batch calls produces the same 19 rows as the measured runs.
* Over 50 entries is rejected with a readable message. No TCP/UDP listener is opened (D15).

**Execution profile:**
* Tier: sonnet
* Why: Additive core function plus a thin MCP wrapper, a new table, and a clear contract; no judgment beyond the stated rules.
* Effort: medium
* Allowlist: `crates/ledger-core/`, `crates/ledger-mcp/`, `DATA-MODEL.md`, `README.md`
* Budget: 60 minutes
* Label: `tier:sonnet`

### Issue B: `void_batch` tool

**Purpose:** undo a bad import in one call instead of one `void_transaction` per row (200 rows is about 8 to 10 minutes and $3 of agent time by the JIP-480 estimate).

**Tasks:**
1. In `ledger-core`, add `void_batch(batch_id, reason)` that voids every not-yet-voided transaction in the batch in one SQLite transaction, writing normal `voids` rows.
2. Expose it as the MCP tool `void_batch` with a `note` and readable errors.
3. `list_transactions` accepts an optional `batch_id` filter.

**Acceptance criteria:**
* Voiding a 3-row batch voids 3 rows with the given reason; totals drop by exactly that sum.
* Rows already voided individually are skipped and counted in the result, not an error.
* An unknown batch id is an `isError: true` result. `transactions` rows are never updated or deleted (D12).

**Execution profile:**
* Tier: sonnet
* Why: Small addition on top of Issue A's batch tables, mirroring `void_transaction`.
* Effort: low
* Allowlist: `crates/ledger-core/`, `crates/ledger-mcp/`, `README.md`
* Budget: 30 minutes
* Blocked by: Issue A
* Label: `tier:sonnet`
