# Decisions

## Platform: Tauri 2 + Rust

Dated 2026-10-27. Freehold Ledger is a small native desktop app, about 10 MB, built on Tauri 2 with a Rust core. It ships Mac first and stays cross-platform. One Rust core serves both the window and the MCP server. The MCP server runs as a stdio sidecar inside the app, so nothing listens on the network. Rejected: Electron + TypeScript (about 150 MB and a heavier runtime), and native SwiftUI (Mac only).

## License: AGPL-3.0-or-later

Dated 2026-10-27. The app is free and owned by the person who uses it. The network clause means nobody can host a closed fork and rent it back as a subscription. Rejected: MIT (anyone can close a fork), and GPL-3.0 (no network clause).

## Repo: its own home

Dated 2026-10-27. The repo lives at `/Users/marcus.salinas/Programming/Freehold Apps/freehold-ledger`, is its own git repo on local `main`, and has no remote. Apps never live in the freehold repo.

## Data: one SQLite file the person owns

Dated 2026-10-27. All ledger data lives in one SQLite file. The default path is `ledger_core::default_db_path()`, which resolves to `dirs::data_dir()/org.freeholders.ledger/ledger.sqlite3` (on macOS, `~/Library/Application Support/org.freeholders.ledger/ledger.sqlite3`). The environment variable `FREEHOLD_LEDGER_DB` overrides that path everywhere: the app, the MCP server, and tests. Details are in `DATA-MODEL.md`.

## Sidecar location

Dated 2026-10-28. `ledger-mcp` ships as a Tauri sidecar (`bundle.externalBin: ["binaries/ledger-mcp"]`). `scripts/build-sidecar.sh` builds it in release and copies it to `app/src-tauri/binaries/ledger-mcp-<host triple>`; both `beforeDevCommand` and `beforeBuildCommand` run that script first. Tauri strips the triple suffix when it places the file, so the sidecar sits next to the app executable in both modes:

- In the bundle: `Freehold Ledger.app/Contents/MacOS/ledger-mcp`, beside `freehold-ledger-app`. Observed in `target/release/bundle/macos/`.
- In `tauri dev`: `target/debug/ledger-mcp`, beside `target/debug/freehold-ledger-app`. Observed: after `cargo build -p freehold-ledger-app`, `target/debug/ledger-mcp` is byte-identical to the release sidecar copy (tauri-build copies the externalBin there).

So `std::env::current_exe()?.parent()/ledger-mcp` resolves in both. In dev it is the copy made at the last build, so rebuild (or rerun `tauri dev`) after changing `ledger-mcp`.

## Revisiting

Dated 2026-10-27. Any decision here can be revisited. Write the new decision as a new dated section; do not rewrite history.
