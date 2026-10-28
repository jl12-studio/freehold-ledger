use ledger_core::{resolve_db_path, Ledger, LedgerError};
use rusqlite::Connection;
use std::path::Path;
use tempfile::TempDir;

const INSERT_TX: &str = "INSERT INTO transactions (kind, occurred_on, amount_cents, description, category, recorded_at) \
     VALUES ('expense', '2026-01-02', 30000, 'device', 'supplies', '2026-01-02T00:00:00Z')";

fn raw(path: &Path) -> Connection {
    Connection::open(path).unwrap()
}

#[test]
fn s_file_1_fresh_open_creates_owned_ledger() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("nested").join("ledger.sqlite3");
    let ledger = Ledger::open(&path).unwrap();
    assert!(path.exists());
    let info = ledger.info().unwrap();
    assert_eq!(info.schema_version, 1);
    assert_eq!(info.transaction_count, 0);
    assert_eq!(ledger.path(), path.as_path());
}

#[test]
fn s_file_2_reopen_keeps_data() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    {
        let _l = Ledger::open(&path).unwrap();
        raw(&path).execute(INSERT_TX, []).unwrap();
    }
    let l = Ledger::open(&path).unwrap();
    assert_eq!(l.info().unwrap().transaction_count, 1);
}

#[test]
fn s_file_3_garbage_file_is_not_a_ledger_and_untouched() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    let bytes = vec![0xABu8; 64];
    std::fs::write(&path, &bytes).unwrap();
    match Ledger::open(&path) {
        Err(LedgerError::NotALedger { path: p }) => assert_eq!(p, path),
        other => panic!("expected NotALedger, got {:?}", other.map(|_| ())),
    }
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert!(!dir.path().join("ledger.sqlite3-wal").exists());
    assert!(!dir.path().join("ledger.sqlite3-shm").exists());
}

#[test]
fn s_file_3b_foreign_sqlite_db_is_not_a_ledger() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("other.sqlite3");
    raw(&path)
        .execute_batch("CREATE TABLE notes (id INTEGER PRIMARY KEY, body TEXT);")
        .unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(matches!(Ledger::open(&path), Err(LedgerError::NotALedger { .. })));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(!dir.path().join("other.sqlite3-wal").exists());
}

#[test]
fn s_file_4_newer_schema_refused_and_untouched() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    drop(Ledger::open(&path).unwrap());
    raw(&path)
        .execute("UPDATE schema_meta SET value='99' WHERE key='schema_version'", [])
        .unwrap();
    let before = std::fs::read(&path).unwrap();
    match Ledger::open(&path) {
        Err(LedgerError::NewerSchema { found, supported }) => {
            assert_eq!((found, supported), (99, 1));
        }
        other => panic!("expected NewerSchema, got {:?}", other.map(|_| ())),
    }
    let v: String = raw(&path)
        .query_row("SELECT value FROM schema_meta WHERE key='schema_version'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, "99");
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn s_imm_1_transactions_are_append_only() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    drop(Ledger::open(&path).unwrap());
    let c = raw(&path);
    c.execute(INSERT_TX, []).unwrap();
    c.execute(
        "INSERT INTO voids (transaction_id, reason, recorded_at) VALUES (1, 'oops', '2026-01-03T00:00:00Z')",
        [],
    )
    .unwrap();
    c.execute(
        "INSERT INTO classification_changes (transaction_id, category, reason, recorded_at) \
         VALUES (1, 'office_expense', 'miscategorized', '2026-01-03T00:00:00Z')",
        [],
    )
    .unwrap();
    for (table, set) in [
        ("transactions", "description='x'"),
        ("voids", "reason='x'"),
        ("classification_changes", "reason='x'"),
    ] {
        let expected = format!("{table} are append-only");
        let e = c.execute(&format!("UPDATE {table} SET {set}"), []).unwrap_err();
        assert!(e.to_string().contains(&expected), "{e}");
        let e = c.execute(&format!("DELETE FROM {table}"), []).unwrap_err();
        assert!(e.to_string().contains(&expected), "{e}");
    }
}

#[test]
fn resolve_db_path_honors_env() {
    // The only test in this binary that touches the env var.
    let prev = std::env::var_os("FREEHOLD_LEDGER_DB");
    std::env::set_var("FREEHOLD_LEDGER_DB", "/tmp/somewhere/else.sqlite3");
    assert_eq!(resolve_db_path(), Path::new("/tmp/somewhere/else.sqlite3"));
    std::env::set_var("FREEHOLD_LEDGER_DB", "");
    assert_eq!(resolve_db_path(), ledger_core::default_db_path());
    match prev {
        Some(v) => std::env::set_var("FREEHOLD_LEDGER_DB", v),
        None => std::env::remove_var("FREEHOLD_LEDGER_DB"),
    }
}

#[test]
fn two_connections_can_write() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    let a = Ledger::open(&path).unwrap();
    let b = Ledger::open(&path).unwrap();
    raw_write(&a);
    raw_write(&b);
    assert_eq!(a.info().unwrap().transaction_count, 2);
}

fn raw_write(l: &Ledger) {
    // Same connection configuration as production, via a fresh handle on the same file.
    let c = raw(l.path());
    c.busy_timeout(std::time::Duration::from_millis(5000)).unwrap();
    c.execute(INSERT_TX, []).unwrap();
}
