# Data Model

This is the ledger in plain language: what is stored, what the rules are, and what happens to each fact when you say something like *"I bought a $300 device, add it to the books so it's ready when I file taxes."* If this document and the app ever disagree, that is a bug in the app.

## 1. What you own

Your books live in one SQLite file on your computer.

- Default location (macOS): `~/Library/Application Support/org.freeholders.ledger/ledger.sqlite3`
- Setting the `FREEHOLD_LEDGER_DB` environment variable points the app, the agent connection, and the tests at a different file instead.
- You can copy it, back it up, or open it with any SQLite tool. It is a normal file that belongs to you.
- Nothing leaves the computer. There is no account, no server, and no sync.

The app and your agent's connection are two programs reading and writing the same file. Both are safe to use at the same time on one machine.

## 2. A transaction

A transaction is one thing that happened to the money of your business: one expense or one piece of income.

| Field | In plain words |
|---|---|
| `id` | A number the ledger gives the entry. Never reused. |
| `kind` | `expense` (money out) or `income` (money in). The kind carries the sign, so the amount is always positive. |
| `occurred_on` | The date it happened, as `YYYY-MM-DD`, in your local time. If you do not say, it is today. |
| `amount_cents` | The amount in whole cents (a $300 device is `30000`). Always greater than zero. US dollars only in v1. |
| `description` | A short note saying what it was. Required. |
| `category` | Which tax bucket it belongs to: a category key (section 3), which also fixes its Schedule C line. |
| `is_asset` | Yes or no: is this equipment or property you keep longer than a year? Defaults to no. Only meaningful for expenses. |
| income source | Income only: who paid you (section 4). Expenses have none. |
| `recorded_at` | When the entry was written into the ledger, in UTC. Set by the ledger, not by you. |
| `replaces_id` | Only on a correction: the id of the entry this one replaces. Empty otherwise. |
| payment method | Optional: the card, bank account, cash or other method it was paid with (see Payment methods below). Empty until one is set. |
| business | Which business the entry belongs to (see Businesses). Set when it is recorded and never changes; a correction stays in its original's business. |

Assets are listed separately in the export. The app does not compute depreciation; your agent and your tax preparer supply that knowledge.

## 3. Categories

Expense categories are the lines in Part II of IRS Schedule C (Form 1040), the form a US sole proprietor files. Using the same buckets means totals land on the right lines at tax time. Each has a stable key that never changes.

| Key | Schedule C line | Label |
|---|---|---|
| `advertising` | 8 | Advertising |
| `car_truck` | 9 | Car and truck expenses |
| `commissions_fees` | 10 | Commissions and fees |
| `contract_labor` | 11 | Contract labor |
| `depreciation_section_179` | 13 | Depreciation and section 179 |
| `employee_benefits` | 14 | Employee benefit programs |
| `insurance` | 15 | Insurance (other than health) |
| `interest_mortgage` | 16a | Interest: mortgage |
| `interest_other` | 16b | Interest: other |
| `legal_professional` | 17 | Legal and professional services |
| `office_expense` | 18 | Office expense |
| `pension_profit_sharing` | 19 | Pension and profit-sharing plans |
| `rent_vehicles_equipment` | 20a | Rent or lease: vehicles, machinery, equipment |
| `rent_other` | 20b | Rent or lease: other business property |
| `repairs_maintenance` | 21 | Repairs and maintenance |
| `supplies` | 22 | Supplies |
| `taxes_licenses` | 23 | Taxes and licenses |
| `travel` | 24a | Travel |
| `meals` | 24b | Deductible meals |
| `utilities` | 25 | Utilities |
| `wages` | 26 | Wages |
| `other_expense` | 27a | Other expenses |
| `uncategorized` | none | Needs a category |

Income categories:

| Key | Schedule C line | Label |
|---|---|---|
| `gross_receipts` | 1 | Gross receipts or sales (the default for income) |
| `other_income` | 6 | Other income |

`uncategorized` is allowed when the right bucket is not yet known, but summaries and the export flag it so it gets fixed before you file.

## 4. Income sources

