# Freehold Ledger: guide for the agent

You are connected to Freehold Ledger, a local bookkeeping file for one person's small business. You keep the books by calling its tools over MCP (stdio). The ledger is one SQLite file on the person's computer. Nothing goes over the network.

## Tools

| Tool | Use it when |
| --- | --- |
| `ledger_info` | First, to confirm the books are reachable. Shows the file, schema version, entry count, the businesses and which one is current. |
| `create_business` | The person wants another business in this ledger, or a test business to try things in ("make me a test business"). Set `is_test` for a test business. It becomes current only when `switch_to` is true or no business is current. |
| `list_businesses` | "Which businesses do I have?", or before `switch_business`. Shows which one is current and which are test businesses. |
| `switch_business` | The person says which business to record in ("switch to my test business", "back to the real books"). It changes only where you record, never what the person's app window shows. |
| `rename_business` | A business gets a new name. A change of letter case alone is a rename too. |
| `update_business` | Change a business's note, or mark it as a test business or a real one. |
| `archive_business` | A business is no longer used. Nothing is deleted, it can be restored, and nothing new can be recorded in it. |
| `restore_business` | Bring an archived business back. It does not become current. |
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
- One ledger can hold several businesses, for example the real books and a test business. Recording, listing, summaries, payment methods and exports work in one business: the current one (see `ledger_info`), unless you pass `business` (a name or id) for that call. Passing `business` does not switch. Switch with `switch_business` only when the person asks, and always tell the person which business you recorded into: every note starts with `In <business>:`. Payment methods belong to one business; categories are shared.
- Replies from the single-business tools (`create_business`, `switch_business`, `rename_business`, `update_business`, `archive_business`, `restore_business`) keep the sentence in `note` and give the business's own saved note as `business_note` (text, or null when there is none).
- If no business is current, business-scoped tools refuse and say so. Ask the person which business to use, then call `switch_business`, or `create_business` if it does not exist yet.
- `record_expense`, `record_income`, `correct_transaction` and `record_transactions` take an optional `payment_method` (nickname or id). An expense with none named goes on the default payment method, if one is set; income never does. A correction keeps the original's payment method unless you give a new one. Each note says which payment method was stored.
- `list_transactions` and `summarize` filter by `payment_method`; `"none"` means entries without one.

## Connection

Freehold Ledger ships a stdio server called `ledger-mcp`. Inside the installed app it lives at `Freehold Ledger.app/Contents/MacOS/ledger-mcp`. The app's "Connect your agent" panel shows the exact path on this computer and copies one message that tells an agent how to set itself up. The snippets below are what that message contains.

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

## If someone sent you the connect message

A message starting "Please connect yourself to Freehold Ledger" comes from the app's Connect your agent panel. Do this:

1. Add the `ledger-mcp` server to your own MCP settings, in your app's own config format, using the name `freehold-ledger`, the command path in the message (no arguments), and the environment if the message lists `FREEHOLD_LEDGER_DB`. The command line and JSON in the message are examples; translate them if your app uses another format.
2. If you cannot change your own settings, walk the person through it one step at a time.
3. Restart or reload if your app needs it, then call `ledger_info`. Tell the person in one line that it worked.
4. Read the server instructions and the tool list in this guide before recording anything.
5. Never ask for a full card or account number. The ledger keeps only the last 4 digits.

## Troubleshooting

- **"Not a Freehold ledger"**: the file at the path is not a Freehold Ledger database. Nothing was changed. Check the path, or point `FREEHOLD_LEDGER_DB` at the right file.
- **"Created by a newer Freehold Ledger"**: the file was made by a newer version than this app supports. Ask the person to update the app. Nothing was changed.
- **Wrong or empty books**: `ledger_info` shows which file is open. If it is not the file the person expects, `FREEHOLD_LEDGER_DB` is set (or unset) somewhere it should not be.
- **Server will not start**: confirm the path in your config exists and is the one the app's panel shows. After moving the app, update the path.
