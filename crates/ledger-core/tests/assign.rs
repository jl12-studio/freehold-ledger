mod common;

use ledger_core::{
    Filter, Ledger, LedgerError, NewExpense, NewIncome, NewPaymentMethod, PaymentKind, PaymentMethodFilter,
    PaymentMethodKey, Replacement,
};
use rusqlite::Connection;

fn nick(s: &str) -> PaymentMethodKey {
    PaymentMethodKey::Nickname(s.into())
}

fn pm(kind: PaymentKind, nickname: &str, last4: Option<&str>, make_default: bool) -> NewPaymentMethod {
    NewPaymentMethod { kind, nickname: nickname.into(), last4: last4.map(String::from), make_default }
}

/// Seeded ledger plus Business Visa (default, id 1), Checking (id 2), Cash (id 3).
fn setup() -> (tempfile::TempDir, Ledger) {
    let (d, mut l) = common::seeded();
    l.create_payment_method(pm(PaymentKind::Card, "Business Visa", Some("4242"), true)).unwrap();
    l.create_payment_method(pm(PaymentKind::BankAccount, "Checking", Some("6789"), false)).unwrap();
    l.create_payment_method(pm(PaymentKind::Cash, "Cash", None, false)).unwrap();
    (d, l)
}

fn raw_count(l: &Ledger, sql: &str) -> i64 {
    Connection::open(l.path()).unwrap().query_row(sql, [], |r| r.get(0)).unwrap()
}

fn expense(pmk: Option<PaymentMethodKey>) -> NewExpense {
    NewExpense {
        occurred_on: None,
        amount_cents: 1500,
        description: "Stamps".into(),
        category: "office_expense".into(),
        is_asset: false,
        payment_method: pmk,
    }
}

fn income(pmk: Option<PaymentMethodKey>) -> NewIncome {
    NewIncome {
        occurred_on: None,
        amount_cents: 9000,
        description: "Logo".into(),
        category: None,
        source: "Fiverr".into(),
        payment_method: pmk,
    }
}

fn ids(v: &[ledger_core::Transaction]) -> Vec<i64> {
    v.iter().map(|t| t.id).collect()
}

fn name_of(t: &ledger_core::Transaction) -> Option<&str> {
    t.payment_method.as_ref().map(|p| p.nickname.as_str())
}

#[test]
fn s_asg_1_existing_entries_start_unassigned() {
    let (_d, l) = setup();
    for id in 1..=10 {
        assert!(l.get(id).unwrap().payment_method.is_none(), "transaction {id}");
    }
    let all = l
        .list(&Filter { payment_method: Some(PaymentMethodFilter::Unassigned), include_voided: true, ..Default::default() })
        .unwrap();
    assert_eq!(all.len(), 10);
}

#[test]
fn s_asg_2_individual_assign() {
    let (_d, mut l) = setup();
    let a = l.assign_payment_method(&nick("business visa"), &[1]).unwrap();
    assert_eq!(ids(&a.assigned), vec![1]);
    assert!(a.unchanged_ids.is_empty());
    let pm = l.get(1).unwrap().payment_method.unwrap();
    assert_eq!(pm.nickname, "Business Visa");
    assert_eq!(pm.kind, PaymentKind::Card);
    assert_eq!(pm.last4.as_deref(), Some("4242"));
}

#[test]
fn s_asg_3_mass_assign_is_idempotent() {
    let (_d, mut l) = setup();
    let a = l.assign_payment_method(&nick("Checking"), &[2, 3, 2]).unwrap();
    assert_eq!(ids(&a.assigned), vec![2, 3]);
    let before = raw_count(&l, "SELECT count(*) FROM payment_method_assignments");
    let b = l.assign_payment_method(&nick("Checking"), &[2, 3]).unwrap();
    assert!(b.assigned.is_empty());
    assert_eq!(b.unchanged_ids, vec![2, 3]);
    assert_eq!(raw_count(&l, "SELECT count(*) FROM payment_method_assignments"), before);
}

#[test]
fn s_asg_4_reassign_keeps_history() {
    let (_d, mut l) = setup();
    l.assign_payment_method(&nick("Business Visa"), &[1]).unwrap();
    l.assign_payment_method(&nick("Cash"), &[1]).unwrap();
    assert_eq!(name_of(&l.get(1).unwrap()), Some("Cash"));
    let rows = raw_count(&l, "SELECT count(*) FROM payment_method_assignments WHERE transaction_id = 1");
    assert_eq!(rows, 2);
    let first: i64 = raw_count(
        &l,
        "SELECT payment_method_id FROM payment_method_assignments WHERE transaction_id = 1 ORDER BY id LIMIT 1",
    );
    assert_eq!(first, 1);
}

