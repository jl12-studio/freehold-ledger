//! `ledger-mcp`: the Freehold Ledger MCP server. Speaks MCP over stdio only; it opens no socket.

mod tools;

use anyhow::Result;
use ledger_core::Ledger;
use rmcp::{transport::stdio, ServiceExt};
use tools::LedgerTools;

#[tokio::main]
async fn main() -> Result<()> {
    // Logs go to stderr only: stdout belongs to the MCP protocol.
    // An unopenable ledger must not stop the server: every tool then reports the reason, so the
    // agent can tell the person what is wrong.
    let ledger = Ledger::open_default().map_err(|e| {
        eprintln!("ledger-mcp: could not open the ledger: {e}");
        e.to_string()
    });
    if let Ok(l) = &ledger {
        eprintln!("ledger-mcp: serving {} over stdio", l.path().display());
    }
    let service = LedgerTools::new(ledger).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