Income always comes from a named source, and each source is stored once. Names match without regard to capital letters: recording income from "upwork" reuses the existing "Upwork" instead of making a second one. Expenses have no source.

## 5. Fixing mistakes (append-mostly)

Nothing in the ledger is ever edited or deleted. A mistake is fixed by adding a record next to it, so the full history is always there. There are three moves.

- **Void**: cancel an entry, with a reason. Example: you logged the same coffee twice, so you void the second one with the reason "duplicate".
- **Correct**: void the original and add a replacement in one step. The replacement points back to the original through `replaces_id`. Example: see the $30.00 typo in section 10.
- **Reclassify**: change only the category. The change is recorded with a reason, and the latest change wins as the entry's effective category; the original is kept. Example: a $45 charge filed under `supplies` is reclassified to `office_expense` with the reason "it was a desk lamp for the home office".

Voided entries never count in any total, and they always stay visible in any listing that asks for them.

## Batches (imports)

When rows are recorded together from a file or statement, the ledger keeps a `batches` record (a short label such as the file name, and when it was recorded) and a `transaction_batches` link from each entry to its batch. Entries recorded one at a time have no batch. A batch is created only when at least one entry was actually recorded.

To stop an import from counting the same money twice, an entry is skipped (and reported) when a live entry already has the same kind, date and amount and the same description, ignoring capital letters. Voided entries do not count as matches, and an entry that repeats an earlier one in the same call is skipped too. An entry marked `allow_duplicate` is recorded anyway, for a real second charge. Both batch tables are append-only, like everything else.

A whole batch can be undone in one step, which voids every live entry in it (replacements of corrected entries included) with one reason and deletes nothing. A replacement made by correcting an entry joins its original's batch.

## Payment methods

A payment method is a card, a bank account, cash, or something else the business pays with or gets paid into. Each has a nickname, such as "Business Visa", and, for cards and accounts, optionally the last 4 digits of its number. The ledger never keeps a full card or account number, an expiry date or a security code. Any text that holds a full card number, or any run of 13 or more digits, is refused before it is stored, wherever it is typed. Digits in any script count, so fullwidth or Arabic-Indic digits are refused the same way.

| Table | In plain words |
|---|---|
| `payment_methods` | One row per payment method: its kind (`card`, `bank_account`, `cash` or `other`), its first nickname, its last 4 digits or none, and when it was added. |
| `payment_method_changes` | Renames and retirements. The latest rename is the nickname you see. A retired method is never deleted. |
| `default_payment_method_changes` | Each time the default is set or cleared. The latest row is the default, so there is at most one. |
| `payment_method_assignments` | Each time an entry is put on a payment method. The latest row for an entry is its payment method; earlier rows are the history. |

- Each payment method belongs to one business. Nicknames are unique among that business's payment methods in use, in any letter case. "none" is reserved.
- Each business has at most one default. A new expense with no payment method named goes on its business's default, if there is one. Income never does.
- A correction's replacement keeps the original's payment method unless a new one is given.
- Voided entries keep their payment method and cannot be moved to another.
- Entries recorded before payment methods existed have none until one is assigned.
- All four tables are append-only, like the rest of the ledger.

## Businesses

One ledger file can hold several businesses, for example the real books and a test business. Every entry, batch and payment method belongs to exactly one business; categories and the list of income source names are shared. Your agent records into its current business unless it names another one for a single entry. The app window picks which business it shows on its own and never changes where the agent records.

| Table | In plain words |
|---|---|
| `businesses` | One row per business: its first name, an optional note, whether it is a test business, and when it was added. |
| `business_changes` | Renames, note changes, test-flag changes, archives and restores. The latest of each kind wins. A business is never deleted. |
| `current_business_changes` | Each time the agent's current business is set or cleared. The latest row is the current one. |
| `transaction_businesses` | Which business each entry belongs to. |
| `batch_businesses` | Which business each import batch belongs to. |
| `payment_method_businesses` | Which business each payment method belongs to. |
| `business_default_payment_method_changes` | Each time a business's default payment method is set or cleared. The latest row per business is its default. |

