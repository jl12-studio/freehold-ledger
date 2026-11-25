//! Backup, verify and restore that are safe while the ledger is open.
//!
//! Everything goes through SQLite's online backup API, never by copying or replacing the file, so
//! other open connections (the app window, the MCP server) keep working.
use crate::db::{migrate, Ledger, SUPPORTED_SCHEMA};
use crate::error::LedgerError;
use chrono::{DateTime, Local};
use rusqlite::backup::Backup;
use rusqlite::{Connection, ErrorCode, OpenFlags, OptionalExtension};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
pub struct BackupInfo {
    pub path: PathBuf,
    pub bytes: u64,
    pub schema_version: u32,
    pub transaction_count: i64,
    /// Local time, RFC 3339.
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct VerifyReport {
    pub ok: bool,
    pub problems: Vec<String>,
    pub schema_version: u32,
    pub supported_schema: u32,
    pub transaction_count: i64,
    pub business_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RestoreInfo {
    pub restored_from: PathBuf,
    pub safety_backup: BackupInfo,
    pub schema_version: u32,
    pub transaction_count: i64,
}

const PAGES_PER_STEP: std::os::raw::c_int = 256;

impl Ledger {
    /// Write a verified copy of the ledger into `folder` (default: `backups/` beside the ledger).
    pub fn backup_to(&self, folder: Option<&Path>) -> Result<BackupInfo, LedgerError> {
        let folder = self.backup_folder(folder);
        std::fs::create_dir_all(&folder)?;
        let now = Local::now();
        let schema = self.verify()?.schema_version;
        let stem = format!("ledger-{}-schema{schema}", now.format("%Y%m%d-%H%M%S"));
        let mut path = folder.join(format!("{stem}.sqlite3"));
        let mut n = 2;
        while path.exists() {
            path = folder.join(format!("{stem}-{n}.sqlite3"));
            n += 1;
        }
        if let Err(e) = write_copy(self.conn(), &path) {
            let _ = std::fs::remove_file(&path);
            return Err(e);
        }
        let report = match Ledger::verify_file(&path) {
            Ok(r) if r.ok => r,
            Ok(r) => {
                let _ = std::fs::remove_file(&path);
                return Err(LedgerError::Io(std::io::Error::other(format!(
                    "the new backup did not verify: {}",
                    r.problems.join("; ")
                ))));
            }
            Err(e) => {
                let _ = std::fs::remove_file(&path);
                return Err(e);
            }
        };
        Ok(BackupInfo {
            bytes: std::fs::metadata(&path)?.len(),
            path,
            schema_version: report.schema_version,
            transaction_count: report.transaction_count,
            created_at: now.to_rfc3339(),
        })
    }

    /// Check the open ledger's integrity. Problems are reported, not returned as `Err`.
    pub fn verify(&self) -> Result<VerifyReport, LedgerError> {
        verify_conn(self.conn(), self.path())
    }

    /// Check a file that is not the open ledger: read-only, never migrated, never created.
    pub fn verify_file(path: &Path) -> Result<VerifyReport, LedgerError> {
        if !path.is_file() {
            return Err(LedgerError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("{} does not exist", path.display()),
            )));
        }
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| map_open(e, path))?;
        conn.busy_timeout(Duration::from_millis(5000))?;
        verify_conn(&conn, path)
    }

    /// Backups in `folder` (default: `backups/` beside the ledger), newest first. Files that are
    /// not readable ledgers are skipped.
    pub fn list_backups(&self, folder: Option<&Path>) -> Result<Vec<BackupInfo>, LedgerError> {
        let folder = self.backup_folder(folder);
        if !folder.is_dir() {
            return Ok(Vec::new());
        }
        let mut found = Vec::new();
        for entry in std::fs::read_dir(&folder)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("sqlite3") {
                continue;
            }
            let Ok(report) = Ledger::verify_file(&path) else { continue };
            let Ok(meta) = std::fs::metadata(&path) else { continue };
            let modified = meta.modified().ok();
            let created_at = modified
                .map(|m| DateTime::<Local>::from(m).to_rfc3339())
                .unwrap_or_default();
            found.push((
                modified,
                BackupInfo {
                    path,
                    bytes: meta.len(),
                    schema_version: report.schema_version,
                    transaction_count: report.transaction_count,
                    created_at,
                },
            ));
        }
        found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.path.cmp(&a.1.path)));
        Ok(found.into_iter().map(|(_, b)| b).collect())
    }

    /// Replace the ledger's contents with a backup, after taking a safety backup of the current
    /// state. Other open connections see the restored data on their next read.
    pub fn restore_from(&mut self, backup: &Path) -> Result<RestoreInfo, LedgerError> {
        let refuse = |reason: String| LedgerError::RestoreRefused { reason };
        if let (Ok(a), Ok(b)) = (backup.canonicalize(), self.path().canonicalize()) {
            if a == b {
                return Err(refuse("That file is the open ledger itself.".into()));
            }
        }
        let report = Ledger::verify_file(backup)?;
        if report.schema_version > report.supported_schema {
            return Err(refuse(format!(
                "That file was made by a newer Freehold Ledger (schema {}; this app supports up to {}).",
                report.schema_version, report.supported_schema
            )));
        }
        if !report.ok {
            return Err(refuse(format!("That file failed its check: {}.", report.problems.join("; "))));
        }

        let safety_backup = self.backup_to(None)?;

        let src = Connection::open_with_flags(backup, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        src.busy_timeout(Duration::from_millis(5000))?;
        {
            let b = Backup::new(&src, self.conn_mut())?;
            run_backup(&b)?;
        }
        let path = self.path().to_path_buf();
        migrate(self.conn_mut(), &path)?;
        let after = self.verify()?;
        Ok(RestoreInfo {
            restored_from: backup.to_path_buf(),
            safety_backup,
            schema_version: after.schema_version,
            transaction_count: after.transaction_count,
        })
    }

    fn backup_folder(&self, folder: Option<&Path>) -> PathBuf {
        match folder {
            Some(f) => f.to_path_buf(),
            None => self
                .path()
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."))
                .join("backups"),
        }
    }
}

