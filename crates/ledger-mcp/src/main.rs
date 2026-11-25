//! `ledger-mcp`: the Freehold Ledger MCP server. Speaks MCP over stdio only; it opens no socket.

mod tools;

use anyhow::Result;
use ledger_core::{resolve_db_path, Ledger};
use rmcp::{transport::stdio, ServiceExt};
use tools::LedgerTools;

#[tokio::main]
async fn main() -> Result<()> {
    // Logs go to stderr only: stdout belongs to the MCP protocol.
    // An unopenable ledger must not stop the server: every tool then reports the reason, so the
    // agent can tell the person what is wrong. A ledger a newer version opened still gets the
    // admin tools, so the backup taken before the upgrade can be restored.
    let path = resolve_db_path();
    let ledger = Ledger::open(&path);
    match &ledger {
        Ok(l) => eprintln!("ledger-mcp: serving {} over stdio", l.path().display()),
        Err(e) => eprintln!("ledger-mcp: could not open the ledger: {e}"),
    }
    let service = LedgerTools::new(path, ledger).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
