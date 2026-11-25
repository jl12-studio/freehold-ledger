# Install Freehold Ledger

This file is written for your agent to follow. A person can follow it too. It installs Freehold Ledger, a free bookkeeping app, in `/Applications`, and connects your agent to it. The app and its ledger stay on your Mac, and nothing is sent anywhere.

The sentence that starts it:

```text
Install Freehold Ledger for me. Follow https://github.com/jl12-studio/freehold-ledger/blob/main/INSTALL.md
```

## What you need

- A Mac, Apple Silicon or Intel, running macOS 11 or later.
- An agent that can run commands and add a local MCP server to its own settings.

Windows and Linux: not yet. Build from source instead, see "Build from source" in the [README](README.md#build-from-source).

## Steps for the agent

1. Download the disk image and the checksums into a temporary folder.

   ```bash
   cd "$(mktemp -d)"
   curl -fL -O https://github.com/jl12-studio/freehold-ledger/releases/latest/download/Freehold-Ledger-universal.dmg
   curl -fL -O https://github.com/jl12-studio/freehold-ledger/releases/latest/download/SHA256SUMS
   ```

2. Check the download. Stop if this fails.

   ```bash
   shasum -a 256 -c SHA256SUMS
   ```

3. Copy the app into `/Applications`. If an older copy is already there, follow "Upgrading the app" in `docs/AGENT-GUIDE.md` first (back up, ask the person to quit the app).

   ```bash
   hdiutil attach Freehold-Ledger-universal.dmg -nobrowse -readonly -mountpoint "$PWD/mount"
   ditto "$PWD/mount/Freehold Ledger.app" "/Applications/Freehold Ledger.app"
   hdiutil detach "$PWD/mount"
   ```

4. Verify the signature and Gatekeeper's verdict, and tell the person the result.

   ```bash
   codesign --verify --deep --strict "/Applications/Freehold Ledger.app"
   spctl --assess --type execute "/Applications/Freehold Ledger.app"
   ```

5. Add the MCP server to your own settings: server name `freehold-ledger`, command `/Applications/Freehold Ledger.app/Contents/MacOS/ledger-mcp`, no arguments. In Claude Code:

   ```bash
   claude mcp add freehold-ledger -- "/Applications/Freehold Ledger.app/Contents/MacOS/ledger-mcp"
   ```

   Claude Desktop and other JSON-configured agents:

   ```json
   {
     "mcpServers": {
       "freehold-ledger": {
         "command": "/Applications/Freehold Ledger.app/Contents/MacOS/ledger-mcp"
       }
     }
   }
   ```

   "Connection" in `docs/AGENT-GUIDE.md` has the same snippets. Restart or reload your agent if it needs that.

6. Call the `ledger_info` tool. Tell the person where their ledger file is and which version is installed.

7. Open the app once so the person sees their books.

   ```bash
   open -a "Freehold Ledger"
   ```

8. Read `docs/AGENT-GUIDE.md` before recording anything.

## If something fails

- **Checksum mismatch**: stop. Download both files again. Never install a disk image that fails the check.
- **macOS refuses to open the app**: do not bypass Gatekeeper. Report what `spctl` said to the person.
- **You cannot change your own settings**: walk the person through "Connect your agent" in `docs/HUMAN-GUIDE.md`, one step at a time.

## Uninstall

Quit the app, then remove it and the MCP server entry.

```bash
rm -rf "/Applications/Freehold Ledger.app"
claude mcp remove freehold-ledger
```

For other agents, delete the `freehold-ledger` entry from their MCP settings.

The ledger file and any backups are not removed. They belong to the person and stay where they are. By default the ledger is `~/Library/Application Support/org.freeholders.ledger/ledger.sqlite3`.

## Build it yourself

The source is at https://github.com/jl12-studio/freehold-ledger.
Releases are at https://github.com/jl12-studio/freehold-ledger/releases/latest.
To build the app and the disk image on your own Mac, follow "Build from source" in the [README](README.md#build-from-source).
