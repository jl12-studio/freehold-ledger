# Freehold Ledger: guide for the agent

You are connected to Freehold Ledger, a local bookkeeping file for one person's small business. You keep the books by calling its tools over MCP (stdio). The ledger is one SQLite file on the person's computer. Nothing goes over the network.

## Tools

| Tool | Use it when |
| --- | --- |
| `ledger_info` | First, to confirm the books are reachable. Shows the file, schema version and entry count. |
| `list_categories` | You are unsure which category key fits. Returns keys, labels and Schedule C lines. |
| `record_expense` | The person bought or paid for something ("I bought...", "I paid..."). Set `is_asset` for equipment or property kept over a year. |
| `record_income` | Money came in ("a client paid me..."). Needs a `source` (who paid). |
| `record_transactions` | Several rows at once from a bank export, statement or spreadsheet: up to 50 per call, with a `label` such as the file name. Each entry needs `kind`, `amount`, `description` and `date`. All or nothing; entries already in the books are skipped and reported. |
| `list_transactions` | Answering "what did I spend on...", finding an entry id before a correction, or reviewing before tax time. Filter by dates, kind, category, source, asset flag, or `batch_id`. Voided entries are hidden unless `include_voided` is true. |
| `reclassify_transaction` | The category was wrong ("that should be filed under..."). Needs an `id`, a `category` and a `reason`. |
| `void_transaction` | An entry should not count (a duplicate or a mistake). Needs an `id` and a `reason`. |
| `void_batch` | A whole import was wrong (the wrong file, or the same file twice). Voids every live entry of one batch, replacements of corrected entries included, with one `reason`; already-voided entries are skipped and counted. Needs the `batch_id` from `record_transactions`; `list_transactions` with `batch_id` shows what it holds. |
| `correct_transaction` | The amount, date, description, category, asset flag or source was wrong. The original is voided and a linked replacement is recorded in one step. Give at least one field to change, plus a `reason`. |
| `summarize` | Totals for a period: spending by tax category, income by source, profit. Use a named `period` or `from` plus `to`. |
| `export_tax_year` | The person wants their books for a preparer ("export my books", "get my taxes ready"). Writes a folder of CSVs, a summary and a plain-language README. |
| `create_payment_method` | The person names a card, bank account or cash they use ("add my Visa ending 4242"). Keep only a nickname and the last 4 digits. Set `make_default` when they say it is the one they usually use. |
| `list_payment_methods` | Finding a nickname or id, or answering "which cards do I have on file?". Shows the default and how many entries use each. |
| `rename_payment_method` | A card or account gets a new name. Entries on it follow the new name. A change of letter case alone (Biz Visa to BIZ VISA) is a rename too. |
| `retire_payment_method` | A card is closed or an account is no longer used. Nothing is deleted: old entries keep it, and nothing new can go on it. |
| `set_default_payment_method` | The person says which card new expenses go on by default, or that there should be no default (`clear`). |
| `assign_payment_method` | Saying what existing entries were paid with ("that was on my Visa", "everything in March was on the Amex"). Find the ids with `list_transactions` (by dates, or `payment_method` set to `"none"`), then assign up to 200 at once. |

## Conventions

- Amounts are US dollars, for example `300` or `"$1,200.50"`. Whole cents only, more than zero.
- Dates are `YYYY-MM-DD`. Omit the date to mean today.
- Import files in chunks of up to 50 rows with `record_transactions` and a label. Skipped duplicates are reported; send an entry again with `allow_duplicate` only when it really is a second charge.
- When unsure of a category, call `list_categories`. If the person wants to decide later, use `uncategorized`.
- Set `is_asset` for equipment or property the business keeps for more than a year (a laptop, a camera, a tool).
- Nothing is ever deleted. Mistakes are voided, reclassified or corrected, and every correction needs a short reason that stays in the history.
- Every result carries a `note`. Relay it to the person in your own words. Teaching is part of the job: the notes explain what was recorded and what to watch for at tax time.
- Payment methods are kept by nickname and the last 4 digits only. Never ask for, repeat, or send a full card or account number, an expiry date or a security code. Any tool input holding a full card number, or any run of 13 or more digits, is refused and nothing is stored; tell the person the ledger only keeps the last 4.
- `record_expense`, `record_income`, `correct_transaction` and `record_transactions` take an optional `payment_method` (nickname or id). An expense with none named goes on the default payment method, if one is set; income never does. A correction keeps the original's payment method unless you give a new one. Each note says which payment method was stored.
- `list_transactions` and `summarize` filter by `payment_method`; `"none"` means entries without one.

## Connection

Freehold Ledger ships a stdio server called `ledger-mcp`. Inside the installed app it lives at `Freehold Ledger.app/Contents/MacOS/ledger-mcp`. The app's "Connect your agent" panel shows the exact path on this computer and copies each snippet below.

Claude Desktop (Settings, Developer, Edit Config; paste into `claude_desktop_config.json`, then restart Claude):

```json
{
  "mcpServers": {
    "freehold-ledger": {
      "command": "<path to ledger-mcp>"
    }
  }
}
```

Claude Code (run in Terminal):

```bash
claude mcp add freehold-ledger -- "<path to ledger-mcp>"
```

Other agents: any MCP client that can launch a stdio server. Command `<path to ledger-mcp>`, no arguments.

To use a ledger file other than the default, set `FREEHOLD_LEDGER_DB` to its full path. In Claude Desktop add `"env": {"FREEHOLD_LEDGER_DB": "<path>"}` beside `command`. In Claude Code add `-e FREEHOLD_LEDGER_DB="<path>"` before `--`.

## Troubleshooting

- **"Not a Freehold ledger"**: the file at the path is not a Freehold Ledger database. Nothing was changed. Check the path, or point `FREEHOLD_LEDGER_DB` at the right file.
- **"Created by a newer Freehold Ledger"**: the file was made by a newer version than this app supports. Ask the person to update the app. Nothing was changed.
- **Wrong or empty books**: `ledger_info` shows which file is open. If it is not the file the person expects, `FREEHOLD_LEDGER_DB` is set (or unset) somewhere it should not be.
- **Server will not start**: confirm the path in your config exists and is the one the app's panel shows. After moving the app, update the path.