#[test]
fn s_asg_5_all_or_nothing() {
    let (_d, mut l) = setup();
    let e = l.assign_payment_method(&nick("Cash"), &[4, 6]).unwrap_err();
    assert!(matches!(e, LedgerError::AssignVoided(6)), "{e:?}");
    assert!(l.get(4).unwrap().payment_method.is_none());

    let e = l.assign_payment_method(&nick("Cash"), &[4, 999]).unwrap_err();
    assert!(matches!(e, LedgerError::NotFound(999)), "{e:?}");
    assert!(l.get(4).unwrap().payment_method.is_none());

    let e = l.assign_payment_method(&nick("Cash"), &[]).unwrap_err();
    assert!(matches!(e, LedgerError::NoIdsGiven), "{e:?}");

    let many: Vec<i64> = (1..=201).collect();
    let e = l.assign_payment_method(&nick("Cash"), &many).unwrap_err();
    assert!(matches!(e, LedgerError::TooManyIds { count: 201 }), "{e:?}");

    l.retire_payment_method(&nick("Cash")).unwrap();
    let e = l.assign_payment_method(&nick("Cash"), &[4]).unwrap_err();
    assert!(matches!(e, LedgerError::RetiredPaymentMethod(_)), "{e:?}");
    assert_eq!(raw_count(&l, "SELECT count(*) FROM payment_method_assignments"), 0);
}

#[test]
fn s_asg_6_filter_by_payment_method() {
    let (_d, mut l) = setup();
    l.assign_payment_method(&nick("Business Visa"), &[1]).unwrap();
    l.assign_payment_method(&nick("Checking"), &[2, 3]).unwrap();
    let f = |pm| Filter { payment_method: Some(pm), ..Default::default() };
    assert_eq!(ids(&l.list(&f(PaymentMethodFilter::Id(1))).unwrap()), vec![1]);
    assert_eq!(ids(&l.list(&f(PaymentMethodFilter::Id(2))).unwrap()), vec![2, 3]);
    assert_eq!(ids(&l.list(&f(PaymentMethodFilter::Unassigned)).unwrap()), vec![10, 4, 5, 8, 9]);
}

#[test]
fn s_pay_1_default_on_a_new_expense() {
    let (_d, mut l) = setup();
    assert_eq!(name_of(&l.record_expense(expense(None)).unwrap()), Some("Business Visa"));
    assert_eq!(name_of(&l.record_expense(expense(Some(nick("cash")))).unwrap()), Some("Cash"));
    l.set_default_payment_method(None).unwrap();
    assert_eq!(name_of(&l.record_expense(expense(None)).unwrap()), None);
}

#[test]
fn s_pay_2_income_never_takes_the_default() {
    let (_d, mut l) = setup();
    assert_eq!(name_of(&l.record_income(income(None)).unwrap()), None);
    assert_eq!(name_of(&l.record_income(income(Some(nick("Checking")))).unwrap()), Some("Checking"));
}

#[test]
fn s_pay_3_refusals_write_nothing() {
    let (_d, mut l) = setup();
    l.retire_payment_method(&nick("Cash")).unwrap();
    let before = l.info().unwrap().transaction_count;
    let e = l.record_expense(expense(Some(nick("Cash")))).unwrap_err();
    assert!(matches!(e, LedgerError::RetiredPaymentMethod(_)), "{e:?}");
    let e = l.record_expense(expense(Some(nick("Amex")))).unwrap_err();
    assert!(matches!(e, LedgerError::UnknownPaymentMethod(_)), "{e:?}");
    assert_eq!(l.info().unwrap().transaction_count, before);
}

#[test]
fn s_pay_4_corrections_keep_the_method() {
    let (_d, mut l) = setup();
    l.assign_payment_method(&nick("Cash"), &[10]).unwrap();
    let c = l.correct(10, "typo", Replacement { amount_cents: Some(2600), ..Default::default() }).unwrap();
    assert_eq!(name_of(&c.replacement), Some("Cash"));

    let c2 = l
        .correct(
            c.replacement.id,
            "was the Visa",
            Replacement { payment_method: Some(nick("Business Visa")), ..Default::default() },
        )
        .unwrap();
    assert_eq!(name_of(&c2.replacement), Some("Business Visa"));

    let c3 = l.correct(5, "typo", Replacement { amount_cents: Some(1000), ..Default::default() }).unwrap();
    assert_eq!(name_of(&c3.replacement), None);

    l.assign_payment_method(&nick("Cash"), &[c2.replacement.id]).unwrap();
    l.retire_payment_method(&nick("Cash")).unwrap();
    let c4 = l.correct(c2.replacement.id, "again", Replacement { amount_cents: Some(2700), ..Default::default() }).unwrap();
    assert_eq!(name_of(&c4.replacement), Some("Cash"));
}
