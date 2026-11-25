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
    /// What the check found wrong with the books as they were before the restore; empty when
    /// they were sound. A safety backup with problems is kept but cannot itself be restored.
    pub safety_backup_problems: Vec<String>,
    pub schema_version: u32,
    pub transaction_count: i64,
}

const PAGES_PER_STEP: std::os::raw::c_int = 256;

impl Ledger {
    /// Write a verified copy of the ledger into `folder` (default: `backups/` beside the ledger).
    pub fn backup_to(&self, folder: Option<&Path>) -> Result<BackupInfo, LedgerError> {
        let schema = self.verify()?.schema_version;
        let (path, now) = self.write_backup(folder, schema)?;
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

    /// Copy the open ledger to a new, uniquely named file in `folder`. The copy is not checked.
    fn write_backup(&self, folder: Option<&Path>, schema: u32) -> Result<(PathBuf, DateTime<Local>), LedgerError> {
        write_backup_of(self.conn(), self.path(), folder, schema)
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
        Ledger::list_backups_in(&self.backup_folder(folder))
    }

    /// Backups in `folder`, newest first, without opening any ledger. Files that are not readable
    /// ledgers are skipped; a folder that does not exist holds none.
    pub fn list_backups_in(folder: &Path) -> Result<Vec<BackupInfo>, LedgerError> {
        if !folder.is_dir() {
            return Ok(Vec::new());
        }
        let mut found = Vec::new();
        for entry in std::fs::read_dir(folder)? {
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

    /// Where backups go by default for the ledger at `ledger`: a `backups` folder beside it.
    pub fn default_backup_folder(ledger: &Path) -> PathBuf {
        ledger_dir_of(ledger).join("backups")
    }

    /// Replace the ledger's contents with a backup, after taking a safety backup of the current
    /// state. The backup is first copied aside, upgraded and checked, so the live books change in
    /// one step or not at all. Other open connections see the restored data on their next read.
    pub fn restore_from(&mut self, backup: &Path) -> Result<RestoreInfo, LedgerError> {
        let path = self.path().to_path_buf();
        restore_into(self.conn_mut(), &path, backup)
    }

    /// The same restore for the ledger file at `ledger`, without opening it as a `Ledger`. This is
    /// the way back when `Ledger::open` refuses the file because a newer version raised its format:
    /// the file is only copied (the safety copy, a raw backup-API copy, is kept as it is) and then
    /// overwritten by the checked backup, never migrated. The backup itself must be at or below
    /// `SUPPORTED_SCHEMA`. The file must already exist and be a Freehold ledger.
    pub fn restore_file(ledger: &Path, backup: &Path) -> Result<RestoreInfo, LedgerError> {
        if !ledger.is_file() {
            return Err(LedgerError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("{} does not exist", ledger.display()),
            )));
        }
        let mut conn = Connection::open_with_flags(ledger, OpenFlags::SQLITE_OPEN_READ_WRITE)
            .map_err(|e| map_open(e, ledger))?;
        conn.busy_timeout(Duration::from_millis(5000))?;
        // Refuses a file that is not a ledger before anything is copied or written.
        verify_conn(&conn, ledger)?;
        restore_into(&mut conn, ledger, backup)
    }

    fn backup_folder(&self, folder: Option<&Path>) -> PathBuf {
        match folder {
            Some(f) => f.to_path_buf(),
            None => Ledger::default_backup_folder(self.path()),
        }
    }
}

fn ledger_dir_of(ledger: &Path) -> PathBuf {
    ledger
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

/// Copy the ledger open on `conn` (the file at `ledger`) to a new, uniquely named file in `folder`
/// (default: `backups/` beside the ledger). The copy is not checked.
fn write_backup_of(
    conn: &Connection,
    ledger: &Path,
    folder: Option<&Path>,
    schema: u32,
) -> Result<(PathBuf, DateTime<Local>), LedgerError> {
    let folder = match folder {
        Some(f) => f.to_path_buf(),
        None => Ledger::default_backup_folder(ledger),
    };
    std::fs::create_dir_all(&folder)?;
    let now = Local::now();
    let stem = format!("ledger-{}-schema{schema}", now.format("%Y-%m-%d-%H%M%S"));
    let mut path = folder.join(format!("{stem}.sqlite3"));
    let mut n = 2;
    while path.exists() {
        path = folder.join(format!("{stem}-{n}.sqlite3"));
        n += 1;
    }
    if let Err(e) = write_copy(conn, &path) {
        let _ = std::fs::remove_file(&path);
        return Err(e);
    }
    Ok((path, now))
}

/// The copy of the current books taken before a restore. It is kept even when the current
/// books fail their check: it is then the only copy of them, and refusing would block the
/// very restore that verify_ledger recommends. Its problems are returned beside it. A file from a
/// newer version is not damaged, so that alone is not a problem: the copy is kept as it is.
fn safety_copy(conn: &Connection, ledger: &Path) -> Result<(BackupInfo, Vec<String>), LedgerError> {
    let schema = verify_conn(conn, ledger).map(|r| r.schema_version).unwrap_or(SUPPORTED_SCHEMA);
    let (path, now) = write_backup_of(conn, ledger, None, schema)?;
    let (schema_version, transaction_count, problems) = match Ledger::verify_file(&path) {
        Ok(r) => (r.schema_version, r.transaction_count, r.problems),
        Err(e) => (schema, 0, vec![e.to_string()]),
    };
    let problems = problems.into_iter().filter(|p| !p.starts_with(NEWER_PROBLEM)).collect();
    let info = BackupInfo {
        bytes: std::fs::metadata(&path)?.len(),
        path,
        schema_version,
        transaction_count,
        created_at: now.to_rfc3339(),
    };
    Ok((info, problems))
}

/// Check `backup`, upgrade a private copy of it and check that, take a safety copy of the ledger
/// open on `conn` (the file at `ledger`), then copy the upgraded backup over it in one step.
fn restore_into(conn: &mut Connection, ledger: &Path, backup: &Path) -> Result<RestoreInfo, LedgerError> {
    let refuse = |reason: String| LedgerError::RestoreRefused { reason };
    if let (Ok(a), Ok(b)) = (backup.canonicalize(), ledger.canonicalize()) {
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

    let staging = ledger_dir_of(ledger).join(format!(".ledger-restore-{}.tmp", std::process::id()));
    remove_staging(&staging);
    let result = prepare_staging(backup, &staging).and_then(|(st, ready)| {
        let (safety_backup, safety_problems) = safety_copy(conn, ledger)?;
        {
            let b = Backup::new(&st, conn)?;
            run_backup(&b)?;
        }
        Ok((safety_backup, safety_problems, ready))
    });
    remove_staging(&staging);
    let (safety_backup, safety_backup_problems, ready) = result?;
    Ok(RestoreInfo {
        restored_from: backup.to_path_buf(),
        safety_backup,
        safety_backup_problems,
        schema_version: ready.schema_version,
        transaction_count: ready.transaction_count,
    })
}

/// The start of the problem `verify_conn` reports for a file from a newer version.
const NEWER_PROBLEM: &str = "made by a newer Freehold Ledger";

fn run_backup(b: &Backup<'_, '_>) -> Result<(), LedgerError> {
    b.run_to_completion(PAGES_PER_STEP, Duration::from_millis(5), None)?;
    Ok(())
}

fn write_copy(src: &Connection, dest: &Path) -> Result<Connection, LedgerError> {
    let mut dst = Connection::open(dest)?;
    {
        let b = Backup::new(src, &mut dst)?;
        run_backup(&b)?;
    }
    // A backup is one self-contained file, not a write-ahead-log pair.
    dst.query_row("PRAGMA journal_mode=DELETE", [], |_| Ok(()))?;
    Ok(dst)
}

/// Copy `backup` to `staging`, bring the copy up to this app's format and check it. Returns the
/// open copy and its report. Nothing outside `staging` is written.
fn prepare_staging(backup: &Path, staging: &Path) -> Result<(Connection, VerifyReport), LedgerError> {
    let refuse = |reason: String| LedgerError::RestoreRefused { reason };
    let src = Connection::open_with_flags(backup, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|e| map_open(e, backup))?;
    src.busy_timeout(Duration::from_millis(5000))?;
    let mut st = write_copy(&src, staging)?;
    drop(src);
    st.execute_batch("PRAGMA foreign_keys=ON;")?;
    if let Err(e) = migrate(&mut st, backup) {
        return Err(refuse(format!("That file could not be brought up to this app's format ({e}).")));
    }
    let ready = verify_conn(&st, backup)?;
    if !ready.ok {
        return Err(refuse(format!("That file failed its check after upgrading: {}.", ready.problems.join("; "))));
    }
    Ok((st, ready))
}

fn remove_staging(staging: &Path) {
    let _ = std::fs::remove_file(staging);
    let mut journal = staging.as_os_str().to_owned();
    journal.push("-journal");
    let _ = std::fs::remove_file(PathBuf::from(journal));
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
                "{NEWER_PROBLEM} (schema {schema_version}; this app supports up to {SUPPORTED_SCHEMA})"
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
