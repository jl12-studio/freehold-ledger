//! Tax-year export: RFC 4180 CSVs, one summary.json and a plain-language README.txt (D17).
use crate::categories::categories;
use crate::db::Ledger;
use crate::error::LedgerError;
use crate::model::{Filter, Kind, Transaction};
use crate::summary::{Summary, SummaryRequest};
use chrono::NaiveDate;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, LedgerError>;

#[derive(Debug, Clone)]
pub struct ExportResult {
    pub folder: PathBuf,
    pub files: Vec<String>,
    pub summary: Summary,
}

fn dollars(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let a = cents.abs();
    format!("{sign}{}.{:02}", a / 100, a % 100)
}

fn money_text(cents: i64) -> String {
    format!("${}", dollars(cents))
}

fn fail(path: &Path, reason: impl ToString) -> LedgerError {
    let reason = reason.to_string();
    let reason = if reason.to_lowercase().contains("permission denied") {
        "permission denied, so Freehold Ledger cannot write there. Pick a folder you can write to".to_string()
    } else {
        reason
    };
    LedgerError::ExportFailed { path: path.to_path_buf(), reason }
}

fn csv_file(path: &Path, header: &[&str], rows: &[Vec<String>]) -> Result<()> {
    let mut w = csv::WriterBuilder::new()
        .terminator(csv::Terminator::CRLF)
        .from_path(path)
        .map_err(|e| fail(path, e))?;
    w.write_record(header).map_err(|e| fail(path, e))?;
    for r in rows {
        w.write_record(r).map_err(|e| fail(path, e))?;
    }
    w.flush().map_err(|e| fail(path, e))
}

fn opt<T: ToString>(v: &Option<T>) -> String {
    v.as_ref().map(|x| x.to_string()).unwrap_or_default()
}

fn cat_order(key: &str) -> usize {
    categories().iter().position(|c| c.key == key).unwrap_or(usize::MAX)
}

struct Files {
    transactions: Vec<Vec<String>>,
    by_category: Vec<Vec<String>>,
    by_source: Vec<Vec<String>>,
    assets: Vec<Vec<String>>,
    corrections: Vec<Vec<String>>,
}

fn build(rows: &[Transaction]) -> Files {
    let mut f = Files {
        transactions: vec![],
        by_category: vec![],
        by_source: vec![],
        assets: vec![],
        corrections: vec![],
    };
    let mut cats: BTreeMap<usize, (String, String, String, i64, i64)> = BTreeMap::new();
    // lowercase source -> (source, category key, cents, count)
    let mut srcs: BTreeMap<String, (String, String, i64, i64)> = BTreeMap::new();
    for t in rows {
        let voided = t.void.is_some();
        f.transactions.push(vec![
            t.id.to_string(),
            t.occurred_on.to_string(),
            match t.kind {
                Kind::Expense => "expense",
                Kind::Income => "income",
            }
            .to_string(),
            t.description.clone(),
            dollars(t.amount_cents),
            t.amount_cents.to_string(),
            t.category.clone(),
            t.category_label.clone(),
            opt(&t.schedule_c_line),
            if t.is_asset { "yes" } else { "no" }.to_string(),
            opt(&t.income_source),
            if voided { "voided" } else { "active" }.to_string(),
            t.void.as_ref().map(|v| v.reason.clone()).unwrap_or_default(),
            opt(&t.replaces_id),
            opt(&t.replaced_by_id),
            t.original_category.clone(),
            t.recorded_at.clone(),
        ]);
        if let Some(v) = &t.void {
            f.corrections.push(vec![
                t.id.to_string(),
                "voided".into(),
                "Entry voided; it stays on record and is left out of all totals".into(),
                v.reason.clone(),
                v.recorded_at.clone(),
                opt(&t.replaced_by_id),
            ]);
        }
        let mut prev = t.original_category.clone();
        for c in &t.classification_history {
            f.corrections.push(vec![
                t.id.to_string(),
                "reclassified".into(),
                format!("Category changed from {prev} to {}", c.category),
                c.reason.clone(),
                c.recorded_at.clone(),
                String::new(),
            ]);
            prev = c.category.clone();
        }
        if voided {
            continue;
        }
        if t.kind == Kind::Expense {
            let e = cats.entry(cat_order(&t.category)).or_insert_with(|| {
                (t.schedule_c_line.clone().unwrap_or_default(), t.category.clone(), t.category_label.clone(), 0, 0)
            });
            e.3 += t.amount_cents;
            e.4 += 1;
            if t.is_asset {
                f.assets.push(vec![
                    t.id.to_string(),
                    t.occurred_on.to_string(),
                    t.description.clone(),
                    dollars(t.amount_cents),
                    t.category.clone(),
                    opt(&t.schedule_c_line),
                ]);
            }
        }
        if let Some(src) = &t.income_source {
            let e = srcs
                .entry(src.to_lowercase())
                .or_insert_with(|| (src.clone(), t.category.clone(), 0, 0));
            e.2 += t.amount_cents;
            e.3 += 1;
        }
    }
    f.by_category = cats
        .into_values()
        .filter(|c| c.3 != 0)
        .map(|(line, key, label, cents, n)| vec![line, key, label, dollars(cents), cents.to_string(), n.to_string()])
        .collect();
    let mut s: Vec<_> = srcs.into_values().collect();
    s.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
    f.by_source = s
        .into_iter()
        .map(|(src, key, cents, n)| vec![src, key, dollars(cents), cents.to_string(), n.to_string()])
        .collect();
    f
}

