# Freehold Ledger

A free bookkeeping app you own outright. Your agent keeps the books by conversation; the records live in one file on your computer.

## Get it

Tell your agent:

```text
Install Freehold Ledger for me. Follow https://github.com/jl12-studio/freehold-ledger/blob/main/INSTALL.md
```

Your agent downloads the app, checks it, installs it, connects itself and tells you where your ledger file is.

No agent? Download the disk image: [Freehold-Ledger-universal.dmg](https://github.com/jl12-studio/freehold-ledger/releases/latest/download/Freehold-Ledger-universal.dmg).

## The scene

*"I bought a $300 device, add it to the books so it's ready when I file taxes."*

There is no subscription. The database belongs to the person who keeps it.

## What makes it Freehold

Freehold Ledger follows the Freehold Standard. Read more at:

- [freeholders.org/standard](https://freeholders.org/standard)
- [PHILOSOPHY.md](https://github.com/jl12-studio/freehold/blob/main/PHILOSOPHY.md)
- [FRAMEWORK.md](https://github.com/jl12-studio/freehold/blob/main/FRAMEWORK.md)

The Standard points this app is built around:

- Point 1: Free, and owned by the person.
- Point 2: Deployable anywhere.
- Point 3: Speaks MCP natively.
- Point 5: A declared conflict story.
- Point 6: Agent-operable administration.
- Point 7: Secure by default.
- Point 8: Self-carrying upgrades.
- Point 9: Usable by conversation.
- Point 10: Teaches while it works.

Check it yourself: [FREEHOLD.md](FREEHOLD.md) lists the ten points with their evidence, and `bash scripts/freehold-check.sh` shows where each one stands.

## Status

Version 1.0.0. Mac (Apple Silicon and Intel). Desktop grade of the Freehold Standard: see FREEHOLD.md.

## Two guides

Freehold apps carry two guides: one for the person ([`docs/HUMAN-GUIDE.md`](docs/HUMAN-GUIDE.md)) and one for their agent ([`docs/AGENT-GUIDE.md`](docs/AGENT-GUIDE.md)). The app's Connect your agent panel gives you one message to paste to your agent so it sets itself up.

## Build from source

You need Rust, Node 24 and pnpm.

```bash
pnpm --dir app install
pnpm --dir app tauri dev
```

To build the Mac app with the bundled MCP server:

```bash
scripts/build-sidecar.sh
pnpm --dir app tauri build --bundles app
```

The app lands at `target/release/bundle/macos/Freehold Ledger.app`, with the MCP server at `Freehold Ledger.app/Contents/MacOS/ledger-mcp`. The build is local and unsigned. `scripts/check-no-listeners.sh` runs the built app and server against a scratch ledger and confirms neither opens a network socket.

To build the universal release disk image and checksums into `dist/release/`, run `FREEHOLD_SIGN_IDENTITY="Developer ID Application: Your Name (TEAMID)" scripts/release.sh`.
Add `--notarize` to also notarize it with Apple (keychain profile in `FREEHOLD_NOTARY_PROFILE`, default `freehold-notary`).
Add `--no-sign` instead for an unsigned build that needs no Apple developer account.

The app opens the ledger at its default location. To point it somewhere else (for example a scratch file while you try things out), set `FREEHOLD_LEDGER_DB` to a file path before launching. The same variable is honored by the app, the MCP server and the tests.

## License

GNU Affero General Public License v3.0 or later. See `LICENSE`.