fn run_backup(b: &Backup<'_, '_>) -> Result<(), LedgerError> {
    b.run_to_completion(PAGES_PER_STEP, Duration::from_millis(5), None)?;
    Ok(())
}

fn write_copy(src: &Connection, dest: &Path) -> Result<(), LedgerError> {
    let mut dst = Connection::open(dest)?;
    {
        let b = Backup::new(src, &mut dst)?;
        run_backup(&b)?;
    }
    // A backup is one self-contained file, not a write-ahead-log pair.
    dst.query_row("PRAGMA journal_mode=DELETE", [], |_| Ok(()))?;
    Ok(())
}

fn map_open(e: rusqlite::Error, path: &Path) -> LedgerError {
    if is_not_a_db(&e) {
        LedgerError::NotALedger { path: path.to_path_buf() }
    } else {
        e.into()
    }
}

fn is_not_a_db(e: &rusqlite::Error) -> bool {
    matches!(
        e.sqlite_error_code(),
        Some(ErrorCode::NotADatabase) | Some(ErrorCode::DatabaseCorrupt)
    )
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool, rusqlite::Error> {
    let n: i64 = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
        [name],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

fn verify_conn(conn: &Connection, path: &Path) -> Result<VerifyReport, LedgerError> {
    let not_a_ledger = || LedgerError::NotALedger { path: path.to_path_buf() };
    let inner = || -> Result<VerifyReport, rusqlite::Error> {
        if !table_exists(conn, "schema_meta")? {
            return Ok(VerifyReport {
                ok: false,
                problems: Vec::new(),
                schema_version: 0,
                supported_schema: SUPPORTED_SCHEMA,
                transaction_count: 0,
                business_count: 0,
            });
        }
        let version: Option<u32> = conn
            .query_row("SELECT value FROM schema_meta WHERE key='schema_version'", [], |r| {
                r.get::<_, String>(0)
            })
            .optional()?
            .and_then(|v| v.parse().ok());
        let schema_version = version.unwrap_or(0);
        let mut problems = Vec::new();

        let mut stmt = conn.prepare("PRAGMA integrity_check")?;
        for row in stmt.query_map([], |r| r.get::<_, String>(0))? {
            let line = row?;
            if line != "ok" {
                problems.push(format!("integrity check: {line}"));
            }
        }
        let mut stmt = conn.prepare("PRAGMA foreign_key_check")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?, r.get::<_, String>(2)?))
        })?;
        for row in rows {
            let (table, rowid, parent) = row?;
            let rowid = rowid.map(|i| i.to_string()).unwrap_or_else(|| "?".into());
            problems.push(format!("foreign key check: {table} row {rowid} points at a missing {parent} row"));
        }

        if schema_version > SUPPORTED_SCHEMA {
            problems.push(format!(
                "made by a newer Freehold Ledger (schema {schema_version}; this app supports up to {SUPPORTED_SCHEMA})"
            ));
        }
        let transaction_count = if table_exists(conn, "transactions")? {
            conn.query_row("SELECT count(*) FROM transactions", [], |r| r.get(0))?
        } else {
            0
        };
        let business_count = if table_exists(conn, "businesses")? {
            conn.query_row("SELECT count(*) FROM businesses", [], |r| r.get(0))?
        } else {
            0
        };
        Ok(VerifyReport {
            ok: version.is_some() && problems.is_empty(),
            problems,
            schema_version,
            supported_schema: SUPPORTED_SCHEMA,
            transaction_count,
            business_count,
        })
    };
    match inner() {
        Ok(r) if r.schema_version == 0 => Err(not_a_ledger()),
        Ok(r) => Ok(r),
        Err(e) if is_not_a_db(&e) => Err(not_a_ledger()),
        Err(e) => Err(e.into()),
    }
}
