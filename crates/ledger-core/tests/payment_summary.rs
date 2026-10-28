mod common;

use chrono::NaiveDate;
use ledger_core::{
    BatchEntry, Filter, Ledger, LedgerError, NewBatch, NewEntry, NewExpense, NewIncome, NewPaymentMethod,
    PaymentKind, PaymentMethodFilter, PaymentMethodKey, SummaryRequest,
};
use rusqlite::Connection;

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

fn nick(s: &str) -> PaymentMethodKey {
    PaymentMethodKey::Nickname(s.into())
}

fn pm(kind: PaymentKind, nickname: &str, last4: Option<&str>, make_default: bool) -> NewPaymentMethod {
    NewPaymentMethod { kind, nickname: nickname.into(), last4: last4.map(String::from), make_default }
}

fn with_methods(mut l: Ledger) -> Ledger {
    l.create_payment_method(pm(PaymentKind::Card, "Business Visa", Some("4242"), true)).unwrap();
    l.create_payment_method(pm(PaymentKind::BankAccount, "Checking", Some("6789"), false)).unwrap();
    l.create_payment_method(pm(PaymentKind::Cash, "Cash", None, false)).unwrap();
    l
}

fn exp(date: &str, cents: i64, desc: &str, cat: &str, pmk: Option<PaymentMethodKey>) -> BatchEntry {
    BatchEntry {
        entry: NewEntry::Expense(NewExpense {
            occurred_on: Some(d(date)),
            amount_cents: cents,
            description: desc.into(),
            category: cat.into(),
            is_asset: false,
            payment_method: None,
        }),
        allow_duplicate: false,
        payment_method: pmk,
    }
}

fn g1() -> BatchEntry {
    exp("2026-01-20", 1817, "GODADDY.COM", "other_expense", None)
}
fn st(pmk: Option<PaymentMethodKey>) -> BatchEntry {
    exp("2026-01-06", 8640, "STAPLES #0412", "supplies", pmk)
}
fn up() -> BatchEntry {
    BatchEntry {
        entry: NewEntry::Income(NewIncome {
            occurred_on: Some(d("2026-01-04")),
            amount_cents: 125000,
            description: "UPWORK ESCROW PAYOUT".into(),
            category: None,
            source: "Upwork".into(),
            payment_method: None,
        }),
        allow_duplicate: false,
        payment_method: None,
    }
}

fn batch(label: &str, pmk: Option<PaymentMethodKey>, entries: Vec<BatchEntry>) -> NewBatch {
    NewBatch { label: label.into(), entries, payment_method: pmk }
}

fn method_of(l: &Ledger, entry: &ledger_core::RecordedEntry) -> Option<String> {
    let t = l.list(&Filter { include_voided: true, ..Default::default() }).unwrap();
    t.iter().find(|t| t.id == entry.transaction.id).unwrap().payment_method.as_ref().map(|p| p.nickname.clone())
}

fn raw_count(l: &Ledger, sql: &str) -> i64 {
    Connection::open(l.path()).unwrap().query_row(sql, [], |r| r.get(0)).unwrap()
}

fn req(pmf: Option<PaymentMethodFilter>) -> SummaryRequest {
    SummaryRequest {
        from: d("2026-01-01"),
        to: d("2026-12-31"),
        category: None,
        source: None,
        is_asset: None,
        payment_method: pmf,
        business_id: None,
    }
}

#[test]
fn s_pay_5_one_method_for_the_whole_statement() {
    let (_d, l) = common::empty();
    let mut l = with_methods(l);
    let r = l
        .record_batch(batch("chase-jan.csv", Some(nick("Checking")), vec![g1(), up(), st(Some(nick("Cash")))]))
        .unwrap();
    assert_eq!(r.recorded.len(), 3);
    assert_eq!(method_of(&l, &r.recorded[0]).as_deref(), Some("Checking"));
    assert_eq!(method_of(&l, &r.recorded[1]).as_deref(), Some("Checking"));
    assert_eq!(method_of(&l, &r.recorded[2]).as_deref(), Some("Cash"));
}