- Business names are unique among businesses in use, in any letter case, and never hold a full card or account number.
- An archived business keeps everything and can still be viewed and exported; nothing new can be recorded in it until it is restored.
- When a schema 3 file is upgraded, every existing entry, batch and payment method moves into one business named "My business", which becomes current. The older `default_payment_method_changes` table is kept as history.
- All seven tables are append-only, like the rest of the ledger.

## 6. Conflict story

The ledger is one file on one computer. The app window and the agent's `ledger-mcp` share it safely through SQLite (write-ahead log, 5 second busy timeout). There is no offline editing on a second device at this grade, so there is nothing to merge. A copy of the file is an independent ledger. Every table is append-only, which is what will make merging simple when the Cloud grade adds sync.

## 7. Versions

The file is now schema 4; files from schema 1, 2 or 3 upgrade automatically when opened. A schema 1 file is upgraded automatically when it is opened, and its entries are unchanged. If an older copy of the app opens a file written by a newer one, it refuses and changes nothing, rather than risk damaging your books. Pinning an old version of the app keeps working safely. `crates/ledger-core/tests/upgrades.rs` proves this for every earlier schema.

## 8. What must never be lost

- Every transaction
- Every void and its reason
- Every classification change
- Every income source
- Every batch and which entries it holds
- Every payment method, rename and retirement
- Every change of default payment method
- Every payment method assignment
- Every business, rename, note, test flag, archive and restore
- Every change of current business
- Which business each entry, batch and payment method belongs to

## 9. What the export will contain (coming in M4)

Exporting a tax year writes one business's books to a new folder named `Freehold Ledger <business> <year> export/` (or Freehold Ledger <business> (test) <year> export/ for a test business), by default under `Documents/Freehold Ledger/exports/`. If that folder already exists, a ` (2)`, ` (3)` suffix is added; nothing is overwritten. Inside:

- `transactions.csv`: every transaction for the year, with a header row, including each entry's payment method nickname and last 4 digits.
- `expenses-by-category.csv`: expense totals per category, with the Schedule C line.
- `income-by-source.csv`: income totals per source.
- `assets.csv`: the entries marked as assets, listed separately.
- `corrections.csv`: voids, replacements, and category changes, with reasons.
- `summary.json`: the same numbers the app's summary shows.
- `README.txt`: a plain-language guide to every file and column.

## 10. The scene, on paper

### The device

> "I bought a $300 device, add it to the books so it's ready when I file taxes."

| Fact in the sentence | Where it lands |
|---|---|
| "bought" (money out) | `kind` = `expense` |
| "$300" | `amount_cents` = `30000` |
| "device" | `description` = "Device" |
| (not stated, so now) | `occurred_on` = today |
| "kept, ready for taxes" | `is_asset` = yes (kept longer than a year) |
| "so it's ready when I file taxes" | `category`, chosen by the agent: `supplies` (line 22) or `depreciation_section_179` (line 13), depending on the treatment your preparer uses |
| (set by the ledger) | `recorded_at` = the moment it was saved |

Filled-in row: expense, today, 30000 cents, "Device", category `depreciation_section_179` (or `supplies`), asset yes, no source, no `replaces_id`.

### An income

> "I got $1,200 from Upwork for a website."

| Fact | Where it lands |
|---|---|
| "got" (money in) | `kind` = `income` |
| "$1,200" | `amount_cents` = `120000` |
| "for a website" | `description` = "Website" |
| "from Upwork" | income source = "Upwork" (reused if it exists) |
| (default) | `category` = `gross_receipts` (line 1) |
| (not stated) | `occurred_on` = today; `is_asset` = no |

### A correction

You typed $30.00 for the device, but it was $300.00.

1. The original stays: expense, 3000 cents, "Device".
2. The ledger voids it with the reason "typo, should be $300.00".
3. In the same step it adds a replacement: expense, 30000 cents, "Device", with `replaces_id` set to the original's id.

Totals now count only the $300.00 entry. Listings that ask for voided entries still show the $30.00 one and its reason.

## 11. Not in v1

- Multiple currencies (USD only)
- Depreciation math
- Invoicing
- Payroll
- Bank sync
- Multiple users
