use crate::categories::valid_keys;
use crate::model::Kind;
use chrono::NaiveDate;
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
    #[error("The {field} looks like it holds a full card number. Freehold Ledger only ever keeps the last 4 digits of a card, so nothing was stored. Leave the number out, or keep just its last 4 digits as last4 on a payment method.")]
    CardNumberInText { field: String },
    #[error("The {field} holds a long number that looks like a card, bank account or routing number. Freehold Ledger only ever keeps the last 4 digits, so nothing was stored. Leave the number out, or keep just its last 4 digits.")]
    AccountNumberInText { field: String },
    #[error("Give the payment method a short nickname, for example \"Business Visa\".")]
    MissingNickname,
    #[error("Keep the nickname to 40 characters or fewer.")]
    NicknameTooLong,
    #[error("\"none\" is reserved for entries with no payment method. Pick another nickname.")]
    NicknameReserved,
    #[error("A payment method called \"{0}\" is already in use. Pick another nickname, or retire the old one first.")]
    NicknameTaken(String),
    #[error("last4 is exactly 4 digits, for example 4242. Freehold Ledger never keeps a full card or account number, so nothing was stored.")]
    InvalidLast4,
    #[error("Cash has no number, so leave last4 out.")]
    Last4NotForCash,
    #[error("No payment method matches \"{0}\". Call list_payment_methods to see the nicknames and ids.")]
    UnknownPaymentMethod(String),
    #[error("More than one retired payment method is called \"{nickname}\" (ids {ids}). Use the id instead.")]
    AmbiguousPaymentMethod { nickname: String, ids: String },
    #[error("\"{0}\" is retired, so nothing new can go on it. Use another payment method or create a new one.")]
    RetiredPaymentMethod(String),
    #[error("\"{0}\" is already retired.")]
    AlreadyRetired(String),
    #[error("Dates look like 2026-10-27.")]
    InvalidDate(String),
    #[error("Transaction {0} was not found.")]
    NotFound(i64),
    #[error("Batch {0} was not found. The batch_id is in the record_transactions result and on every entry in list_transactions.")]
    UnknownBatch(i64),
    #[error("Transaction {0} is already voided.")]
    AlreadyVoided(i64),
    #[error("Transaction {0} was voided; record a new one instead.")]
    Voided(i64),
    #[error("The start date {from} is after the end date {to}. Put the earlier date first.")]
    InvalidRange { from: NaiveDate, to: NaiveDate },
    #[error("Give at least one entry to record.")]
    EmptyBatch,
    #[error("{count} entries is more than one call takes. Send at most 50 entries per call, for example 50 rows of the file at a time. Nothing was recorded.")]
    BatchTooLarge { count: usize },
    #[error("Give the batch a short label, for example the name of the file the rows came from.")]
    MissingBatchLabel,
    #[error("Entry {index}: {reason} Nothing in this batch was recorded.")]
    BatchEntry { index: usize, reason: Box<LedgerError> },
    #[error("Could not write the export to {}: {reason}. Nothing was created.", .path.display())]
    ExportFailed { path: PathBuf, reason: String },
}
