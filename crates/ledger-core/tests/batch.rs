mod common;

use chrono::NaiveDate;
use common::empty;
use ledger_core::{
    BatchEntry, Filter, Kind, LedgerError, NewBatch, NewEntry, NewExpense, NewIncome, SummaryRequest,
    MAX_BATCH,
};

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

fn exp(date: &str, cents: i64, desc: &str, cat: &str) -> BatchEntry {
    BatchEntry {
        entry: NewEntry::Expense(NewExpense {
            occurred_on: Some(d(date)),
            amount_cents: cents,
            description: desc.into(),
            category: cat.into(),
            is_asset: false,
            payment_method: None,
        }),
        allow_duplicate: false, payment_method: None,
    }
}

fn inc(date: &str, cents: i64, desc: &str, source: &str) -> BatchEntry {
    BatchEntry {
        entry: NewEntry::Income(NewIncome {
            occurred_on: Some(d(date)),
            amount_cents: cents,
            description: desc.into(),
            category: None,
            source: source.into(),
            payment_method: None,
        }),
        allow_duplicate: false, payment_method: None,
    }
}

fn g1() -> BatchEntry {
    exp("2026-01-20", 1817, "GODADDY.COM", "other_expense")
}
fn st() -> BatchEntry {
    exp("2026-01-06", 8640, "STAPLES #0412", "supplies")
}
fn up() -> BatchEntry {
    inc("2026-01-04", 125000, "UPWORK ESCROW PAYOUT", "Upwork")
}
fn lic() -> BatchEntry {
    exp("2026-01-22", 7500, "CITY OF AUSTIN BUSINESS LICENSE", "taxes_licenses")
}
fn batch(label: &str, entries: Vec<BatchEntry>) -> NewBatch {
    NewBatch { label: label.into(), entries, payment_method: None }
}

fn summary_2026(l: &ledger_core::Ledger) -> ledger_core::Summary {
    l
        .summarize(&SummaryRequest {
            from: d("2026-01-01"),
            to: d("2026-12-31"),
            category: None,
            source: None,
            is_asset: None,
            payment_method: None,
        })
        .unwrap()
}

#[test]
fn s_dup_1_reimport_records_nothing_new() {
    let (_d, mut l) = empty();
    let first = l.record_batch(batch("bank-export.csv", vec![g1(), st(), up()])).unwrap();
    assert_eq!(first.recorded.len(), 3);
    assert!(first.batch_id.is_some());
    assert_eq!(l.info().unwrap().transaction_count, 3);

    let again = l.record_batch(batch("bank-export.csv (again)", vec![g1(), st(), up()])).unwrap();
    assert!(again.recorded.is_empty());
    assert_eq!(again.batch_id, None);
    let skipped: Vec<(usize, i64)> = again.skipped_duplicates.iter().map(|s| (s.entry, s.matches_id)).collect();
    let ids: Vec<i64> = first.recorded.iter().map(|r| r.transaction.id).collect();
    assert_eq!(skipped, vec![(1, ids[0]), (2, ids[1]), (3, ids[2])]);
    assert_eq!(l.info().unwrap().transaction_count, 3);
    let s = summary_2026(&l);
    assert_eq!(s.expenses_total_cents, 10457);
    assert_eq!(s.income_total_cents, 125000);
}

#[test]
fn s_dup_2_overlapping_export_records_only_new_rows() {
    let (_d, mut l) = empty();
    let first = l.record_batch(batch("a.csv", vec![g1(), st()])).unwrap();
    let r = l.record_batch(batch("b.csv", vec![st(), g1(), lic()])).unwrap();
    assert_eq!(r.recorded.len(), 1);
    assert_eq!(r.recorded[0].entry, 3);
    let skipped: Vec<(usize, i64)> = r.skipped_duplicates.iter().map(|s| (s.entry, s.matches_id)).collect();
    assert_eq!(skipped, vec![(1, first.recorded[1].transaction.id), (2, first.recorded[0].transaction.id)]);
}

#[test]
fn s_dup_3_duplicate_inside_one_call() {
    let (_d, mut l) = empty();
    let r = l.record_batch(batch("x", vec![g1(), g1(), st()])).unwrap();
    assert_eq!(r.recorded.len(), 2);
    assert_eq!(r.skipped_duplicates.len(), 1);
    assert_eq!(r.skipped_duplicates[0].entry, 2);
    assert_eq!(r.skipped_duplicates[0].matches_id, r.recorded[0].transaction.id);

    let mut again = g1();
    again.allow_duplicate = true;
    let r2 = l.record_batch(batch("second charge", vec![again])).unwrap();
    assert_eq!(r2.recorded.len(), 1);
    let live = l.list(&Filter::default()).unwrap().iter().filter(|t| t.description == "GODADDY.COM").count();
    assert_eq!(live, 2);
}

