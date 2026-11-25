mod common;

use common::{v3_fixture, INSERT_TX};
use ledger_core::{resolve_db_path, Ledger, LedgerError};
use rusqlite::Connection;
use std::path::Path;
use tempfile::TempDir;

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
    assert_eq!(info.schema_version, 4);
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
            assert_eq!((found, supported), (99, 4));
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
fn s_imm_2_insert_or_replace_cannot_rewrite_append_only_rows() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    let mut l = Ledger::open(&path).unwrap();
    let t = l
        .record_expense(ledger_core::NewExpense {
            occurred_on: None,
            amount_cents: 3000,
            description: "Device".into(),
            category: "supplies".into(),
            is_asset: false,
            payment_method: None,
        })
        .unwrap();
    l.reclassify(t.id, "office_expense", "desk lamp").unwrap();
    l.void(t.id, "duplicate").unwrap();
    let c = raw(&path);
    for (table, sql) in [
        (
            "transactions",
            "INSERT OR REPLACE INTO transactions (id, kind, occurred_on, amount_cents, description, category, recorded_at) \
             VALUES (1, 'expense', '2026-01-02', 999999, 'rewritten', 'supplies', 'x')",
        ),
        ("voids", "REPLACE INTO voids (transaction_id, reason, recorded_at) VALUES (1, 'rewritten', 'x')"),
        ("voids", "INSERT OR REPLACE INTO voids (id, transaction_id, reason, recorded_at) VALUES (1, 1, 'rewritten', 'x')"),
        (
            "classification_changes",
            "INSERT OR REPLACE INTO classification_changes (id, transaction_id, category, reason, recorded_at) \
             VALUES (1, 1, 'supplies', 'rewritten', 'x')",
        ),
    ] {
        let e = c.execute(sql, []).unwrap_err();
        assert!(e.to_string().contains(&format!("{table} are append-only")), "{sql}: {e}");
    }
    let after = l.get(t.id).unwrap();
    assert_eq!(after.amount_cents, 3000);
    assert_eq!(after.description, "Device");
    assert_eq!(after.void.unwrap().reason, "duplicate");
    assert_eq!(after.classification_history[0].reason, "desk lamp");
}

#[test]
fn s_file_5_schema_1_file_upgrades() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    {
        let c = raw(&path);
        c.execute_batch(include_str!("../migrations/0001_init.sql")).unwrap();
        c.execute("INSERT INTO schema_meta (key, value) VALUES ('schema_version','1')", []).unwrap();
        c.execute(INSERT_TX, []).unwrap();
    }
    let mut l = Ledger::open(&path).unwrap();
    let info = l.info().unwrap();
    assert_eq!(info.schema_version, 4);
    assert_eq!(info.transaction_count, 1);
    let bs = l.businesses(true).unwrap();
    assert_eq!(bs.len(), 1);
    assert_eq!((bs[0].id, bs[0].name.as_str(), bs[0].transaction_count), (1, "My business", 1));
    assert_eq!(info.current_business_id, Some(1));
    let r = l
        .record_batch(ledger_core::NewBatch {
            label: "after upgrade".into(),
            payment_method: None,
            entries: vec![ledger_core::BatchEntry {
                entry: ledger_core::NewEntry::Expense(ledger_core::NewExpense {
                    occurred_on: None,
                    amount_cents: 500,
                    description: "pens".into(),
                    category: "supplies".into(),
                    is_asset: false,
                    payment_method: None,
                }),
                allow_duplicate: false,
                payment_method: None,
            }],
        })
        .unwrap();
    assert_eq!(r.recorded.len(), 1);
    assert_eq!(l.info().unwrap().transaction_count, 2);
}

#[test]
fn s_imm_3_batch_tables_are_append_only() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    drop(Ledger::open(&path).unwrap());
    let c = raw(&path);
    c.execute(INSERT_TX, []).unwrap();
    c.execute("INSERT INTO batches (label, recorded_at) VALUES ('f.csv', '2026-01-03T00:00:00Z')", []).unwrap();
    c.execute("INSERT INTO transaction_batches (transaction_id, batch_id) VALUES (1, 1)", []).unwrap();
    for (table, set, replace) in [
        ("batches", "label='x'", "INSERT OR REPLACE INTO batches (id, label, recorded_at) VALUES (1, 'x', 'x')"),
        (
            "transaction_batches",
            "batch_id=2",
            "INSERT OR REPLACE INTO transaction_batches (transaction_id, batch_id) VALUES (1, 1)",
        ),
    ] {
        let expected = format!("{table} are append-only");
        let e = c.execute(&format!("UPDATE {table} SET {set}"), []).unwrap_err();
        assert!(e.to_string().contains(&expected), "{e}");
        let e = c.execute(&format!("DELETE FROM {table}"), []).unwrap_err();
        assert!(e.to_string().contains(&expected), "{e}");
        let e = c.execute(replace, []).unwrap_err();
        assert!(e.to_string().contains(&expected), "{e}");
    }
}

