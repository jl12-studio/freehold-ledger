use crate::error::LedgerError;
use crate::migrations::MIGRATIONS;
use rusqlite::{Connection, ErrorCode, OptionalExtension, TransactionBehavior};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const SUPPORTED_SCHEMA: u32 = 4;

pub fn default_db_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("org.freeholders.ledger")
        .join("ledger.sqlite3")
}

/// `FREEHOLD_LEDGER_DB` if set and non-empty, else the default path.
pub fn resolve_db_path() -> PathBuf {
    match std::env::var_os("FREEHOLD_LEDGER_DB") {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => default_db_path(),
    }
}

#[derive(Debug, Serialize)]
pub struct LedgerInfo {
    pub path: String,
    pub schema_version: u32,
    pub transaction_count: i64,
    /// The agent's current business: the latest `current_business_changes` row, if any.
    pub current_business_id: Option<i64>,
}

pub struct Ledger {
    conn: Connection,
    path: PathBuf,
}

enum Probe {
    /// No tables at all: a new, empty file.
    Empty,
    /// A ledger at this schema version.
    Ledger(u32),
}

impl Ledger {
    pub fn open(path: &Path) -> Result<Ledger, LedgerError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let mut conn = Connection::open(path)?;
        conn.busy_timeout(Duration::from_millis(5000))?;
        let not_a_ledger = || LedgerError::NotALedger { path: path.to_path_buf() };

        // Read-only probing: nothing is written until the file is known to be ours.
        match probe(&conn) {
            Ok(Probe::Ledger(v)) if v > SUPPORTED_SCHEMA => {
                return Err(LedgerError::NewerSchema { found: v, supported: SUPPORTED_SCHEMA })
            }
            Ok(_) => {}
            Err(ProbeError::NotLedger) => return Err(not_a_ledger()),
            Err(ProbeError::Sqlite(e)) if is_not_a_db(&e) => return Err(not_a_ledger()),
            Err(ProbeError::Sqlite(e)) => return Err(e.into()),
        }

        // D6 pragmas, then migrate in one BEGIN IMMEDIATE transaction.
        conn.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(()))?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;

        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        // Re-read under the write lock: another process may have migrated or upgraded meanwhile.
        let current = match probe(&tx) {
            Ok(Probe::Empty) => 0,
            Ok(Probe::Ledger(v)) => v,
            Err(ProbeError::NotLedger) => return Err(not_a_ledger()),
            Err(ProbeError::Sqlite(e)) => return Err(e.into()),
        };
        if current > SUPPORTED_SCHEMA {
            return Err(LedgerError::NewerSchema { found: current, supported: SUPPORTED_SCHEMA });
        }
        for (version, sql) in MIGRATIONS.iter().filter(|(v, _)| *v > current) {
            tx.execute_batch(sql)?;
            tx.execute(
                "INSERT INTO schema_meta (key, value) VALUES ('schema_version', ?1) \
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [version.to_string()],
            )?;
        }
        tx.commit()?;

        Ok(Ledger { conn, path: path.to_path_buf() })
    }

    pub fn open_default() -> Result<Ledger, LedgerError> {
        Ledger::open(&resolve_db_path())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn info(&self) -> Result<LedgerInfo, LedgerError> {
        let schema_version = match probe(&self.conn) {
            Ok(Probe::Ledger(v)) => v,
            Ok(Probe::Empty) | Err(ProbeError::NotLedger) => {
                return Err(LedgerError::NotALedger { path: self.path.clone() })
            }
            Err(ProbeError::Sqlite(e)) => return Err(e.into()),
        };
        let transaction_count =
            self.conn.query_row("SELECT count(*) FROM transactions", [], |r| r.get(0))?;
        let current_business_id: Option<i64> = self
            .conn
            .query_row(
                "SELECT business_id FROM current_business_changes ORDER BY id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        Ok(LedgerInfo {
            path: self.path.display().to_string(),
            schema_version,
            transaction_count,
            current_business_id,
        })
    }

    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

    pub(crate) fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }
}

enum ProbeError {
    NotLedger,
    Sqlite(rusqlite::Error),
}

impl From<rusqlite::Error> for ProbeError {
    fn from(e: rusqlite::Error) -> Self {
        ProbeError::Sqlite(e)
    }
}

fn is_not_a_db(e: &rusqlite::Error) -> bool {
    matches!(
        e.sqlite_error_code(),
        Some(ErrorCode::NotADatabase) | Some(ErrorCode::DatabaseCorrupt)
    )
}

fn probe(conn: &Connection) -> Result<Probe, ProbeError> {
    let tables: i64 = conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get(0))?;
    if tables == 0 {
        return Ok(Probe::Empty);
    }
    let has_meta: i64 = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='schema_meta'",
        [],
        |r| r.get(0),
    )?;
    if has_meta == 0 {
        return Err(ProbeError::NotLedger);
    }
    let value: Option<String> = conn
        .query_row("SELECT value FROM schema_meta WHERE key='schema_version'", [], |r| r.get(0))
        .optional()?;
    match value.and_then(|v| v.parse::<u32>().ok()) {
        Some(v) => Ok(Probe::Ledger(v)),
        None => Err(ProbeError::NotLedger),
    }
}