#[test]
fn s_dup_4_voided_rows_are_not_duplicates() {
    let (_d, mut l) = empty();
    let r = l.record_batch(batch("x", vec![g1()])).unwrap();
    l.void(r.recorded[0].transaction.id, "mistake").unwrap();
    let r2 = l.record_batch(batch("y", vec![g1()])).unwrap();
    assert_eq!(r2.recorded.len(), 1);
    assert!(r2.skipped_duplicates.is_empty());
}

#[test]
fn s_dup_5_match_boundaries() {
    let (_d, mut l) = empty();
    let first = l.record_batch(batch("x", vec![g1()])).unwrap();
    let lower = exp("2026-01-20", 1817, "godaddy.com", "other_expense");
    let r = l.record_batch(batch("y", vec![lower])).unwrap();
    assert_eq!(r.skipped_duplicates.len(), 1);
    assert_eq!(r.skipped_duplicates[0].matches_id, first.recorded[0].transaction.id);
    assert!(r.recorded.is_empty());

    let other_day = exp("2026-01-21", 1817, "GODADDY.COM", "other_expense");
    let other_amount = exp("2026-01-20", 1818, "GODADDY.COM", "other_expense");
    let other_kind = inc("2026-01-20", 1817, "GODADDY.COM", "GoDaddy");
    let r = l.record_batch(batch("z", vec![other_day, other_amount, other_kind])).unwrap();
    assert_eq!(r.recorded.len(), 3);
    assert!(r.skipped_duplicates.is_empty());
    assert_eq!(r.recorded[2].transaction.kind, Kind::Income);
}

#[test]
fn s_bat_1_all_or_nothing() {
    let (_d, mut l) = empty();
    let bad = exp("2026-01-08", 500, "TOY STORE", "toys");
    let err = l.record_batch(batch("x", vec![g1(), st(), bad])).unwrap_err();
    assert!(matches!(err, LedgerError::BatchEntry { index: 3, .. }));
    let msg = err.to_string();
    assert!(msg.starts_with("Entry 3:"), "{msg}");
    assert!(msg.ends_with("Nothing in this batch was recorded."), "{msg}");
    assert_eq!(l.info().unwrap().transaction_count, 0);

    let err = l.record_batch(batch("x", vec![up(), g1(), exp("2026-01-08", 500, "T", "toys")])).unwrap_err();
    assert!(matches!(err, LedgerError::BatchEntry { index: 3, .. }));
    assert!(l.income_sources().unwrap().is_empty());
    assert_eq!(l.info().unwrap().transaction_count, 0);
    // a later good batch still gets batch id 1: no batch row was left behind
    let ok = l.record_batch(batch("x", vec![g1()])).unwrap();
    assert_eq!(ok.batch_id, Some(1));
}

#[test]
fn s_bat_2_limits() {
    let (_d, mut l) = empty();
    assert!(matches!(l.record_batch(batch("x", vec![])).unwrap_err(), LedgerError::EmptyBatch));
    let many: Vec<BatchEntry> = (0..51).map(|i| exp("2026-01-05", 100 + i, "ROW", "supplies")).collect();
    assert!(matches!(l.record_batch(batch("x", many)).unwrap_err(), LedgerError::BatchTooLarge { count: 51 }));
    let fifty: Vec<BatchEntry> = (0..MAX_BATCH as i64).map(|i| exp("2026-01-05", 100 + i, "ROW", "supplies")).collect();
    let r = l.record_batch(batch("x", fifty)).unwrap();
    assert_eq!(r.recorded.len(), 50);
    assert!(r.batch_id.is_some());
    assert!(matches!(l.record_batch(batch("   ", vec![g1()])).unwrap_err(), LedgerError::MissingBatchLabel));
}

#[test]
fn s_bat_3_batch_link() {
    let (_d, mut l) = empty();
    let single = l
        .record_expense(NewExpense {
            occurred_on: Some(d("2026-01-02")),
            amount_cents: 999,
            description: "single".into(),
            category: "supplies".into(),
            is_asset: false,
            payment_method: None,
        })
        .unwrap();
    assert_eq!(single.batch_id, None);
    let r = l.record_batch(batch("x", vec![g1(), st()])).unwrap();
    let bid = r.batch_id.unwrap();
    for rec in &r.recorded {
        assert_eq!(rec.transaction.batch_id, Some(bid));
        assert_eq!(l.get(rec.transaction.id).unwrap().batch_id, Some(bid));
    }
    let listed = l.list(&Filter::default()).unwrap();
    for t in listed {
        if t.id == single.id {
            assert_eq!(t.batch_id, None);
        } else {
            assert_eq!(t.batch_id, Some(bid));
        }
    }
}