#[test]
fn s_pay_6_defaults_inside_a_batch() {
    let (_d, l) = common::empty();
    let mut l = with_methods(l);
    let r = l.record_batch(batch("jan.csv", None, vec![g1(), up()])).unwrap();
    assert_eq!(method_of(&l, &r.recorded[0]).as_deref(), Some("Business Visa"));
    assert_eq!(method_of(&l, &r.recorded[1]), None);
}

#[test]
fn s_pay_7_bad_method_records_nothing() {
    let (_d, l) = common::empty();
    let mut l = with_methods(l);
    let e = l.record_batch(batch("x", Some(nick("Nope")), vec![g1(), up()])).unwrap_err();
    assert!(matches!(e, LedgerError::UnknownPaymentMethod(_)), "{e:?}");
    assert_eq!(l.info().unwrap().transaction_count, 0);
    assert_eq!(raw_count(&l, "SELECT COUNT(*) FROM batches"), 0);

    l.retire_payment_method(&nick("Cash")).unwrap();
    let e = l.record_batch(batch("x", None, vec![g1(), st(Some(nick("Cash")))])).unwrap_err();
    match e {
        LedgerError::BatchEntry { index, ref reason } => {
            assert_eq!(index, 2);
            assert!(reason.to_string().contains("retired"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(l.info().unwrap().transaction_count, 0);
    assert_eq!(raw_count(&l, "SELECT COUNT(*) FROM batches"), 0);
}

#[test]
fn s_pay_8_duplicates_still_skip() {
    let (_d, l) = common::empty();
    let mut l = with_methods(l);
    let first = l.record_batch(batch("chase-jan.csv", Some(nick("Checking")), vec![g1(), up()])).unwrap();
    let again = l.record_batch(batch("again", Some(nick("Cash")), vec![g1()])).unwrap();
    assert!(again.recorded.is_empty());
    assert_eq!(again.skipped_duplicates.len(), 1);
    assert_eq!(again.skipped_duplicates[0].matches_id, first.recorded[0].transaction.id);
    assert_eq!(method_of(&l, &first.recorded[0]).as_deref(), Some("Checking"));
}

fn assigned_seeded() -> (tempfile::TempDir, Ledger) {
    let (dir, l) = common::seeded();
    let mut l = with_methods(l);
    l.assign_payment_method(&nick("Business Visa"), &[1, 10]).unwrap();
    l.assign_payment_method(&nick("Checking"), &[2, 3]).unwrap();
    l.assign_payment_method(&nick("Cash"), &[4]).unwrap();
    (dir, l)
}

#[test]
fn s_sum_4_breakdown_by_payment_method() {
    let (_d, plain) = common::seeded();
    let before = serde_json::to_value(plain.summarize(&req(None)).unwrap()).unwrap();
    let (_d2, l) = assigned_seeded();
    let s = l.summarize(&req(None)).unwrap();
    let rows: Vec<(Option<&str>, i64, i64, i64)> = s
        .by_payment_method
        .iter()
        .map(|b| (b.nickname.as_deref(), b.expenses_total_cents, b.income_total_cents, b.count))
        .collect();
    assert_eq!(
        rows,
        vec![
            (Some("Checking"), 0, 128500, 2),
            (Some("Business Visa"), 32500, 0, 2),
            (Some("Cash"), 4210, 0, 1),
            (None, 31999, 0, 2),
        ]
    );
    assert_eq!(s.by_payment_method[0].last4.as_deref(), Some("6789"));
    assert_eq!(s.by_payment_method[3].payment_method_id, None);

    let mut after = serde_json::to_value(&s).unwrap();
    after.as_object_mut().unwrap().remove("by_payment_method");
    let mut before = before;
    before.as_object_mut().unwrap().remove("by_payment_method");
    assert_eq!(after, before);
}

#[test]
fn s_sum_5_filter_by_payment_method() {
    let (_d, l) = assigned_seeded();
    let visa = l.find_payment_method(&nick("Business Visa")).unwrap().id;
    let s = l.summarize(&req(Some(PaymentMethodFilter::Id(visa)))).unwrap();
    assert_eq!(s.expenses_total_cents, 32500);
    assert_eq!(s.income_total_cents, 0);
    assert_eq!(s.profit_cents, -32500);
    let s = l.summarize(&req(Some(PaymentMethodFilter::Unassigned))).unwrap();
    assert_eq!(s.expenses_total_cents, 31999);
}
