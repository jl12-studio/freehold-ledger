use crate::categories::valid_keys;
use crate::model::Kind;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LedgerError {
    #[error("Could not read or write the ledger file: {0}")]
    Io(#[from] std::io::Error),
    #[error("The ledger database reported an error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("{} is not a Freehold ledger. Nothing was changed.", .path.display())]
    NotALedger { path: PathBuf },
    #[error("This ledger was created by a newer Freehold Ledger (schema {found}); this app supports up to schema {supported}. Update the app to open it. Nothing was changed.")]
    NewerSchema { found: u32, supported: u32 },
    #[error("Amount must be more than $0.00.")]
    InvalidAmount(i64),
    #[error("\"{key}\" is not a {} category. Valid categories: {}.", kind.as_str(), valid_keys(*kind).join(", "))]
    UnknownCategory { key: String, kind: Kind },
    #[error("Add a short description of what this was.")]
    MissingDescription,
    #[error("Income needs a source, for example the client or platform that paid.")]
    MissingSource,
    #[error("Add a short reason for this change.")]
    MissingReason,
    #[error("Dates look like 2026-10-27.")]
    InvalidDate(String),
    #[error("Transaction {0} was not found.")]
    NotFound(i64),
    #[error("Transaction {0} is already voided.")]
    AlreadyVoided(i64),
    #[error("Transaction {0} was voided; record a new one instead.")]
    Voided(i64),
}