#[test]
fn s_file_6_schema_2_file_upgrades() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    {
        let c = raw(&path);
        c.execute_batch(include_str!("../migrations/0001_init.sql")).unwrap();
        c.execute_batch(include_str!("../migrations/0002_batches.sql")).unwrap();
        c.execute("INSERT INTO schema_meta (key, value) VALUES ('schema_version','2')", []).unwrap();
        c.execute(INSERT_TX, []).unwrap();
    }
    let mut l = Ledger::open(&path).unwrap();
    let info = l.info().unwrap();
    assert_eq!(info.schema_version, 4);
    assert_eq!(info.transaction_count, 1);
    let bs = l.businesses(true).unwrap();
    assert_eq!(bs.len(), 1);
    assert_eq!((bs[0].id, bs[0].name.as_str(), bs[0].transaction_count), (1, "My business", 1));
    assert_eq!(info.current_business_id, Some(1));
    let p = l
        .create_payment_method(ledger_core::NewPaymentMethod {
            kind: ledger_core::PaymentKind::Cash,
            nickname: "Cash".into(),
            last4: None,
            make_default: false,
        })
        .unwrap();
    assert_eq!(p.id, 1);
}

#[test]
fn s_imm_4_payment_method_tables_are_append_only() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    drop(Ledger::open(&path).unwrap());
    let c = raw(&path);
    c.execute(INSERT_TX, []).unwrap();
    c.execute("INSERT INTO payment_methods (kind, nickname, last4, created_at) VALUES ('card','Visa','4242','x')", []).unwrap();
    c.execute("INSERT INTO payment_method_changes (payment_method_id, action, nickname, recorded_at) VALUES (1,'rename','Biz','x')", []).unwrap();
    c.execute("INSERT INTO default_payment_method_changes (payment_method_id, recorded_at) VALUES (1,'x')", []).unwrap();
    c.execute("INSERT INTO payment_method_assignments (transaction_id, payment_method_id, recorded_at) VALUES (1,1,'x')", []).unwrap();
    for (table, set, replace) in [
        (
            "payment_methods",
            "nickname='x'",
            "INSERT OR REPLACE INTO payment_methods (id, kind, nickname, created_at) VALUES (1,'cash','x','x')",
        ),
        (
            "payment_method_changes",
            "nickname='x'",
            "INSERT OR REPLACE INTO payment_method_changes (id, payment_method_id, action, nickname, recorded_at) VALUES (1,1,'rename','x','x')",
        ),
        (
            "default_payment_method_changes",
            "payment_method_id=NULL",
            "INSERT OR REPLACE INTO default_payment_method_changes (id, payment_method_id, recorded_at) VALUES (1,NULL,'x')",
        ),
        (
            "payment_method_assignments",
            "payment_method_id=2",
            "INSERT OR REPLACE INTO payment_method_assignments (id, transaction_id, payment_method_id, recorded_at) VALUES (1,1,1,'x')",
        ),
    ] {
        let expected = format!("{table} are append-only");
        let e = c.execute(&format!("UPDATE {table} SET {set}"), []).unwrap_err();
        assert!(e.to_string().contains(&expected), "{e}");
        let e = c.execute(&format!("DELETE FROM {table}"), []).unwrap_err();
        assert!(e.to_string().contains(&expected), "{e}");
        let e = c.execute(replace, []).unwrap_err();
        assert!(e.to_string().contains(&expected), "{e}");
    }
    let e = c
        .execute("INSERT INTO payment_methods (kind, nickname, last4, created_at) VALUES ('card','Y','42424','x')", [])
        .unwrap_err();
    assert!(e.to_string().contains("CHECK"), "{e}");
    let e = c
        .execute("INSERT INTO payment_methods (kind, nickname, created_at) VALUES ('card','Visa 1234567890123','x')", [])
        .unwrap_err();
    assert!(e.to_string().contains("CHECK"), "{e}");
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

fn count(c: &Connection, sql: &str) -> i64 {
    c.query_row(sql, [], |r| r.get(0)).unwrap()
}

#[test]
fn s_mig_1_v3_file_moves_into_my_business() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    v3_fixture(&path, false);
    let l = Ledger::open(&path).unwrap();
    let info = l.info().unwrap();
    assert_eq!(info.schema_version, 4);
    assert_eq!(info.transaction_count, 3);
    assert_eq!(info.current_business_id, Some(1));
    let bs = l.businesses(true).unwrap();
    assert_eq!(bs.len(), 1);
    let b = &bs[0];
    assert_eq!((b.id, b.name.as_str()), (1, "My business"));
    assert!(b.is_current && !b.is_test && !b.archived);
    assert_eq!(b.transaction_count, 3);
    let c = raw(&path);
    assert_eq!(count(&c, "SELECT count(*) FROM transaction_businesses WHERE business_id = 1"), 3);
    assert_eq!(count(&c, "SELECT count(*) FROM transaction_businesses"), 3);
    assert_eq!(count(&c, "SELECT count(*) FROM batch_businesses WHERE batch_id = 1 AND business_id = 1"), 1);
    assert_eq!(count(&c, "SELECT count(*) FROM payment_method_businesses WHERE business_id = 1"), 2);
    assert_eq!(count(&c, "SELECT count(*) FROM business_default_payment_method_changes"), 1);
    assert_eq!(
        count(&c, "SELECT count(*) FROM business_default_payment_method_changes WHERE business_id = 1 AND payment_method_id = 2"),
        1
    );
    assert_eq!(count(&c, "SELECT count(*) FROM voids WHERE transaction_id = 2"), 1);
    assert_eq!(count(&c, "SELECT replaces_id FROM transactions WHERE id = 3"), 2);
    assert_eq!(
        count(&c, "SELECT count(*) FROM payment_method_assignments WHERE transaction_id = 1 AND payment_method_id = 1"),
        1
    );
    assert_eq!(count(&c, "SELECT count(*) FROM default_payment_method_changes"), 2);
}