fn three(l: &mut ledger_core::Ledger) -> (i64, Vec<i64>) {
    let r = l.record_batch(batch("bank-export.csv", vec![g1(), st(), lic()])).unwrap();
    (r.batch_id.unwrap(), r.recorded.iter().map(|x| x.transaction.id).collect())
}

#[test]
fn s_bat_4_void_batch_undoes_an_import() {
    let (_d, mut l) = empty();
    let (b, ids) = three(&mut l);
    assert_eq!(summary_2026(&l).expenses_total_cents, 17957);
    let r = l.void_batch(b, "Imported the wrong file").unwrap();
    assert_eq!(r.batch_id, b);
    assert_eq!(r.label, "bank-export.csv");
    assert_eq!(r.voided.len(), 3);
    assert!(r.already_voided_ids.is_empty());
    for t in &r.voided {
        assert_eq!(t.void.as_ref().unwrap().reason, "Imported the wrong file");
    }
    assert_eq!(summary_2026(&l).expenses_total_cents, 0);
    assert_eq!(l.info().unwrap().transaction_count, 3);
    let all = l.list(&Filter { include_voided: true, ..Default::default() }).unwrap();
    assert_eq!(all.len(), 3);
    assert!(ids.iter().all(|i| all.iter().any(|t| t.id == *i)));
}

#[test]
fn s_bat_5_already_voided_rows_are_skipped_and_counted() {
    let (_d, mut l) = empty();
    let (b, ids) = three(&mut l);
    l.void(ids[1], "duplicate").unwrap();
    let r = l.void_batch(b, "Imported the wrong file").unwrap();
    assert_eq!(r.voided.len(), 2);
    assert_eq!(r.already_voided_ids, vec![ids[1]]);
    assert_eq!(l.get(ids[1]).unwrap().void.unwrap().reason, "duplicate");
}

#[test]
fn s_bat_6_corrected_rows_go_too() {
    let (_d, mut l) = empty();
    let (b, ids) = three(&mut l);
    let c = l
        .correct(ids[2], "typo", ledger_core::Replacement { amount_cents: Some(750), ..Default::default() })
        .unwrap();
    assert_eq!(c.replacement.batch_id, Some(b));
    let r = l.void_batch(b, "Imported the wrong file").unwrap();
    let mut got: Vec<i64> = r.voided.iter().map(|t| t.id).collect();
    got.sort();
    let mut want = vec![ids[0], ids[1], c.replacement.id];
    want.sort();
    assert_eq!(got, want);
    assert_eq!(r.already_voided_ids, vec![ids[2]]);
    assert_eq!(summary_2026(&l).expenses_total_cents, 0);

    let single = l
        .record_expense(NewExpense {
            occurred_on: Some(d("2026-02-02")),
            amount_cents: 500,
            description: "solo".into(),
            category: "supplies".into(),
            is_asset: false,
            payment_method: None,
        })
        .unwrap();
    let c2 = l.correct(single.id, "typo", Default::default()).unwrap();
    assert_eq!(c2.replacement.batch_id, None);
}

#[test]
fn s_bat_7_errors_write_nothing() {
    let (_d, mut l) = empty();
    let (b, _) = three(&mut l);
    assert!(matches!(l.void_batch(999, "x"), Err(LedgerError::UnknownBatch(999))));
    assert!(matches!(l.void_batch(b, "   "), Err(LedgerError::MissingReason)));
    assert_eq!(l.list(&Filter::default()).unwrap().len(), 3);
    l.void_batch(b, "wrong file").unwrap();
    let again = l.void_batch(b, "wrong file").unwrap();
    assert!(again.voided.is_empty());
    assert_eq!(again.already_voided_ids.len(), 3);
}

#[test]
fn s_lst_2_list_filters_by_batch() {
    let (_d, mut l) = empty();
    let b1 = l.record_batch(batch("one", vec![g1(), st()])).unwrap();
    let b2 = l.record_batch(batch("two", vec![lic()])).unwrap();
    let (b1, b2) = (b1.batch_id.unwrap(), b2.batch_id.unwrap());
    let mut descs: Vec<String> = l
        .list(&Filter { batch_id: Some(b1), ..Default::default() })
        .unwrap()
        .into_iter()
        .map(|t| t.description)
        .collect();
    descs.sort();
    assert_eq!(descs, vec!["GODADDY.COM", "STAPLES #0412"]);
    let two = l.list(&Filter { batch_id: Some(b2), ..Default::default() }).unwrap();
    assert_eq!(two.len(), 1);
    assert_eq!(two[0].description, "CITY OF AUSTIN BUSINESS LICENSE");
    l.void_batch(b1, "oops").unwrap();
    assert!(l.list(&Filter { batch_id: Some(b1), ..Default::default() }).unwrap().is_empty());
    let all = l.list(&Filter { batch_id: Some(b1), include_voided: true, ..Default::default() }).unwrap();
    assert_eq!(all.len(), 2);
}
