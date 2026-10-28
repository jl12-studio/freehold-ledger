use crate::categories::category;
use crate::db::Ledger;
use crate::error::LedgerError;
use crate::model::{Filter, Kind};
use chrono::{Datelike, Duration, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

type Result<T> = std::result::Result<T, LedgerError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Period {
    ThisMonth,
    LastMonth,
    ThisQuarter,
    LastQuarter,
    YearToDate,
    ThisYear,
    LastYear,
}

fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).expect("valid calendar date")
}

fn month_range(y: i32, m: u32) -> (NaiveDate, NaiveDate) {
    let first = ymd(y, m, 1);
    let next = if m == 12 { ymd(y + 1, 1, 1) } else { ymd(y, m + 1, 1) };
    (first, next - Duration::days(1))
}

fn quarter_range(y: i32, q: u32) -> (NaiveDate, NaiveDate) {
    let start = q * 3 + 1;
    (month_range(y, start).0, month_range(y, start + 2).1)
}

/// Inclusive date range for a named period, relative to `today`.
pub fn resolve_period(p: Period, today: NaiveDate) -> (NaiveDate, NaiveDate) {
    let (y, m) = (today.year(), today.month());
    let q = (m - 1) / 3;
    match p {
        Period::ThisMonth => month_range(y, m),
        Period::LastMonth => if m == 1 { month_range(y - 1, 12) } else { month_range(y, m - 1) },
        Period::ThisQuarter => quarter_range(y, q),
        Period::LastQuarter => if q == 0 { quarter_range(y - 1, 3) } else { quarter_range(y, q - 1) },
        Period::YearToDate => (ymd(y, 1, 1), today),
        Period::ThisYear => (ymd(y, 1, 1), ymd(y, 12, 31)),
        Period::LastYear => (ymd(y - 1, 1, 1), ymd(y - 1, 12, 31)),
    }
}

#[derive(Debug, Clone)]
pub struct SummaryRequest {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub category: Option<String>,
    pub source: Option<String>,
    pub is_asset: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CategoryTotal {
    pub key: String,
    pub label: String,
    pub schedule_c_line: Option<String>,
    pub kind: Kind,
    pub total_cents: i64,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceTotal {
    pub source: String,
    pub total_cents: i64,
    pub count: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Totals {
    pub total_cents: i64,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub expenses_total_cents: i64,
    pub income_total_cents: i64,
    /// Income minus expenses; negative when spending is higher.
    pub profit_cents: i64,
    pub by_category: Vec<CategoryTotal>,
    pub by_source: Vec<SourceTotal>,
    pub assets: Totals,
    pub uncategorized: Totals,
    pub voided_excluded: i64,
}

impl Ledger {
    pub fn summarize(&self, r: &SummaryRequest) -> Result<Summary> {
        if r.from > r.to {
            return Err(LedgerError::InvalidRange { from: r.from, to: r.to });
        }
        let rows = self.list(&Filter {
            from: Some(r.from),
            to: Some(r.to),
            category: r.category.clone(),
            source: r.source.clone(),
            is_asset: r.is_asset,
            include_voided: true,
            ..Default::default()
        })?;
        let mut s = Summary {
            from: r.from,
            to: r.to,
            expenses_total_cents: 0,
            income_total_cents: 0,
            profit_cents: 0,
            by_category: Vec::new(),
            by_source: Vec::new(),
            assets: Totals::default(),
            uncategorized: Totals::default(),
            voided_excluded: 0,
        };
        let mut cats: HashMap<String, CategoryTotal> = HashMap::new();
        let mut srcs: HashMap<String, SourceTotal> = HashMap::new();
        for t in rows {
            if t.void.is_some() {
                s.voided_excluded += 1;
                continue;
            }
            match t.kind {
                Kind::Expense => s.expenses_total_cents += t.amount_cents,
                Kind::Income => s.income_total_cents += t.amount_cents,
            }
            if t.is_asset {
                s.assets.total_cents += t.amount_cents;
                s.assets.count += 1;
            }
            if t.category == "uncategorized" {
                s.uncategorized.total_cents += t.amount_cents;
                s.uncategorized.count += 1;
            }
            let c = cats.entry(t.category.clone()).or_insert_with(|| CategoryTotal {
                key: t.category.clone(),
                label: category(&t.category).map(|c| c.label.to_string()).unwrap_or_else(|| t.category.clone()),
                schedule_c_line: t.schedule_c_line.clone(),
                kind: t.kind,
                total_cents: 0,
                count: 0,
            });
            c.total_cents += t.amount_cents;
            c.count += 1;
            if let Some(name) = &t.income_source {
                let e = srcs
                    .entry(name.to_lowercase())
                    .or_insert_with(|| SourceTotal { source: name.clone(), total_cents: 0, count: 0 });
                e.total_cents += t.amount_cents;
                e.count += 1;
            }
        }
        s.profit_cents = s.income_total_cents - s.expenses_total_cents;
        let mut by_category: Vec<_> = cats.into_values().filter(|c| c.total_cents != 0).collect();
        by_category.sort_by(|a, b| {
            (a.kind == Kind::Income, -a.total_cents, &a.key).cmp(&(b.kind == Kind::Income, -b.total_cents, &b.key))
        });
        s.by_category = by_category;
        let mut by_source: Vec<_> = srcs.into_values().collect();
        by_source.sort_by(|a, b| b.total_cents.cmp(&a.total_cents).then_with(|| a.source.cmp(&b.source)));
        s.by_source = by_source;
        Ok(s)
    }
}
