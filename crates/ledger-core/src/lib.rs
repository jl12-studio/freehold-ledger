//! Pure library that owns the Freehold Ledger SQLite file.
mod admin;
mod businesses;
mod categories;
mod db;
mod error;
mod export;
mod guard;
mod migrations;
mod model;
mod ops;
mod payment_methods;
mod seed;
mod summary;

pub use db::{default_db_path, resolve_db_path, Ledger, LedgerInfo, SUPPORTED_SCHEMA};
pub use admin::{BackupInfo, RestoreInfo, VerifyReport};
pub use error::LedgerError;

/// The version of this build, for the agent to report.
pub fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub use categories::{categories, category, Category};
pub use model::{
    Assignment, BatchEntry, BatchResult, BatchVoid, ClassificationChange, Correction, Filter, Kind, Business, BusinessKey, NewBatch, NewBusiness, NewEntry,
    NewExpense, NewIncome, NewPaymentMethod, PaymentKind, PaymentMethod, PaymentMethodKey, PaymentMethodFilter, PaymentMethodRef,
    RecordedEntry, Replacement, SkippedDuplicate, Transaction, VoidInfo,
    MAX_ASSIGN, MAX_BATCH,
};
pub use export::ExportResult;
pub use guard::{check_no_full_number, MIN_FULL_NUMBER_DIGITS};
pub use ops::parse_date;
pub use seed::{seed_demo, seed_demo_businesses, seed_demo_payment_methods};
pub use summary::{resolve_period, CategoryTotal, Period, PaymentMethodTotal, SourceTotal, Summary, SummaryRequest, Totals};