fn entries(n: &str) -> String {
    format!("{n} {}", if n == "1" { "entry" } else { "entries" })
}

/// The year's numbers in prose, so a reader of README.txt alone can answer
/// "what goes on Schedule C line 22" and "which entries are assets".
fn at_a_glance(f: &Files) -> String {
    let mut t = String::from("AT A GLANCE\n\nExpenses by Schedule C line (active entries; the same numbers as expenses-by-category.csv):\n");
    if f.by_category.is_empty() {
        t.push_str("- None.\n");
    }
    for r in &f.by_category {
        let line = if r[0].is_empty() { "No line yet".to_string() } else { format!("Line {}", r[0]) };
        t.push_str(&format!("- {line} {} ({}): ${} from {}\n", r[2], r[1], r[3], entries(&r[5])));
    }
    t.push_str("\nIncome by who paid (active entries; the same numbers as income-by-source.csv):\n");
    if f.by_source.is_empty() {
        t.push_str("- None.\n");
    }
    for r in &f.by_source {
        t.push_str(&format!("- {} ({}): ${} from {}\n", r[0], r[1], r[2], entries(&r[4])));
    }
    t.push_str(
        "\nAssets, meaning equipment or property kept longer than a year (the same rows as assets.csv). \
Each asset's amount is also included in its category's total above and in expenses-by-category.csv, \
so if an asset is depreciated or deducted on its own it should not be counted again in that category:\n",
    );
    if f.assets.is_empty() {
        t.push_str("- None.\n");
    }
    for r in &f.assets {
        let line = if r[5].is_empty() { String::new() } else { format!(", Schedule C line {}", r[5]) };
        t.push_str(&format!("- Entry {}, {}, {}: ${}, category {}{line}\n", r[0], r[1], r[2], r[3], r[4]));
    }
    t.push('\n');
    t
}

fn readme(year: i32, s: &Summary, f: &Files, empty: bool) -> String {
    let mut t = String::new();
    if empty {
        t.push_str(&format!("No entries were recorded for {year}.\n\n"));
    }
    t.push_str(&format!(
        "FREEHOLD LEDGER {year} EXPORT\n\n\
This folder is a copy of the bookkeeping for {year}, written so that an accountant, a tax preparer or an AI assistant can read it without asking questions. \
Every .csv file opens in Numbers, Excel or Google Sheets. summary.json holds the same totals for software.\n\n\
Period: {} to {}.\n\n\
Totals for the period: income {}, expenses {}, profit before taxes {}. \
Of the expenses, {} across {} entries are marked as assets, and {} across {} entries still need a category. \
{} voided entries are in the records but not in any total.\n\n\
Money is in US dollars. In the CSV files, amounts have two decimals and no symbol (for example 300.00). Columns ending in _cents are the same amount as a whole number of cents. Dates are YYYY-MM-DD.\n\n\
Voided entries are included for the record but excluded from all totals. Nothing in the ledger is ever deleted or edited; mistakes are voided or corrected and the history stays.\n\n\
Categories follow the IRS Schedule C (Form 1040) lines, for example line 22 is Supplies and line 18 is Office expense. Income on line 1 is gross receipts or sales. \"uncategorized\" (\"Needs a category\") has no line and should be sorted out before filing.\n\n\
Freehold Ledger does not give tax advice; your preparer decides how to treat assets.\n\n",
        s.from,
        s.to,
        money_text(s.income_total_cents),
        money_text(s.expenses_total_cents),
        money_text(s.profit_cents),
        money_text(s.assets.total_cents),
        s.assets.count,
        money_text(s.uncategorized.total_cents),
        s.uncategorized.count,
        s.voided_excluded,
    ));
    t.push_str(&at_a_glance(f));
    t.push_str(
        "FILES\n\n\
transactions.csv: every entry dated in the year, voided ones included. Columns: id (the entry number); date; kind (expense or income); description; amount (dollars); amount_cents; category_key (the current category); category_label (its plain name); schedule_c_line (the Schedule C line, blank for uncategorized); is_asset (yes if it is equipment or property kept longer than a year); income_source (who paid, for income); status (active or voided); voided_reason (why it was voided); replaces_id (the older entry this one replaced); replaced_by_id (the newer entry that replaced this one); original_category_key (the category when first recorded); recorded_at (when it was entered, UTC).\n\n\
expenses-by-category.csv: active expenses added up by category, in Schedule C line order with uncategorized last. Columns: schedule_c_line; category_key; category_label; total (dollars); total_cents; count (number of entries).\n\n\
income-by-source.csv: active income added up by who paid. Columns: income_source; category_key (income category); total (dollars); total_cents; count.\n\n\
assets.csv: active expenses marked as assets, listed one per row. Columns: id; date; description; amount (dollars); category_key; schedule_c_line. Freehold Ledger does not compute depreciation.\n\n\
corrections.csv: the full correction trail, one row per void and per category change. Columns: transaction_id (the entry affected); action (voided or reclassified); detail (what changed); reason (the reason given); recorded_at (when, UTC); replacement_id (the entry that replaced a voided one, if any).\n\n\
summary.json: the totals for the whole year as software-readable data: expenses_total_cents, income_total_cents, profit_cents, by_category, by_source, assets, uncategorized, voided_excluded, plus generated_at, app_version and schema_version.\n\n\
README.txt: this file.\n",
    );
    t
}

