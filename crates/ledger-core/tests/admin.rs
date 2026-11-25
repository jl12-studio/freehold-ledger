mod common;

use ledger_core::{Ledger, LedgerError, NewExpense, SUPPORTED_SCHEMA};

fn add(l: &mut Ledger, n: usize) {
    for i in 0..n {
        l.record_expense(NewExpense {
            occurred_on: None,
            amount_cents: 1000 + i as i64,
            description: format!("item {i}"),
            category: "supplies".into(),
            is_asset: false,
            payment_method: None,
        })
        .unwrap();
    }
}

#[test]
fn s_bak_1_backup_lands_in_backups_beside_the_ledger_and_verifies() {
    let (dir, mut l) = common::empty();
    add(&mut l, 3);
    let info = l.backup_to(None).unwrap();
    assert!(info.path.starts_with(dir.path().join("backups")));
    assert!(info.path.exists());
    assert_eq!(info.transaction_count, 3);
    assert_eq!(info.schema_version, SUPPORTED_SCHEMA);
    assert!(info.bytes > 0);
    let report = Ledger::verify_file(&info.path).unwrap();
    assert!(report.ok, "{:?}", report.problems);
    assert_eq!(report.transaction_count, 3);
    let name = info.path.file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.starts_with("ledger-") && name.ends_with("-schema4.sqlite3"), "{name}");
}

#[test]
fn s_bak_2_backup_while_another_connection_is_open_leaves_it_working() {
    let (_dir, mut l) = common::empty();
    add(&mut l, 3);
    let mut other = Ledger::open(l.path()).unwrap();
    let info = l.backup_to(None).unwrap();
    assert_eq!(info.transaction_count, 3);
    assert_eq!(other.info().unwrap().transaction_count, 3);
    add(&mut other, 1);
    assert_eq!(other.info().unwrap().transaction_count, 4);
    add(&mut l, 1);
    assert_eq!(l.info().unwrap().transaction_count, 5);
    assert_eq!(other.info().unwrap().transaction_count, 5);
}

#[test]
fn s_bak_3_two_backups_in_the_same_second_get_distinct_names() {
    let (_dir, l) = common::empty();
    let a = l.backup_to(None).unwrap();
    let b = l.backup_to(None).unwrap();
    assert_ne!(a.path, b.path);
    assert!(a.path.exists() && b.path.exists());
    assert_eq!(l.list_backups(None).unwrap().len(), 2);
}

#[test]
fn s_ver_1_healthy_ledger_verifies_clean() {
    let (_dir, mut l) = common::empty();
    add(&mut l, 2);
    let r = l.verify().unwrap();
    assert!(r.ok);
    assert!(r.problems.is_empty());
    assert_eq!((r.schema_version, r.supported_schema), (SUPPORTED_SCHEMA, SUPPORTED_SCHEMA));
    assert_eq!(r.transaction_count, 2);
    assert_eq!(r.business_count, 1);
}

#[test]
fn s_ver_2_missing_file_is_io_and_text_file_is_not_a_ledger() {
    let dir = tempfile::TempDir::new().unwrap();
    assert!(matches!(Ledger::verify_file(&dir.path().join("nope.sqlite3")), Err(LedgerError::Io(_))));
    assert!(!dir.path().join("nope.sqlite3").exists());
    let text = dir.path().join("notes.sqlite3");
    std::fs::write(&text, "just some words, not a database at all, long enough to look like a header").unwrap();
    assert!(matches!(Ledger::verify_file(&text), Err(LedgerError::NotALedger { .. })));
}

fn newer_schema_file(l: &Ledger, dir: &std::path::Path) -> std::path::PathBuf {
    let b = l.backup_to(Some(dir)).unwrap();
    let c = rusqlite::Connection::open(&b.path).unwrap();
    c.execute(
        "UPDATE schema_meta SET value = ?1 WHERE key = 'schema_version'",
        [(SUPPORTED_SCHEMA + 1).to_string()],
    )
    .unwrap();
    b.path
}

#[test]
fn s_ver_3_newer_schema_is_reported_and_restore_refuses_it() {
    let (dir, mut l) = common::empty();
    add(&mut l, 1);
    let path = newer_schema_file(&l, &dir.path().join("elsewhere"));
    let r = Ledger::verify_file(&path).unwrap();
    assert!(!r.ok);
    assert!(r.schema_version > r.supported_schema);
    assert!(r.problems.iter().any(|p| p.contains("made by a newer Freehold Ledger")), "{:?}", r.problems);
    assert!(matches!(l.restore_from(&path), Err(LedgerError::RestoreRefused { .. })));
    assert_eq!(l.info().unwrap().transaction_count, 1);
}

#[test]
fn s_res_1_restore_returns_to_the_backup_and_keeps_a_safety_copy() {
    let (_dir, mut l) = common::empty();
    add(&mut l, 3);
    let backup = l.backup_to(None).unwrap();
    add(&mut l, 2);
    let mut other = Ledger::open(l.path()).unwrap();
    assert_eq!(other.info().unwrap().transaction_count, 5);
    let r = l.restore_from(&backup.path).unwrap();
    assert_eq!(r.transaction_count, 3);
    assert_eq!(r.safety_backup.transaction_count, 5);
    assert!(r.safety_backup.path.exists());
    assert!(r.safety_backup_problems.is_empty(), "{:?}", r.safety_backup_problems);
    assert_eq!(l.info().unwrap().transaction_count, 3);
    assert_eq!(other.info().unwrap().transaction_count, 3);
    add(&mut other, 1);
    assert_eq!(l.info().unwrap().transaction_count, 4);
    assert!(l.verify().unwrap().ok);
}

