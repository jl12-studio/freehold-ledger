# Decisions

## Platform: Tauri 2 + Rust

Dated 2026-10-27. Freehold Ledger is a small native desktop app, about 10 MB, built on Tauri 2 with a Rust core. It ships Mac first and stays cross-platform. One Rust core serves both the window and the MCP server. The MCP server runs as a stdio sidecar inside the app, so nothing listens on the network. Rejected: Electron + TypeScript (about 150 MB and a heavier runtime), and native SwiftUI (Mac only).

## License: AGPL-3.0-or-later

Dated 2026-10-27. The app is free and owned by the person who uses it. The network clause means nobody can host a closed fork and rent it back as a subscription. Rejected: MIT (anyone can close a fork), and GPL-3.0 (no network clause).

## Repo: its own home

Dated 2026-10-27. The repo lives at `/Users/marcus.salinas/Programming/Freehold Apps/freehold-ledger`, is its own git repo on local `main`, and has no remote. Apps never live in the freehold repo.

## Data: one SQLite file the person owns

Dated 2026-10-27. All ledger data lives in one SQLite file. The default path is `ledger_core::default_db_path()`, which resolves to `dirs::data_dir()/org.freeholders.ledger/ledger.sqlite3` (on macOS, `~/Library/Application Support/org.freeholders.ledger/ledger.sqlite3`). The environment variable `FREEHOLD_LEDGER_DB` overrides that path everywhere: the app, the MCP server, and tests. Details are in `DATA-MODEL.md`.

## Revisiting

Dated 2026-10-27. Any decision here can be revisited. Write the new decision as a new dated section; do not rewrite history.
