mod common;
use chrono::NaiveDate;
use ledger_core::{Filter, Kind, LedgerError, Period, SummaryRequest};

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}
fn today() -> NaiveDate {
    d("2026-10-27")
}
fn req(p: Period) -> SummaryRequest {
    let (from, to) = ledger_core::resolve_period(p, today());
    SummaryRequest { from, to, category: None, source: None, is_asset: None, payment_method: None, business_id: None }
}

#[test]
fn s_sum_1_quarter_and_assets() {
    let (_d, l) = common::seeded();
    assert_eq!(ledger_core::resolve_period(Period::ThisQuarter, today()), (d("2026-10-01"), d("2026-12-31")));
    let mut r = req(Period::ThisQuarter);
    r.is_asset = Some(true);
    let s = l.summarize(&r).unwrap();
    assert_eq!(s.expenses_total_cents, 30000);
    assert_eq!(s.assets.total_cents, 30000);
    assert_eq!(s.assets.count, 1);

    assert_eq!(ledger_core::resolve_period(Period::LastQuarter, today()), (d("2026-07-01"), d("2026-09-30")));
    let s = l.summarize(&req(Period::LastQuarter)).unwrap();
    assert_eq!(s.expenses_total_cents, 6710);
    assert_eq!(s.income_total_cents, 8500);
    let ink = s.by_category.iter().find(|c| c.key == "office_expense").unwrap();
    assert_eq!(ink.total_cents, 4210);
    assert!(s.by_category.iter().all(|c| c.key != "uncategorized"));
}

#[test]
fn s_sum_2_income_by_source_this_year() {
    let (_d, l) = common::seeded();
    let s = l.summarize(&req(Period::ThisYear)).unwrap();
    let got: Vec<(&str, i64)> = s.by_source.iter().map(|x| (x.source.as_str(), x.total_cents)).collect();
    assert_eq!(got, vec![("Upwork", 120000), ("Etsy", 8500)]);
    let mut r = req(Period::ThisYear);
    r.source = Some("upwork".into());
    let s = l.summarize(&r).unwrap();
    assert_eq!(s.income_total_cents, 120000);
}

#[test]
fn s_sum_3_profit_excludes_voided() {
    let (_d, l) = common::seeded();
    let s = l.summarize(&req(Period::ThisYear)).unwrap();
    assert_eq!(s.expenses_total_cents, 68709);
    assert_eq!(s.income_total_cents, 128500);
    assert_eq!(s.profit_cents, 59791);
    assert_eq!(s.voided_excluded, 2);
    assert_eq!(s.uncategorized.count, 0);
    // expenses first, descending, then income
    assert_eq!(s.by_category.first().unwrap().kind, Kind::Expense);
    assert_eq!(s.by_category.last().unwrap().kind, Kind::Income);

    // reconciles with list_transactions
    let f = Filter { from: Some(s.from), to: Some(s.to), ..Default::default() };
    let listed = l.list(&f).unwrap();
    let sum = |k: Kind| listed.iter().filter(|t| t.kind == k).map(|t| t.amount_cents).sum::<i64>();
    assert_eq!(sum(Kind::Expense), s.expenses_total_cents);
    assert_eq!(sum(Kind::Income), s.income_total_cents);

    let s = l.summarize(&req(Period::LastYear)).unwrap();
    assert_eq!(s.expenses_total_cents, 90000);
    assert_eq!(s.profit_cents, -90000);
}

#[test]
fn uncategorized_is_counted_until_reclassified() {
    let (_d, mut l) = common::seeded();
    let t = l
        .record_expense(ledger_core::NewExpense {
            occurred_on: Some(d("2026-10-01")),
            amount_cents: 500,
            description: "Mystery".into(),
            category: "uncategorized".into(),
            is_asset: false,
            payment_method: None,
        })
        .unwrap();
    let s = l.summarize(&req(Period::ThisYear)).unwrap();
    assert_eq!((s.uncategorized.count, s.uncategorized.total_cents), (1, 500));
    l.reclassify(t.id, "supplies", "Pens").unwrap();
    assert_eq!(l.summarize(&req(Period::ThisYear)).unwrap().uncategorized.count, 0);
}

#[test]
fn period_resolution_boundaries() {
    use Period::*;
    let r = |p, t: &str| ledger_core::resolve_period(p, d(t));
    assert_eq!(r(ThisMonth, "2026-01-01"), (d("2026-01-01"), d("2026-01-31")));
    assert_eq!(r(LastMonth, "2026-01-15"), (d("2025-12-01"), d("2025-12-31")));
    assert_eq!(r(ThisQuarter, "2026-03-31"), (d("2026-01-01"), d("2026-03-31")));
    assert_eq!(r(LastQuarter, "2026-01-02"), (d("2025-10-01"), d("2025-12-31")));
    assert_eq!(r(ThisMonth, "2028-02-10"), (d("2028-02-01"), d("2028-02-29")));
    assert_eq!(r(LastMonth, "2028-03-01"), (d("2028-02-01"), d("2028-02-29")));
    assert_eq!(r(YearToDate, "2026-12-31"), (d("2026-01-01"), d("2026-12-31")));
    assert_eq!(r(YearToDate, "2026-10-27"), (d("2026-01-01"), d("2026-10-27")));
    assert_eq!(r(ThisYear, "2026-12-31"), (d("2026-01-01"), d("2026-12-31")));
    assert_eq!(r(LastYear, "2026-01-01"), (d("2025-01-01"), d("2025-12-31")));
}

#[test]
fn invalid_range_rejected() {
    let (_d, l) = common::seeded();
    let r = SummaryRequest { from: d("2026-02-01"), to: d("2026-01-01"), category: None, source: None, is_asset: None, payment_method: None, business_id: None };
    assert!(matches!(l.summarize(&r), Err(LedgerError::InvalidRange { .. })));
}

#[test]
fn s_sum_6_summary_per_business() {
    let (_d, mut l) = common::seeded();
    l.create_business(ledger_core::NewBusiness { name: "Test shop".into(), note: None, is_test: true, make_current: false })
        .unwrap();
    l.record_expense_in(
        Some(&ledger_core::BusinessKey::Id(2)),
        ledger_core::NewExpense {
            occurred_on: Some(d("2026-06-01")),
            amount_cents: 1200,
            description: "Shop thing".into(),
            category: "supplies".into(),
            is_asset: false,
            payment_method: None,
        },
    )
    .unwrap();
    let mut r = req(Period::ThisYear);
    r.business_id = Some(2);
    let s = l.summarize(&r).unwrap();
    assert_eq!(s.expenses_total_cents, 1200);
    assert_eq!(s.income_total_cents, 0);
    r.business_id = Some(1);
    let s = l.summarize(&r).unwrap();
    assert_eq!(s.expenses_total_cents, 68709);
}