fn pick_folder(parent: &Path, year: i32) -> PathBuf {
    let base = format!("Freehold Ledger {year} export");
    let first = parent.join(&base);
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|n| parent.join(format!("{base} ({n})")))
        .find(|p| !p.exists())
        .expect("an unused folder name")
}

impl Ledger {
    /// Writes `Freehold Ledger {year} export` under `dest_parent` (default: Documents/Freehold Ledger/exports).
    /// Everything is written to a temporary folder first and renamed into place, so a failure leaves nothing.
    pub fn export_tax_year(&self, year: i32, dest_parent: Option<&Path>) -> Result<ExportResult> {
        let invalid = || LedgerError::InvalidDate(format!("{year}"));
        let from = NaiveDate::from_ymd_opt(year, 1, 1).ok_or_else(invalid)?;
        let to = NaiveDate::from_ymd_opt(year, 12, 31).ok_or_else(invalid)?;
        let parent = match dest_parent {
            Some(p) => p.to_path_buf(),
            None => dirs::document_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("Freehold Ledger")
                .join("exports"),
        };

        let mut rows = self.list(&Filter { from: Some(from), to: Some(to), include_voided: true, ..Default::default() })?;
        rows.sort_by_key(|t| (t.occurred_on, t.id));
        let summary = self.summarize(&SummaryRequest { from, to, category: None, source: None, is_asset: None, payment_method: None })?;
        let schema_version = self.info()?.schema_version;
        let files = build(&rows);

        std::fs::create_dir_all(&parent).map_err(|e| fail(&parent, e))?;
        let tmp = parent.join(format!(
            ".freehold-export-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir(&tmp).map_err(|e| fail(&parent, e))?;
        let written = write_all(&tmp, year, &rows, &files, &summary, schema_version).and_then(|names| {
            let dest = pick_folder(&parent, year);
            std::fs::rename(&tmp, &dest).map_err(|e| fail(&dest, e))?;
            Ok((dest, names))
        });
        match written {
            Ok((folder, names)) => Ok(ExportResult { folder, files: names, summary }),
            Err(e) => {
                let _ = std::fs::remove_dir_all(&tmp);
                Err(e)
            }
        }
    }
}

fn write_all(
    dir: &Path,
    year: i32,
    rows: &[Transaction],
    f: &Files,
    summary: &Summary,
    schema_version: u32,
) -> Result<Vec<String>> {
    csv_file(
        &dir.join("transactions.csv"),
        &[
            "id", "date", "kind", "description", "amount", "amount_cents", "category_key", "category_label",
            "schedule_c_line", "is_asset", "income_source", "status", "voided_reason", "replaces_id",
            "replaced_by_id", "original_category_key", "recorded_at",
        ],
        &f.transactions,
    )?;
    csv_file(
        &dir.join("expenses-by-category.csv"),
        &["schedule_c_line", "category_key", "category_label", "total", "total_cents", "count"],
        &f.by_category,
    )?;
    csv_file(
        &dir.join("income-by-source.csv"),
        &["income_source", "category_key", "total", "total_cents", "count"],
        &f.by_source,
    )?;
    csv_file(
        &dir.join("assets.csv"),
        &["id", "date", "description", "amount", "category_key", "schedule_c_line"],
        &f.assets,
    )?;
    csv_file(
        &dir.join("corrections.csv"),
        &["transaction_id", "action", "detail", "reason", "recorded_at", "replacement_id"],
        &f.corrections,
    )?;

    let mut j = serde_json::to_value(summary).expect("summary serializes");
    if let Value::Object(o) = &mut j {
        o.remove("by_payment_method");
        o.insert("generated_at".into(), chrono::Utc::now().to_rfc3339().into());
        o.insert("app_version".into(), env!("CARGO_PKG_VERSION").into());
        o.insert("schema_version".into(), schema_version.into());
    }
    let p = dir.join("summary.json");
    std::fs::write(&p, serde_json::to_string_pretty(&j).expect("json") + "\n").map_err(|e| fail(&p, e))?;
    let p = dir.join("README.txt");
    std::fs::write(&p, readme(year, summary, f, rows.is_empty())).map_err(|e| fail(&p, e))?;
    Ok(vec![
        "transactions.csv",
        "expenses-by-category.csv",
        "income-by-source.csv",
        "assets.csv",
        "corrections.csv",
        "summary.json",
        "README.txt",
    ]
    .into_iter()
    .map(String::from)
    .collect())
}
