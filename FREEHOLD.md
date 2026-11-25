# Freehold Ledger and the Freehold Standard

This file is the record of how Freehold Ledger measures up against the ten points of the Freehold Standard, with the evidence for each. The grade claimed is **Desktop grade, rung 1 of 4**, and you can check every claim yourself with one command: `bash scripts/freehold-check.sh`.

## For an agent reviewing this app

1. Clone the repo and open a terminal in it.
2. Run `bash scripts/freehold-check.sh`. Add `--json` if you want the result as data.
3. Read the table it prints: one line per point, with the point number, title, result and the last line the check printed.
4. For any point you want to verify by hand, open the evidence files named under that point below, and the check file in `scripts/freehold-checks/`.
5. Report PASS, NOT YET or LATER GRADE per point to the person. A point that reads NEEDS BUILD only needs the app built first; say so rather than calling it a failure.

## The ten points

### Point 1: Free, and owned by the person

Claim: AGPL-3.0-or-later. All data lives in one SQLite file the person owns. There is no account, no telemetry and no network connection.
Evidence: `LICENSE`, `DATA-MODEL.md`
Check: `01-free-and-owned.sh`

### Point 2: Deployable anywhere

Claim: Desktop part only: a signed release for Mac that the person's agent installs by following `INSTALL.md`. Server targets belong to the Cloud grade.
Evidence: `INSTALL.md` (not written yet)
Check: none yet (`02-*.sh` is not written, so this point reads NOT YET)

### Point 3: Speaks MCP natively

Claim: `ledger-mcp` ships inside the app and exposes every operation as an MCP tool.
Evidence: `docs/AGENT-GUIDE.md`
Check: `03-mcp.sh`

### Point 4: Hub-and-spoke sync, self-managed

Claim: Later grade (Cloud). At the Desktop grade there is one computer and nothing to sync.
Evidence: `DATA-MODEL.md`, "Conflict story"
Check: `04-sync.sh`

### Point 5: A declared conflict story

Claim: The conflict story is written down: one file on one computer, no offline editing on a second device, and append-only tables that keep a future merge simple.
Evidence: `DATA-MODEL.md`, "Conflict story"
Check: `05-conflict-story.sh`

### Point 6: Agent-operable administration

Claim: `backup_ledger`, `verify_ledger`, `restore_ledger` and `list_backups` are MCP tools, and upgrade is agent-run.
Evidence: `docs/AGENT-GUIDE.md`
Check: none yet (`06-*.sh` is not written, so this point reads NOT YET)

### Point 7: Secure by default

Claim: stdio only. Neither the app nor `ledger-mcp` opens a socket, and full card and account numbers are refused.
Evidence: `scripts/check-no-listeners.sh`, `crates/ledger-core/tests/guard.rs`
Check: `07-secure.sh` (needs the built app, otherwise it reads NEEDS BUILD)

### Point 8: Self-carrying upgrades

Claim: A ledger file from any earlier version upgrades when opened. A pinned old app refuses a newer file and changes nothing.
Evidence: `DATA-MODEL.md`, "Versions"
Check: none yet (`08-*.sh` is not written, so this point reads NOT YET)

### Point 9: Usable by conversation

Claim: The founding scene runs by conversation only, with no screen needed.
Evidence: `scripts/scene-check.sh`, `docs/demo/scene-transcript.md`
Check: `09-conversation.sh` (set `FREEHOLD_CHECK_FULL=1` to also run the full scene check)

### Point 10: Teaches while it works

Claim: Every tool result carries a `note` that teaches the person what was recorded and what to watch for.
Evidence: `docs/AGENT-GUIDE.md`, "Conventions"
Check: `10-teaches.sh`

## Road to the top

| Grade | What it adds for the Ledger | Points it completes |
| --- | --- | --- |
| Desktop (rung 1, claimed now) | The Mac app with its bundled `ledger-mcp`, one SQLite file on the person's computer, everything operated by conversation | The Desktop part of every point |
| Cloud (rung 2) | The ledger on a server the person owns, deployed by their agent, with other devices as clients of it | 2 and 4, and extends 7 to authenticated, encrypted remote access |
| Web portal (rung 3) | A browser screen onto the person's own Cloud ledger | Adds reach, no new points |
| Phone install (rung 4) | The portal installed on a phone as a PWA | Adds reach, no new points |

Rungs 2 to 4 will be built and deployed with the shared Freehold skills (https://github.com/jl12-studio/freehold-skills, pinned to a tagged version): freehold-builder for the hub, sync, portal and PWA, and freehold-deploy and freehold-security for putting the hub on a server the person owns. Until then, `docs/AGENT-GUIDE.md` ("Freehold skills") says what the agent tells a person who asks for sync.