#[test]
fn s_mig_2_v3_null_default_stays_none() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    v3_fixture(&path, true);
    drop(Ledger::open(&path).unwrap());
    let c = raw(&path);
    assert_eq!(count(&c, "SELECT count(*) FROM business_default_payment_method_changes"), 1);
    assert_eq!(
        count(&c, "SELECT count(*) FROM business_default_payment_method_changes WHERE business_id = 1 AND payment_method_id IS NULL"),
        1
    );
}

#[test]
fn s_mig_3_fresh_file_has_my_business_current() {
    let dir = TempDir::new().unwrap();
    let l = Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();
    let bs = l.businesses(true).unwrap();
    assert_eq!(bs.len(), 1);
    assert_eq!((bs[0].id, bs[0].name.as_str()), (1, "My business"));
    assert_eq!(l.info().unwrap().current_business_id, Some(1));
}

#[test]
fn s_imm_5_business_tables_are_append_only() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    drop(Ledger::open(&path).unwrap());
    let c = raw(&path);
    c.execute(INSERT_TX, []).unwrap();
    c.execute("INSERT INTO batches (label, recorded_at) VALUES ('f.csv', 'x')", []).unwrap();
    c.execute("INSERT INTO payment_methods (kind, nickname, created_at) VALUES ('cash','Cash','x')", []).unwrap();
    c.execute("INSERT INTO business_changes (business_id, action, name, recorded_at) VALUES (1,'rename','Biz','x')", []).unwrap();
    c.execute("INSERT INTO transaction_businesses (transaction_id, business_id) VALUES (1,1)", []).unwrap();
    c.execute("INSERT INTO batch_businesses (batch_id, business_id) VALUES (1,1)", []).unwrap();
    c.execute("INSERT INTO payment_method_businesses (payment_method_id, business_id) VALUES (1,1)", []).unwrap();
    for (table, set, replace) in [
        (
            "businesses",
            "name='x'",
            "INSERT OR REPLACE INTO businesses (id, name, is_test, created_at) VALUES (1,'x',0,'x')",
        ),
        (
            "business_changes",
            "name='x'",
            "INSERT OR REPLACE INTO business_changes (id, business_id, action, name, recorded_at) VALUES (1,1,'rename','x','x')",
        ),
        (
            "current_business_changes",
            "business_id=NULL",
            "INSERT OR REPLACE INTO current_business_changes (id, business_id, recorded_at) VALUES (1,NULL,'x')",
        ),
        (
            "transaction_businesses",
            "business_id=2",
            "INSERT OR REPLACE INTO transaction_businesses (transaction_id, business_id) VALUES (1,1)",
        ),
        (
            "batch_businesses",
            "business_id=2",
            "INSERT OR REPLACE INTO batch_businesses (batch_id, business_id) VALUES (1,1)",
        ),
        (
            "payment_method_businesses",
            "business_id=2",
            "INSERT OR REPLACE INTO payment_method_businesses (payment_method_id, business_id) VALUES (1,1)",
        ),
        (
            "business_default_payment_method_changes",
            "payment_method_id=NULL",
            "INSERT OR REPLACE INTO business_default_payment_method_changes (id, business_id, payment_method_id, recorded_at) VALUES (1,1,NULL,'x')",
        ),
    ] {
        if table == "business_default_payment_method_changes" {
            c.execute("INSERT INTO business_default_payment_method_changes (business_id, payment_method_id, recorded_at) VALUES (1,1,'x')", []).unwrap();
        }
        let expected = format!("{table} are append-only");
        let e = c.execute(&format!("UPDATE {table} SET {set}"), []).unwrap_err();
        assert!(e.to_string().contains(&expected), "{e}");
        let e = c.execute(&format!("DELETE FROM {table}"), []).unwrap_err();
        assert!(e.to_string().contains(&expected), "{e}");
        let e = c.execute(replace, []).unwrap_err();
        assert!(e.to_string().contains(&expected), "{e}");
    }
    let e = c
        .execute("INSERT INTO businesses (name, created_at) VALUES ('Shop 1234567890123','x')", [])
        .unwrap_err();
    assert!(e.to_string().contains("CHECK"), "{e}");
}