#[test]
fn s_res_2_schema3_backup_is_upgraded_on_restore() {
    let (dir, mut l) = common::empty();
    add(&mut l, 1);
    let v3 = dir.path().join("old").join("v3.sqlite3");
    std::fs::create_dir_all(v3.parent().unwrap()).unwrap();
    common::v3_fixture(&v3, false);
    let r = l.restore_from(&v3).unwrap();
    assert_eq!(r.schema_version, SUPPORTED_SCHEMA);
    let info = l.info().unwrap();
    assert_eq!(info.schema_version, SUPPORTED_SCHEMA);
    assert_eq!(info.transaction_count, 3);
    assert_eq!(l.businesses(true).unwrap().len(), 1);
    assert!(l.verify().unwrap().ok);
}

#[test]
fn s_res_3_text_file_is_refused_with_no_safety_backup() {
    let (dir, mut l) = common::empty();
    add(&mut l, 2);
    let text = dir.path().join("notes.sqlite3");
    std::fs::write(&text, "this is plain text pretending to be a ledger backup, nothing more").unwrap();
    let err = l.restore_from(&text).unwrap_err();
    assert!(matches!(err, LedgerError::RestoreRefused { .. } | LedgerError::NotALedger { .. }), "{err:?}");
    assert!(!dir.path().join("backups").exists());
    assert_eq!(l.info().unwrap().transaction_count, 2);
}

#[test]
fn s_res_4_the_open_ledger_itself_is_refused() {
    let (dir, mut l) = common::empty();
    add(&mut l, 2);
    let own = l.path().to_path_buf();
    let err = l.restore_from(&own).unwrap_err();
    assert!(matches!(err, LedgerError::RestoreRefused { .. }), "{err:?}");
    assert!(err.to_string().ends_with("Nothing was changed."));
    assert!(!dir.path().join("backups").exists());
    assert_eq!(l.info().unwrap().transaction_count, 2);
}

fn staging_leftovers(dir: &std::path::Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(".ledger-restore-"))
        .collect()
}

// A file that passes its check but cannot be upgraded must not replace the books: the upgrade
// happens on a private copy before the live file is touched.
#[test]
fn s_res_5_a_backup_that_cannot_be_upgraded_leaves_the_books_as_they_were() {
    let (dir, mut l) = common::empty();
    add(&mut l, 2);
    let other = Ledger::open(l.path()).unwrap();
    let bad = dir.path().join("old").join("broken-v3.sqlite3");
    std::fs::create_dir_all(bad.parent().unwrap()).unwrap();
    rusqlite::Connection::open(&bad)
        .unwrap()
        .execute_batch(
            "CREATE TABLE schema_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO schema_meta (key, value) VALUES ('schema_version', '3');",
        )
        .unwrap();
    assert!(Ledger::verify_file(&bad).unwrap().ok);

    let err = l.restore_from(&bad).unwrap_err();
    assert!(matches!(err, LedgerError::RestoreRefused { .. }), "{err:?}");
    assert!(err.to_string().ends_with("Nothing was changed."), "{err}");
    assert_eq!(l.info().unwrap().transaction_count, 2);
    assert_eq!(l.info().unwrap().schema_version, SUPPORTED_SCHEMA);
    assert!(l.verify().unwrap().ok);
    assert_eq!(other.info().unwrap().transaction_count, 2);
    assert!(staging_leftovers(dir.path()).is_empty(), "{:?}", staging_leftovers(dir.path()));
}

// verify_ledger tells the agent to restore when the books have problems, so a restore must not
// be blocked by those problems. The broken books are kept as the safety backup.
#[test]
fn s_res_6_books_that_fail_their_check_can_still_be_restored_and_are_kept() {
    let (dir, mut l) = common::empty();
    add(&mut l, 2);
    let good = l.backup_to(Some(&dir.path().join("good"))).unwrap();
    add(&mut l, 1);
    let c = rusqlite::Connection::open(l.path()).unwrap();
    c.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
    c.execute("INSERT INTO transaction_businesses (transaction_id, business_id) VALUES (999, 1)", []).unwrap();
    drop(c);
    assert!(!l.verify().unwrap().ok);

    let r = l.restore_from(&good.path).unwrap();
    assert_eq!(r.transaction_count, 2);
    assert!(r.safety_backup.path.is_file());
    assert_eq!(r.safety_backup.transaction_count, 3);
    assert!(r.safety_backup_problems.iter().any(|p| p.contains("foreign key")), "{:?}", r.safety_backup_problems);
    assert_eq!(l.info().unwrap().transaction_count, 2);
    assert!(l.verify().unwrap().ok);
    assert!(staging_leftovers(dir.path()).is_empty());
}
