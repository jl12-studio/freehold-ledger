mod common;
use common::empty;
use ledger_core::{BusinessKey, LedgerError, NewBusiness};
use rusqlite::Connection;

fn nb(name: &str) -> NewBusiness {
    NewBusiness { name: name.into(), note: None, is_test: false, make_current: false }
}

fn rows(l: &ledger_core::Ledger, table: &str) -> i64 {
    Connection::open(l.path())
        .unwrap()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

#[test]
fn s_biz_1_create_and_name_rules() {
    let (_d, mut l) = empty();
    let b = l.create_business(NewBusiness { is_test: true, ..nb("Test shop") }).unwrap();
    assert_eq!(b.id, 2);
    assert!(b.is_test && !b.is_current && !b.archived);
    assert_eq!(l.info().unwrap().current_business_id, Some(1));
    match l.create_business(nb("test SHOP")) {
        Err(LedgerError::BusinessNameTaken(n)) => assert_eq!(n, "Test shop"),
        other => panic!("{:?}", other.map(|b| b.id)),
    }
    assert!(matches!(l.create_business(nb("  ")), Err(LedgerError::MissingBusinessName)));
    assert!(matches!(l.create_business(nb(&"x".repeat(61))), Err(LedgerError::BusinessNameTooLong)));
    let before = rows(&l, "businesses");
    assert!(matches!(
        l.create_business(nb("Acct 4242424242424242")),
        Err(LedgerError::CardNumberInText { .. })
    ));
    assert_eq!(rows(&l, "businesses"), before);
}

#[test]
fn s_biz_1b_create_makes_current_when_asked_or_when_none() {
    let (_d, mut l) = empty();
    let b = l.create_business(NewBusiness { make_current: true, ..nb("Second") }).unwrap();
    assert!(b.is_current);
    assert_eq!(l.info().unwrap().current_business_id, Some(b.id));
    l.archive_business(&BusinessKey::Id(b.id)).unwrap();
    assert!(l.current_business().unwrap().is_none());
    let c = l.create_business(nb("Third")).unwrap();
    assert!(c.is_current);
}

#[test]
fn s_biz_2_rename_rules() {
    let (_d, mut l) = empty();
    l.create_business(NewBusiness { is_test: true, ..nb("Test shop") }).unwrap();
    let b = l.rename_business(&BusinessKey::Id(2), "TEST SHOP").unwrap();
    assert_eq!(b.name, "TEST SHOP");
    let n = rows(&l, "business_changes");
    l.rename_business(&BusinessKey::Id(2), "TEST SHOP").unwrap();
    assert_eq!(rows(&l, "business_changes"), n);
    for name in ["My business", "MY BUSINESS", "my Business"] {
        match l.rename_business(&BusinessKey::Id(2), name) {
            Err(LedgerError::BusinessNameTaken(x)) => assert_eq!(x, "My business"),
            other => panic!("{:?}", other.map(|b| b.id)),
        }
    }
}

#[test]
fn s_biz_3_note_and_test_flag() {
    let (_d, mut l) = empty();
    l.create_business(nb("Test shop")).unwrap();
    let b = l.set_business_note(&BusinessKey::Id(2), Some("For trying")).unwrap();
    assert_eq!(b.note.as_deref(), Some("For trying"));
    let n = rows(&l, "business_changes");
    l.set_business_note(&BusinessKey::Id(2), Some(" For trying ")).unwrap();
    assert_eq!(rows(&l, "business_changes"), n);
    let b = l.set_business_note(&BusinessKey::Id(2), None).unwrap();
    assert_eq!(b.note, None);
    assert!(matches!(
        l.set_business_note(&BusinessKey::Id(2), Some(&"n".repeat(201))),
        Err(LedgerError::BusinessNoteTooLong)
    ));
    assert!(matches!(
        l.set_business_note(&BusinessKey::Id(2), Some("card 4242424242424242")),
        Err(LedgerError::CardNumberInText { .. })
    ));
    let b = l.set_business_test(&BusinessKey::Id(1), true).unwrap();
    assert!(b.is_test);
    let n = rows(&l, "business_changes");
    l.set_business_test(&BusinessKey::Id(1), true).unwrap();
    assert_eq!(rows(&l, "business_changes"), n);
    assert!(!l.set_business_test(&BusinessKey::Id(1), false).unwrap().is_test);
}

#[test]
fn s_biz_4_current_and_archive() {
    let (_d, mut l) = empty();
    l.create_business(nb("Test shop")).unwrap();
    let b = l.set_current_business(&BusinessKey::Name("test shop".into())).unwrap();
    assert_eq!(b.id, 2);
    assert!(b.is_current);
    assert_eq!(l.current_business().unwrap().unwrap().id, 2);
    let n = rows(&l, "current_business_changes");
    l.set_current_business(&BusinessKey::Id(2)).unwrap();
    assert_eq!(rows(&l, "current_business_changes"), n);
    let a = l.archive_business(&BusinessKey::Id(2)).unwrap();
    assert!(a.archived && a.archived_at.is_some() && !a.is_current);
    assert!(l.current_business().unwrap().is_none());
    assert!(l.info().unwrap().current_business_id.is_none());
    assert!(matches!(l.archive_business(&BusinessKey::Id(2)), Err(LedgerError::AlreadyArchived(_))));
    assert!(matches!(l.set_current_business(&BusinessKey::Id(2)), Err(LedgerError::ArchivedBusiness(_))));
}

#[test]
fn s_biz_5_restore_and_name_resolution() {
    let (_d, mut l) = empty();
    l.create_business(nb("Test shop")).unwrap();
    l.archive_business(&BusinessKey::Id(2)).unwrap();
    let new = l.create_business(nb("Test shop")).unwrap();
    assert_eq!(new.id, 3);
    assert!(matches!(
        l.restore_business(&BusinessKey::Id(2)),
        Err(LedgerError::BusinessNameTaken(_))
    ));
    assert!(matches!(l.restore_business(&BusinessKey::Id(3)), Err(LedgerError::NotArchived(_))));
    assert_eq!(l.find_business(&BusinessKey::Name("Test shop".into())).unwrap().id, 3);
    l.archive_business(&BusinessKey::Id(3)).unwrap();
    match l.find_business(&BusinessKey::Name("Test shop".into())) {
        Err(LedgerError::AmbiguousBusiness { ids, .. }) => {
            assert!(ids.contains('2') && ids.contains('3'), "{ids}")
        }
        other => panic!("{:?}", other.map(|b| b.id)),
    }
    assert!(matches!(l.find_business(&BusinessKey::Name("Nope".into())), Err(LedgerError::UnknownBusiness(_))));
    assert!(matches!(l.find_business(&BusinessKey::Id(99)), Err(LedgerError::UnknownBusiness(_))));
    // restoring needs a free name; a restored business does not become current
    l.restore_business(&BusinessKey::Id(3)).unwrap();
    assert!(matches!(l.restore_business(&BusinessKey::Id(2)), Err(LedgerError::BusinessNameTaken(_))));
    l.rename_business(&BusinessKey::Id(3), "Renamed").unwrap();
    let r = l.restore_business(&BusinessKey::Id(2)).unwrap();
    assert!(!r.archived && !r.is_current);
}

#[test]
fn s_biz_6_listing_order() {
    let (_d, mut l) = empty();
    l.create_business(nb("zeta")).unwrap();
    l.create_business(nb("Alpha")).unwrap();
    l.create_business(nb("old")).unwrap();
    l.archive_business(&BusinessKey::Name("old".into())).unwrap();
    let names = |v: Vec<ledger_core::Business>| v.into_iter().map(|b| b.name).collect::<Vec<_>>();
    assert_eq!(names(l.businesses(false).unwrap()), ["My business", "Alpha", "zeta"]);
    let all = l.businesses(true).unwrap();
    assert_eq!(all.last().unwrap().name, "old");
    assert!(all.iter().all(|b| !b.created_at.is_empty()));
    assert!(all.last().unwrap().archived_at.is_some());
    l.set_current_business(&BusinessKey::Name("zeta".into())).unwrap();
    assert_eq!(names(l.businesses(false).unwrap()), ["zeta", "Alpha", "My business"]);
}
