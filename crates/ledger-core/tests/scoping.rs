mod common;

use chrono::NaiveDate;
use common::{empty, seeded};
use ledger_core::{
    BatchEntry, BusinessKey, Filter, Kind, Ledger, LedgerError, NewBatch, NewBusiness, NewEntry, NewExpense,
    NewIncome, Replacement,
};
use rusqlite::Connection;

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

fn shop(l: &mut Ledger) {
    l.create_business(NewBusiness { name: "Test shop".into(), note: None, is_test: true, make_current: false })
        .unwrap();
}

fn test_shop() -> BusinessKey {
    BusinessKey::Name("test shop".into())
}

fn exp(desc: &str) -> NewExpense {
    NewExpense {
        occurred_on: Some(d("2026-06-01")),
        amount_cents: 1999,
        description: desc.into(),
        category: "supplies".into(),
        is_asset: false,
        payment_method: None,
    }
}

fn inc(source: &str) -> NewIncome {
    NewIncome {
        occurred_on: Some(d("2026-06-02")),
        amount_cents: 5000,
        description: "job".into(),
        category: None,
        source: source.into(),
        payment_method: None,
    }
}

fn count(l: &Ledger, table: &str) -> i64 {
    Connection::open(l.path()).unwrap().query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0)).unwrap()
}

fn batch(desc: &str) -> NewBatch {
    NewBatch {
        label: "import".into(),
        entries: vec![BatchEntry {
            entry: NewEntry::Expense(exp(desc)),
            allow_duplicate: false,
            payment_method: None,
        }],
        payment_method: None,
    }
}

#[test]
fn s_scope_1_records_into_current_or_named_business() {
    let (_d, mut l) = seeded();
    shop(&mut l);
    let a = l.record_expense(exp("Plain")).unwrap();
    assert_eq!(a.business_id, 1);
    let b = l.record_expense_in(Some(&test_shop()), exp("Named")).unwrap();
    assert_eq!(b.business_id, 2);
    assert_eq!(l.get(b.id).unwrap().business_id, 2);
    assert_eq!(l.info().unwrap().current_business_id, Some(1));
}

#[test]
fn s_scope_2_archived_or_missing_current_writes_nothing() {
    let (_d, mut l) = seeded();
    shop(&mut l);
    l.archive_business(&test_shop()).unwrap();
    let before = count(&l, "transactions");
    match l.record_expense_in(Some(&test_shop()), exp("Nope")) {
        Err(LedgerError::ArchivedBusiness(n)) => assert_eq!(n, "Test shop"),
        other => panic!("{:?}", other.map(|t| t.id)),
    }
    assert_eq!(count(&l, "transactions"), before);
    l.archive_business(&BusinessKey::Id(1)).unwrap();
    assert!(matches!(l.record_income(inc("Upwork")), Err(LedgerError::NoCurrentBusiness)));
    assert_eq!(count(&l, "transactions"), before);
}

#[test]
fn s_scope_3_list_can_be_scoped() {
    let (_d, mut l) = empty();
    shop(&mut l);
    let a = l.record_expense(exp("Mine")).unwrap();
    let b = l.record_expense_in(Some(&test_shop()), exp("Theirs")).unwrap();
    l.void(b.id, "oops").unwrap();
    let only = l.list(&Filter { business_id: Some(2), include_voided: true, ..Default::default() }).unwrap();
    assert_eq!(only.iter().map(|t| t.id).collect::<Vec<_>>(), vec![b.id]);
    let all = l.list(&Filter { include_voided: true, ..Default::default() }).unwrap();
    assert!(all.iter().any(|t| t.id == a.id) && all.iter().any(|t| t.id == b.id));
    assert!(all.iter().all(|t| t.kind == Kind::Expense));
}

#[test]
fn s_scope_4_correct_stays_in_business_and_archived_blocks_it() {
    let (_d, mut l) = empty();
    shop(&mut l);
    let e = l.record_expense_in(Some(&test_shop()), exp("Shop buy")).unwrap();
    let c = l.correct(e.id, "fix", Replacement { amount_cents: Some(2500), ..Default::default() }).unwrap();
    assert_eq!(c.replacement.business_id, 2);
    let f = l.record_expense_in(Some(&test_shop()), exp("Other")).unwrap();
    let g = l.record_expense_in(Some(&test_shop()), exp("Third")).unwrap();
    l.archive_business(&test_shop()).unwrap();
    let before = count(&l, "transactions");
    match l.correct(f.id, "fix", Replacement::default()) {
        Err(LedgerError::ArchivedBusiness(n)) => assert_eq!(n, "Test shop"),
        other => panic!("{:?}", other.map(|c| c.replacement.id)),
    }
    assert_eq!(count(&l, "transactions"), before);
    assert!(l.reclassify(f.id, "office_expense", "better").is_ok());
    assert!(l.void(g.id, "gone").is_ok());
}

#[test]
fn s_scope_5_duplicate_check_stays_inside_the_business() {
    let (_d, mut l) = seeded();
    shop(&mut l);
    let first = l.record_expense(exp("Domain renewal")).unwrap();
    let r = l.record_batch_in(Some(&test_shop()), batch("Domain renewal")).unwrap();
    assert!(r.skipped_duplicates.is_empty());
    assert_eq!(r.recorded.len(), 1);
    let bid = r.batch_id.unwrap();
    let in_batch: i64 = Connection::open(l.path())
        .unwrap()
        .query_row("SELECT business_id FROM batch_businesses WHERE batch_id = ?1", [bid], |r| r.get(0))
        .unwrap();
    assert_eq!(in_batch, 2);
    assert_eq!(r.recorded[0].transaction.business_id, 2);
    assert_ne!(r.recorded[0].transaction.id, first.id);
    let again = l.record_batch_in(Some(&test_shop()), batch("Domain renewal")).unwrap();
    assert!(again.recorded.is_empty());
    assert_eq!(again.skipped_duplicates[0].matches_id, r.recorded[0].transaction.id);
}

#[test]
fn s_scope_6_income_sources_are_per_business() {
    let (_d, mut l) = empty();
    shop(&mut l);
    l.record_income(inc("Upwork")).unwrap();
    l.record_income_in(Some(&test_shop()), inc("Test buyer")).unwrap();
    let one = l.income_sources_in(1).unwrap();
    assert!(one.contains(&"Upwork".to_string()) && !one.contains(&"Test buyer".to_string()));
    assert_eq!(l.income_sources_in(2).unwrap(), vec!["Test buyer".to_string()]);
    l.record_income_in(Some(&test_shop()), inc("upwork")).unwrap();
    assert_eq!(l.income_sources_in(2).unwrap(), vec!["Test buyer".to_string(), "Upwork".to_string()]);
    assert_eq!(count(&l, "income_sources"), 2);
}
