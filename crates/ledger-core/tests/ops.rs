mod common;
use chrono::NaiveDate;
use ledger_core::*;

fn d(s: &str) -> NaiveDate {
    parse_date(s).unwrap()
}

fn exp(cents: i64, desc: &str, cat: &str, asset: bool) -> NewExpense {
    NewExpense { occurred_on: None, amount_cents: cents, description: desc.into(), category: cat.into(), is_asset: asset }
}

fn inc(cents: i64, desc: &str, src: &str) -> NewIncome {
    NewIncome { occurred_on: None, amount_cents: cents, description: desc.into(), category: None, source: src.into() }
}

fn count(l: &Ledger) -> i64 {
    l.info().unwrap().transaction_count
}

#[test]
fn s_rec_1_scene_expense_defaults_to_today() {
    let (_d, mut l) = common::empty();
    let t = l.record_expense(exp(30000, "Device", "supplies", true)).unwrap();
    let listed = l.list(&Filter::default()).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, t.id);
    assert_eq!(listed[0].occurred_on, chrono::Local::now().date_naive());
    assert_eq!(listed[0].amount_cents, 30000);
    assert_eq!(listed[0].category, "supplies");
    assert!(listed[0].is_asset);
    assert_eq!(listed[0].schedule_c_line.as_deref(), Some("22"));
}

#[test]
fn s_rec_2_income_with_source_reuses_source_case_insensitively() {
    let (_d, mut l) = common::empty();
    let a = l.record_income(inc(120000, "Website build", "Upwork")).unwrap();
    let b = l.record_income(inc(5000, "Fix", "upwork")).unwrap();
    assert_eq!(a.income_source.as_deref(), Some("Upwork"));
    assert_eq!(b.income_source.as_deref(), Some("Upwork"));
    assert_eq!(l.income_sources().unwrap(), vec!["Upwork".to_string()]);
    assert_eq!(a.category, "gross_receipts");
}

#[test]
fn s_rec_3_invalid_inputs_rejected_and_nothing_written() {
    let (_d, mut l) = common::empty();
    assert!(matches!(l.record_expense(exp(0, "x", "supplies", false)), Err(LedgerError::InvalidAmount(0))));
    assert!(matches!(l.record_expense(exp(-5, "x", "supplies", false)), Err(LedgerError::InvalidAmount(-5))));
    assert!(matches!(l.record_expense(exp(100, "x", "toys", false)), Err(LedgerError::UnknownCategory { .. })));
    assert!(matches!(l.record_expense(exp(100, "x", "gross_receipts", false)), Err(LedgerError::UnknownCategory { .. })));
    assert!(matches!(l.record_expense(exp(100, "  ", "supplies", false)), Err(LedgerError::MissingDescription)));
    assert!(matches!(l.record_income(inc(100, "x", "  ")), Err(LedgerError::MissingSource)));
    assert!(matches!(parse_date("2026-13-01"), Err(LedgerError::InvalidDate(_))));
    assert_eq!(count(&l), 0);
    assert!(l.income_sources().unwrap().is_empty());
}

#[test]
fn s_cor_1_void_excludes_and_keeps_trail() {
    let (_d, mut l) = common::empty();
    let t = l.record_expense(exp(1999, "Dup", "advertising", false)).unwrap();
    l.void(t.id, "Duplicate entry").unwrap();
    assert!(l.list(&Filter::default()).unwrap().is_empty());
    let all = l.list(&Filter { include_voided: true, ..Default::default() }).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].void.as_ref().unwrap().reason, "Duplicate entry");
    assert!(matches!(l.void(t.id, "again"), Err(LedgerError::AlreadyVoided(_))));
}

#[test]
fn s_cor_2_correct_voids_original_and_links_replacement() {
    let (_d, mut l) = common::empty();
    let t = l.record_expense(exp(3000, "Monitor", "supplies", false)).unwrap();
    let c = l
        .correct(t.id, "Typo", Replacement { amount_cents: Some(30000), ..Default::default() })
        .unwrap();
    assert_eq!(c.voided.amount_cents, 3000);
    assert_eq!(c.voided.void.as_ref().unwrap().reason, "Typo");
    assert_eq!(c.replacement.amount_cents, 30000);
    assert_eq!(c.replacement.replaces_id, Some(t.id));
    assert_eq!(c.voided.replaced_by_id, Some(c.replacement.id));
    assert_eq!(c.replacement.description, "Monitor");
    assert_eq!(l.get(t.id).unwrap().replaced_by_id, Some(c.replacement.id));
    assert!(matches!(l.correct(t.id, "again", Replacement::default()), Err(LedgerError::Voided(_))));
}

#[test]
fn s_cor_3_reclassify_keeps_history() {
    let (_d, mut l) = common::empty();
    let t = l.record_expense(exp(4210, "Ink", "uncategorized", false)).unwrap();
    let r = l.reclassify(t.id, "office_expense", "Office supplies").unwrap();
    assert_eq!(r.category, "office_expense");
    assert_eq!(r.original_category, "uncategorized");
    assert_eq!(r.classification_history.len(), 1);
    let v = l.record_expense(exp(100, "x", "supplies", false)).unwrap();
    l.void(v.id, "oops").unwrap();
    assert!(matches!(l.reclassify(v.id, "travel", "r"), Err(LedgerError::Voided(_))));
    let i = l.record_income(inc(100, "x", "Etsy")).unwrap();
    assert!(matches!(l.reclassify(i.id, "supplies", "r"), Err(LedgerError::UnknownCategory { .. })));
}

#[test]
fn s_lst_1_filters_on_seeded_ledger() {
    let (_d, l) = common::seeded();
    let f = |f: Filter| l.list(&f).unwrap();
    let range = f(Filter { from: Some(d("2026-07-01")), to: Some(d("2026-09-30")), ..Default::default() });
    let descs: Vec<_> = range.iter().map(|t| t.description.as_str()).collect();
    assert_eq!(descs, vec!["Flyers", "Print sale", "Printer ink and paper"]);
    assert_eq!(f(Filter { kind: Some(Kind::Income), ..Default::default() }).len(), 2);
    let office = f(Filter { category: Some("office_expense".into()), ..Default::default() });
    assert_eq!(office.len(), 1);
    assert_eq!(office[0].original_category, "uncategorized");
    assert_eq!(f(Filter { source: Some("etsy".into()), ..Default::default() }).len(), 1);
    assert_eq!(f(Filter { is_asset: Some(true), ..Default::default() }).len(), 1);
    let y25 = f(Filter { to: Some(d("2025-12-31")), ..Default::default() });
    assert_eq!(y25.len(), 1);
    assert_eq!(y25[0].category, "rent_other");
    assert_eq!(f(Filter { from: Some(d("2026-01-01")), ..Default::default() }).len(), 7);
    assert_eq!(f(Filter { include_voided: true, ..Default::default() }).len(), 10);
}
