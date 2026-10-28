# Freehold Ledger

A free bookkeeping app you own outright. Your agent keeps the books by conversation; the records live in one file on your computer.

## The scene

*"I bought a $300 device, add it to the books so it's ready when I file taxes."*

There is no subscription. The database belongs to the person who keeps it.

## What makes it Freehold

Freehold Ledger follows the Freehold Standard. Read more at:

- [freeholders.org/standard](https://freeholders.org/standard)
- [PHILOSOPHY.md](https://github.com/jl12-studio/freehold/blob/main/PHILOSOPHY.md)
- [FRAMEWORK.md](https://github.com/jl12-studio/freehold/blob/main/FRAMEWORK.md)

The Standard points this app is built around:

1. Free, and owned by the person.
3. Speaks MCP natively.
5. A declared conflict story.
7. Secure by default.
8. Self-carrying upgrades.
9. Usable by conversation.
10. Teaches while it works.

## Status

Stage 1 (local desktop, Mac first). Not released yet.

## Two guides

Freehold apps carry two guides: one for the person (this README and `docs/HUMAN-GUIDE.md`, coming in M3) and one for their agent (`docs/AGENT-GUIDE.md`, coming in M3).

## Build from source

You need Rust, Node 24 and pnpm.

```bash
pnpm --dir app install
pnpm --dir app tauri dev
```

The app opens the ledger at its default location. To point it somewhere else (for example a scratch file while you try things out), set `FREEHOLD_LEDGER_DB` to a file path before launching. The same variable is honored by the app, the MCP server and the tests.

## License

GNU Affero General Public License v3.0 or later. See `LICENSE`.
