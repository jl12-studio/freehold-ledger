# Freehold Ledger: guide for you

Freehold Ledger keeps the books for your small business. You talk to your agent; it writes entries into a file that belongs to you.

## What you own, and where it lives

Your ledger is one file on your computer. By default it is here:

`~/Library/Application Support/org.freeholders.ledger/ledger.sqlite3`

The app shows the exact location at the top of its window, with a Show in Finder button. Nothing is sent anywhere. Any SQLite tool can open the file, with or without this app.

## How to back it up

Quit the app and your agent first. Then copy `ledger.sqlite3`, and any `ledger.sqlite3-wal` file next to it, to a safe place.

If you are comfortable in Terminal, this makes a single clean copy:

```
sqlite3 ledger.sqlite3 ".backup backup.sqlite3"
```

## Connect your agent

Open the app and press Connect your agent. Press Copy, then paste the message into a chat with your AI agent. The agent adds Freehold Ledger to its own settings and checks that it can see your ledger. If your agent asks you to restart or reload, do that, then say: "What's in my ledger?"

The message works with any AI app on your computer that supports MCP. Chats in a browser or on your phone can't connect to a local app yet.

If your agent cannot change its own settings, you can do it by hand. Add a local (stdio) MCP server in your AI app's settings with these facts (use the path the panel shows):

- name: freehold-ledger
- command: /Applications/Freehold Ledger.app/Contents/MacOS/ledger-mcp
- arguments: none
- environment: only if you use a different ledger file, set FREEHOLD_LEDGER_DB to its full path

Two common formats, as examples. A command line, like Claude Code's, run in Terminal:

```
claude mcp add freehold-ledger -- "/Applications/Freehold Ledger.app/Contents/MacOS/ledger-mcp"
```

A JSON settings file with an "mcpServers" section, used by many apps (merge this in and restart the app):

```json
{
  "mcpServers": {
    "freehold-ledger": {
      "command": "/Applications/Freehold Ledger.app/Contents/MacOS/ledger-mcp"
    }
  }
}
```

## Things to say to your agent

- "I just bought a $1,200 laptop for work today." (It will treat this as equipment you keep over a year.)
- "A client paid me $850 for a logo, last Tuesday."
- "I paid $39 for my design software subscription on the first."
- "What did I spend on software this quarter?"
- "That coffee order was filed under supplies, put it under meals."

## How mistakes get fixed

Just tell your agent what was wrong. Nothing is ever deleted. A wrong category is changed and the old one is kept in the history. A duplicate is voided with a reason. A wrong amount or date is corrected: the original is voided and a linked replacement is recorded. Your agent will ask for a reason, and the app shows the trail on each entry.

## Cards and accounts

You can tell your agent which card, bank account or cash each entry was paid with. The ledger keeps only a nickname, such as "Business Visa", and the last 4 digits. It never keeps a full card or account number, an expiry date or a security code: if one is typed into anything, it is refused and nothing is saved.

Things to say:

- "Add my business Visa ending 4242 and use it by default."
- "That $300 laptop was on the Visa."
- "Everything I spent in March was on the Amex."
- "My old Amex is closed."

The app shows each entry's payment method in its own column, and you can filter by it. Entries made before you added a card show "Not set" until you assign one.

## What the year-end export gives your preparer

Press Export in the app (or ask your agent for your books for the year). It creates a folder such as "Freehold Ledger 2026 export" (with the year you chose) in Documents/Freehold Ledger/exports. Inside:

- `README.txt` explaining every file in plain words
- `transactions.csv`, every entry for the year, voided ones marked, with the payment method and its last 4 digits so your preparer can match entries to statements
- `expenses-by-category.csv`, spending by Schedule C line
- `income-by-source.csv`, who paid you and how much
- `assets.csv`, equipment you kept over a year (no depreciation is calculated)
- `corrections.csv`, every void and category change with its reason
- `summary.json`, the year's totals for software

Hand your preparer the whole folder. The CSVs open in Numbers, Excel or Google Sheets.
