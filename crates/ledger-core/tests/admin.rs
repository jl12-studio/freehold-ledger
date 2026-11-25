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

// Rollback after a format change: the newer version raised the file format, then the person went
// back to an older app. Ledger::open refuses that file, so the restore must work without it.

/// A ledger file as a newer version left it: `n` entries, schema raised past what this build knows.
fn newer_ledger(path: &std::path::Path, n: usize) {
    {
        let mut l = Ledger::open(path).unwrap();
        add(&mut l, n);
    }
    let c = rusqlite::Connection::open(path).unwrap();
    c.execute(
        "UPDATE schema_meta SET value = ?1 WHERE key = 'schema_version'",
        [(SUPPORTED_SCHEMA + 1).to_string()],
    )
    .unwrap();
}

fn descriptions(path: &std::path::Path) -> Vec<String> {
    let c = rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let mut s = c.prepare("SELECT description FROM transactions ORDER BY id").unwrap();
    let rows = s.query_map([], |r| r.get::<_, String>(0)).unwrap();
    rows.map(|r| r.unwrap()).collect()
}

#[test]
fn s_res_7_a_ledger_opened_by_a_newer_version_can_be_restored_without_opening_it() {
    let (dir, mut good) = common::empty();
    add(&mut good, 3);
    let backup = good.backup_to(None).unwrap();
    let live = dir.path().join("newer").join("ledger.sqlite3");
    std::fs::create_dir_all(live.parent().unwrap()).unwrap();
    newer_ledger(&live, 2);
    assert!(matches!(Ledger::open(&live), Err(LedgerError::NewerSchema { .. })));
    let before = descriptions(&live);
    assert_eq!(before.len(), 2);

    let r = Ledger::restore_file(&live, &backup.path).unwrap();

    assert_eq!(r.transaction_count, 3);
    assert_eq!(r.schema_version, SUPPORTED_SCHEMA);
    assert!(r.safety_backup.path.starts_with(live.parent().unwrap().join("backups")));
    assert_eq!(r.safety_backup.schema_version, SUPPORTED_SCHEMA + 1);
    assert_eq!(r.safety_backup.transaction_count, 2);
    assert!(
        r.safety_backup_problems.is_empty(),
        "being from a newer version is not damage: {:?}",
        r.safety_backup_problems
    );
    assert_eq!(descriptions(&r.safety_backup.path), before);
    let kept = rusqlite::Connection::open(&r.safety_backup.path).unwrap();
    let v: String = kept.query_row("SELECT value FROM schema_meta WHERE key='schema_version'", [], |r| r.get(0)).unwrap();
    assert_eq!(v, (SUPPORTED_SCHEMA + 1).to_string());
    drop(kept);

    let l = Ledger::open(&live).unwrap();
    let info = l.info().unwrap();
    assert_eq!((info.schema_version, info.transaction_count), (SUPPORTED_SCHEMA, 3));
    assert!(l.verify().unwrap().ok);
    assert!(staging_leftovers(live.parent().unwrap()).is_empty());
}

#[test]
fn s_res_8_restoring_a_newer_backup_into_a_newer_ledger_is_refused_and_changes_nothing() {
    let (dir, mut good) = common::empty();
    add(&mut good, 1);
    let newer_backup = newer_schema_file(&good, &dir.path().join("elsewhere"));
    let live = dir.path().join("newer").join("ledger.sqlite3");
    std::fs::create_dir_all(live.parent().unwrap()).unwrap();
    newer_ledger(&live, 2);
    let bytes = std::fs::read(&live).unwrap();

    let err = Ledger::restore_file(&live, &newer_backup).unwrap_err();

    assert!(matches!(err, LedgerError::RestoreRefused { .. }), "{err:?}");
    assert!(err.to_string().contains("newer Freehold Ledger") && err.to_string().ends_with("Nothing was changed."), "{err}");
    assert_eq!(std::fs::read(&live).unwrap(), bytes);
    assert!(!live.parent().unwrap().join("backups").exists());
    assert!(staging_leftovers(live.parent().unwrap()).is_empty());
}

#[test]
fn s_res_9_restore_file_refuses_a_missing_or_foreign_ledger_without_creating_or_changing_it() {
    let (dir, mut good) = common::empty();
    add(&mut good, 1);
    let backup = good.backup_to(None).unwrap();

    let missing = dir.path().join("none").join("ledger.sqlite3");
    assert!(Ledger::restore_file(&missing, &backup.path).is_err());
    assert!(!missing.exists());

    let text = dir.path().join("notes.sqlite3");
    std::fs::write(&text, "this is plain text pretending to be a ledger, nothing more to see here").unwrap();
    let err = Ledger::restore_file(&text, &backup.path).unwrap_err();
    assert!(matches!(err, LedgerError::NotALedger { .. }), "{err:?}");
    assert_eq!(std::fs::read(&text).unwrap(), b"this is plain text pretending to be a ledger, nothing more to see here");
}

#[test]
fn s_res_10_default_backup_folder_sits_beside_the_ledger_and_lists_without_opening_it() {
    let (dir, mut good) = common::empty();
    add(&mut good, 1);
    let backup = good.backup_to(None).unwrap();
    let live = dir.path().join("ledger.sqlite3");
    assert_eq!(Ledger::default_backup_folder(&live), dir.path().join("backups"));
    let found = Ledger::list_backups_in(&Ledger::default_backup_folder(&live)).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].path, backup.path);
    assert!(Ledger::list_backups_in(&dir.path().join("nowhere")).unwrap().is_empty());
}

// The rollback in the guides must only name tools that still work when a newer version opened the
// books: ledger_info, list_backups, verify_ledger (with a path) and restore_ledger.
#[test]
fn s_res_11_the_guides_rollback_names_only_tools_that_work_on_books_a_newer_version_opened() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let agent = std::fs::read_to_string(root.join("docs/AGENT-GUIDE.md")).unwrap();
    let human = std::fs::read_to_string(root.join("docs/HUMAN-GUIDE.md")).unwrap();
    let work = ["ledger_info", "list_backups", "verify_ledger", "restore_ledger"];

    let start = agent.find("7. If anything is wrong, go back:").expect("step 7 is the rollback");
    let end = agent[start..].find("### Staying on a version").expect("the section after the steps") + start;
    let rollback = &agent[start..end];
    let named: Vec<&str> = rollback.split('`').skip(1).step_by(2).filter(|t| t.chars().all(|c| c.is_ascii_lowercase() || c == '_')).collect();
    for tool in work {
        assert!(named.contains(&tool), "the rollback should name {tool}: {named:?}");
    }
    for t in &named {
        assert!(work.contains(t) || !t.contains('_'), "the rollback names {t}, which does not work on books a newer version opened");
    }
    assert!(rollback.contains("verify_ledger` with a `path`"), "verify_ledger only works with a path in that state");

    let updating = &human[human.find("## Updating").unwrap()..human.find("## Things to say to your agent").unwrap()];
    assert!(updating.contains("go back to the previous version") && updating.contains("backup"));
}
