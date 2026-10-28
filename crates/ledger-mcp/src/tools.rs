//! One MCP tool per ledger operation. A thin translation layer: every rule lives in `ledger-core`.

use ledger_core::{Ledger, LedgerError};
use rmcp::{
    handler::server::common::schema_for_input,
    model::{CallToolResult, ContentBlock, JsonObject},
    schemars, tool, tool_handler, tool_router, ServerHandler,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct LedgerTools {
    /// The open ledger, or the plain-language reason it could not be opened.
    ledger: Arc<Mutex<Result<Ledger, String>>>,
}

impl LedgerTools {
    pub fn new(ledger: Result<Ledger, String>) -> Self {
        Self { ledger: Arc::new(Mutex::new(ledger)) }
    }
}

/// What can go wrong inside a tool. Every variant becomes an `isError: true` tool result.
enum ToolError {
    Ledger(LedgerError),
    /// The ledger could not be opened at startup; carries the `LedgerError` text.
    Unavailable(String),
    /// The input itself was unusable before the ledger was consulted.
    Input(String),
}

impl From<LedgerError> for ToolError {
    fn from(e: LedgerError) -> Self {
        ToolError::Ledger(e)
    }
}

impl ToolError {
    fn message(&self) -> String {
        match self {
            ToolError::Ledger(e) => e.to_string(),
            ToolError::Unavailable(m) | ToolError::Input(m) => m.clone(),
        }
    }
}

type ToolResult = Result<Value, ToolError>;

fn respond(r: ToolResult) -> CallToolResult {
    match r {
        Ok(v) => CallToolResult::success(vec![ContentBlock::text(v.to_string())]),
        Err(e) => CallToolResult::error(vec![ContentBlock::text(e.message())]),
    }
}

impl LedgerTools {
    fn with_ledger<T>(&self, f: impl FnOnce(&mut Ledger) -> Result<T, ToolError>) -> Result<T, ToolError> {
        let mut guard = self.ledger.lock().unwrap_or_else(|p| p.into_inner());
        match &mut *guard {
            Ok(l) => f(l),
            Err(msg) => Err(ToolError::Unavailable(msg.clone())),
        }
    }
}

/// Parse the raw tool arguments into `T`, turning a serde failure into a readable tool error.
fn parse_args<T: for<'de> Deserialize<'de>>(args: JsonObject) -> Result<T, ToolError> {
    serde_json::from_value(Value::Object(args))
        .map_err(|e| ToolError::Input(format!("Could not read the tool input: {e}.")))
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ListCategoriesInput {
    /// Only categories for this kind of entry: "expense" or "income". Omit for both.
    kind: Option<KindArg>,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum KindArg {
    Expense,
    Income,
}

impl From<KindArg> for ledger_core::Kind {
    fn from(k: KindArg) -> Self {
        match k {
            KindArg::Expense => ledger_core::Kind::Expense,
            KindArg::Income => ledger_core::Kind::Income,
        }
    }
}


/// Dollars as the wire carries them: a decimal string (`"300"`, `"$1,200.50"`) or a JSON number.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(untagged)]
enum AmountArg {
    Text(String),
    Number(serde_json::Number),
}

/// Convert dollars to integer cents exactly. Zero and negatives are passed through as
/// non-positive cents so `ledger-core` rejects them with its own message.
fn dollars_to_cents(a: &AmountArg) -> Result<i64, ToolError> {
    let bad = |why: &str| ToolError::Input(format!("{why} Amounts are dollars, for example \"300\" or \"300.50\"."));
    let raw = match a {
        AmountArg::Text(s) => s.trim().to_string(),
        AmountArg::Number(n) => n.to_string(),
    };
    let (negative, body) = match raw.strip_prefix('-') {
        Some(rest) => (true, rest.trim_start()),
        None => (false, raw.as_str()),
    };
    let body = body.strip_prefix('$').unwrap_or(body).trim_start();
    let (int_part, frac_part) = match body.split_once('.') {
        Some((i, f)) => (i, f),
        None => (body, ""),
    };
    let grouped = int_part.len() > 3
        && int_part.contains(',')
        && int_part.split(',').enumerate().all(|(i, g)| {
            !g.is_empty() && g.bytes().all(|b| b.is_ascii_digit()) && if i == 0 { g.len() <= 3 } else { g.len() == 3 }
        });
    let int_digits: String = if grouped { int_part.replace(',', "") } else { int_part.to_string() };
    let all_digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if int_digits.is_empty() && frac_part.is_empty() || !all_digits(&int_digits) || !all_digits(frac_part) {
        return Err(bad("That is not an amount I can read."));
    }
    if frac_part.len() > 2 {
        return Err(ToolError::Input(
            "Amounts have at most 2 decimal places (whole cents), for example \"300.50\".".into(),
        ));
    }
    if int_digits.len() > 13 {
        return Err(bad("That amount is too large."));
    }
    let dollars: i64 = if int_digits.is_empty() { 0 } else { int_digits.parse().map_err(|_| bad("That is not an amount I can read."))? };
    let cents_frac: i64 = format!("{frac_part:0<2}").parse().unwrap_or(0);
    let cents = dollars * 100 + cents_frac;
    Ok(if negative { -cents } else { cents })
}

fn format_dollars(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.unsigned_abs();
    let whole = (abs / 100).to_string();
    let mut grouped = String::new();
    for (i, ch) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    format!("{sign}${grouped}.{:02}", abs % 100)
}

fn parse_date_arg(d: &Option<String>) -> Result<Option<chrono::NaiveDate>, ToolError> {
    d.as_deref().map(|s| ledger_core::parse_date(s).map_err(ToolError::from)).transpose()
}

/// A transaction as JSON, with a formatted `amount` beside `amount_cents`.
fn txn_json(t: &ledger_core::Transaction) -> Value {
    let mut v = serde_json::to_value(t).expect("transaction serializes");
    v["amount"] = Value::String(format_dollars(t.amount_cents));
    v
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RecordExpenseInput {
    /// Dollars spent, as a decimal string or number: "300", "$1,200.50" or 19.99. Whole cents only; must be more than zero.
    amount: AmountArg,
    /// Short plain-language description of what was bought or paid for.
    description: String,
    /// Expense category key, for example "supplies" or "travel". Call list_categories if unsure; use "uncategorized" to decide later.
    category: String,
    /// Date the money was spent, YYYY-MM-DD. Omit for today.
    date: Option<String>,
    /// True for equipment or property kept longer than a year (a laptop, a camera, a tool). Defaults to false.
    is_asset: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RecordIncomeInput {
    /// Dollars received, as a decimal string or number: "1200", "$1,200.50" or 49.99. Whole cents only; must be more than zero.
    amount: AmountArg,
    /// Short plain-language description of what the money was for.
    description: String,
    /// Who paid: the client or platform, for example "Upwork". The same source in any letter case is one source.
    source: String,
    /// Income category key: "gross_receipts" (default, Schedule C line 1) or "other_income" (line 6).
    category: Option<String>,
    /// Date the money was received, YYYY-MM-DD. Omit for today.
    date: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ListTransactionsInput {
    /// First date to include, YYYY-MM-DD (inclusive).
    from: Option<String>,
    /// Last date to include, YYYY-MM-DD (inclusive).
    to: Option<String>,
    /// Only "expense" or "income" entries.
    kind: Option<KindArg>,
    /// Only entries whose current category is this key (see list_categories).
    category: Option<String>,
    /// Only income from this source (case-insensitive).
    source: Option<String>,
    /// Only assets (true) or only non-assets (false).
    is_asset: Option<bool>,
    /// Also list voided entries, each with its void reason. Defaults to false.
    include_voided: Option<bool>,
    /// Most entries to return, newest first. Defaults to 50; the maximum is 500.
    limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ReclassifyInput {
    /// Id of the transaction to reclassify.
    id: i64,
    /// The new category key; must match the transaction's kind (see list_categories).
    category: String,
    /// Short reason for the change; it is kept in the history.
    reason: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct VoidInput {
    /// Id of the transaction to void.
    id: i64,
    /// Short reason it is being voided; it is kept in the history.
    reason: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CorrectInput {
    /// Id of the transaction to correct.
    id: i64,
    /// Short reason for the correction; it is kept in the history.
    reason: String,
    /// New amount in dollars, as a decimal string or number.
    amount: Option<AmountArg>,
    /// New date, YYYY-MM-DD.
    date: Option<String>,
    /// New description.
    description: Option<String>,
    /// New category key; must match the transaction's kind.
    category: Option<String>,
    /// New asset flag (expenses only).
    is_asset: Option<bool>,
    /// New income source (income only).
    source: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum PeriodArg {
    ThisMonth,
    LastMonth,
    ThisQuarter,
    LastQuarter,
    YearToDate,
    ThisYear,
    LastYear,
}

impl From<PeriodArg> for ledger_core::Period {
    fn from(p: PeriodArg) -> Self {
        use ledger_core::Period as P;
        match p {
            PeriodArg::ThisMonth => P::ThisMonth,
            PeriodArg::LastMonth => P::LastMonth,
            PeriodArg::ThisQuarter => P::ThisQuarter,
            PeriodArg::LastQuarter => P::LastQuarter,
            PeriodArg::YearToDate => P::YearToDate,
            PeriodArg::ThisYear => P::ThisYear,
            PeriodArg::LastYear => P::LastYear,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SummarizeInput {
    /// A named period: this_month, last_month, this_quarter, last_quarter, year_to_date, this_year or last_year (calendar quarters). Use this or from+to, not both.
    period: Option<PeriodArg>,
    /// First date of a custom range, YYYY-MM-DD (inclusive). Give with `to`.
    from: Option<String>,
    /// Last date of a custom range, YYYY-MM-DD (inclusive). Give with `from`.
    to: Option<String>,
    /// Only entries whose current category is this key (see list_categories).
    category: Option<String>,
    /// Only income from this source (case-insensitive).
    source: Option<String>,
    /// True for only assets (equipment kept over a year), false for only non-assets.
    is_asset: Option<bool>,
}

/// Add a dollar-formatted sibling for every `*_cents` number, for example `profit_cents` -> `profit`.
fn add_dollars(v: &mut Value) {
    match v {
        Value::Object(m) => {
            let extra: Vec<(String, Value)> = m
                .iter()
                .filter_map(|(k, x)| {
                    let name = k.strip_suffix("_cents")?;
                    Some((name.to_string(), Value::String(format_dollars(x.as_i64()?))))
                })
                .collect();
            for x in m.values_mut() {
                add_dollars(x);
            }
            m.extend(extra);
        }
        Value::Array(a) => a.iter_mut().for_each(add_dollars),
        _ => {}
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ExportInput {
    /// The calendar year to export, for example 2026.
    year: i32,
    /// Absolute path of the folder to create the export inside. Omit for Documents/Freehold Ledger/exports.
    destination: Option<String>,
}

fn with_note(mut v: Value, note: impl Into<String>) -> Value {
    v["note"] = Value::String(note.into());
    v
}

#[tool_router]
impl LedgerTools {
    #[tool(
        description = "Show which ledger file is open, its schema version and how many transactions it holds. Call this first to confirm the books are reachable.",
        annotations(read_only_hint = true)
    )]
    fn ledger_info(&self) -> CallToolResult {
        respond(self.with_ledger(|l| {
            let info = l.info()?;
            Ok(with_note(
                serde_json::to_value(info).expect("info serializes"),
                "This one file holds all the books; amounts are US dollars and nothing is ever deleted, only voided or corrected.",
            ))
        }))
    }

    #[tool(
        description = "List the valid category keys with their labels and IRS Schedule C lines. Call this when unsure which category fits an expense or income.",
        input_schema = schema_for_input::<ListCategoriesInput>().expect("schema"),
        annotations(read_only_hint = true)
    )]
    fn list_categories(&self, args: JsonObject) -> CallToolResult {
        respond((|| {
            let input: ListCategoriesInput = parse_args(args)?;
            let kind: Option<ledger_core::Kind> = input.kind.map(Into::into);
            let cats: Vec<_> = ledger_core::categories()
                .iter()
                .filter(|c| kind.is_none_or(|k| c.kind == k))
                .collect();
            Ok(with_note(
                json!({ "categories": cats }),
                "Expense categories are the IRS Schedule C Part II lines, so totals map straight onto the tax form; use uncategorized when unsure and reclassify later.",
            ))
        })())
    }

    #[tool(
        description = "Record money the business spent: a purchase, bill, or fee. Use for 'I bought...', 'I paid...'. Set is_asset for equipment or property kept longer than a year (a laptop, a camera, a tool).",
        input_schema = schema_for_input::<RecordExpenseInput>().expect("schema")
    )]
    fn record_expense(&self, args: JsonObject) -> CallToolResult {
        respond((|| {
            let i: RecordExpenseInput = parse_args(args)?;
            let new = ledger_core::NewExpense {
                occurred_on: parse_date_arg(&i.date)?,
                amount_cents: dollars_to_cents(&i.amount)?,
                description: i.description,
                category: i.category,
                is_asset: i.is_asset.unwrap_or(false),
            };
            let t = self.with_ledger(|l| Ok(l.record_expense(new)?))?;
            let note = if t.is_asset {
                "Marked as an asset: equipment kept over a year can often be deducted all at once or over several years; your tax preparer will choose, and the export lists assets separately.".to_string()
            } else if t.category == "uncategorized" {
                "Recorded as uncategorized: reclassify it once you know the category so it lands on the right Schedule C line.".to_string()
            } else {
                format!(
                    "Recorded as {}{}: it counts toward your deductions for the year.",
                    t.category_label,
                    t.schedule_c_line.as_deref().map(|l| format!(" (Schedule C line {l})")).unwrap_or_default()
                )
            };
            Ok(with_note(txn_json(&t), note))
        })())
    }
    #[tool(
        description = "Record money the business received: a payment, invoice, or sale. Use for 'a client paid me...', 'I got paid...'. Income needs a source (who paid), for example the client or platform.",
        input_schema = schema_for_input::<RecordIncomeInput>().expect("schema")
    )]
    fn record_income(&self, args: JsonObject) -> CallToolResult {
        respond((|| {
            let i: RecordIncomeInput = parse_args(args)?;
            let new = ledger_core::NewIncome {
                occurred_on: parse_date_arg(&i.date)?,
                amount_cents: dollars_to_cents(&i.amount)?,
                description: i.description,
                category: i.category,
                source: i.source,
            };
            let t = self.with_ledger(|l| Ok(l.record_income(new)?))?;
            let note = format!(
                "Recorded as {}{}: income is reported for the year it was received, and the same source name is reused each time.",
                t.category_label,
                t.schedule_c_line.as_deref().map(|l| format!(" (Schedule C line {l})")).unwrap_or_default()
            );
            Ok(with_note(txn_json(&t), note))
        })())
    }

    #[tool(
        description = "List ledger entries, newest first, optionally filtered by date range, kind, category, income source, or asset flag. Voided entries are hidden unless include_voided is true. Use to answer 'what did I spend on...', to find an id before correcting something, or to review before tax time.",
        input_schema = schema_for_input::<ListTransactionsInput>().expect("schema"),
        annotations(read_only_hint = true)
    )]
    fn list_transactions(&self, args: JsonObject) -> CallToolResult {
        respond((|| {
            let i: ListTransactionsInput = parse_args(args)?;
            let limit = i.limit.unwrap_or(50);
            if !(1..=500).contains(&limit) {
                return Err(ToolError::Input("limit must be between 1 and 500.".into()));
            }
            let filter = ledger_core::Filter {
                from: parse_date_arg(&i.from)?,
                to: parse_date_arg(&i.to)?,
                kind: i.kind.map(Into::into),
                category: i.category,
                source: i.source,
                is_asset: i.is_asset,
                include_voided: i.include_voided.unwrap_or(false),
                limit: Some(limit),
            };
            let rows = self.with_ledger(|l| Ok(l.list(&filter)?))?;
            let count = rows.len();
            let note = if count as u32 == limit {
                format!("Showing the newest {limit}; narrow the dates or filters, or raise limit, to see others.")
            } else if filter.include_voided {
                "Voided entries are included here, marked with their reason; they never count in totals.".to_string()
            } else {
                "Voided entries are hidden; set include_voided to see them with their reasons.".to_string()
            };
            let transactions: Vec<Value> = rows.iter().map(txn_json).collect();
            Ok(with_note(json!({ "transactions": transactions, "count": count }), note))
        })())
    }

    #[tool(
        description = "Change the category of an existing entry, for example from uncategorized to office_expense. The old category stays in the history. Use when the person says 'that should be filed under...'. For a wrong amount, date, or description use correct_transaction.",
        input_schema = schema_for_input::<ReclassifyInput>().expect("schema")
    )]
    fn reclassify_transaction(&self, args: JsonObject) -> CallToolResult {
        respond((|| {
            let i: ReclassifyInput = parse_args(args)?;
            let t = self.with_ledger(|l| Ok(l.reclassify(i.id, &i.category, &i.reason)?))?;
            Ok(with_note(
                txn_json(&t),
                "Reclassified, not rewritten: the original category stays in the history and the new one counts in totals.",
            ))
        })())
    }

    #[tool(
        description = "Void an entry that should not count, such as a duplicate or a mistake. It is never deleted: it stays in the history with your reason and is left out of totals. To fix an amount or date and keep the entry, use correct_transaction instead.",
        input_schema = schema_for_input::<VoidInput>().expect("schema")
    )]
    fn void_transaction(&self, args: JsonObject) -> CallToolResult {
        respond((|| {
            let i: VoidInput = parse_args(args)?;
            let t = self.with_ledger(|l| Ok(l.void(i.id, &i.reason)?))?;
            Ok(with_note(
                txn_json(&t),
                "Voided, not deleted: it stays in your history and no longer counts in totals.",
            ))
        })())
    }

    #[tool(
        description = "Fix a wrong amount, date, description, category, asset flag, or income source on an existing entry. The original is voided with your reason and a linked replacement is recorded in one step, so the history shows what changed. Give at least one field to change.",
        input_schema = schema_for_input::<CorrectInput>().expect("schema")
    )]
    fn correct_transaction(&self, args: JsonObject) -> CallToolResult {
        respond((|| {
            let i: CorrectInput = parse_args(args)?;
            if i.amount.is_none()
                && i.date.is_none()
                && i.description.is_none()
                && i.category.is_none()
                && i.is_asset.is_none()
                && i.source.is_none()
            {
                return Err(ToolError::Input(
                    "Say what to change: give at least one of amount, date, description, category, is_asset or source.".into(),
                ));
            }
            let r = ledger_core::Replacement {
                occurred_on: parse_date_arg(&i.date)?,
                amount_cents: i.amount.as_ref().map(dollars_to_cents).transpose()?,
                description: i.description,
                category: i.category,
                is_asset: i.is_asset,
                source: i.source,
            };
            let c = self.with_ledger(|l| Ok(l.correct(i.id, &i.reason, r)?))?;
            Ok(with_note(
                json!({ "voided": txn_json(&c.voided), "replacement": txn_json(&c.replacement) }),
                "Corrected by voiding the original and recording a linked replacement, so the history shows exactly what changed and why.",
            ))
        })())
    }

    #[tool(
        description = "Totals for a period: spending by tax category, income by source, profit. Use for 'what did I spend...', 'how much came in from...', 'how am I doing this quarter'. For equipment purchases use is_asset=true.",
        input_schema = schema_for_input::<SummarizeInput>().expect("schema"),
        annotations(read_only_hint = true)
    )]
    fn summarize(&self, args: JsonObject) -> CallToolResult {
        respond((|| {
            let i: SummarizeInput = parse_args(args)?;
            let (from, to) = match (i.period, parse_date_arg(&i.from)?, parse_date_arg(&i.to)?) {
                (Some(p), None, None) => {
                    ledger_core::resolve_period(p.into(), chrono::Local::now().date_naive())
                }
                (None, Some(f), Some(t)) => (f, t),
                _ => {
                    return Err(ToolError::Input(
                        "Give either period (for example this_quarter) or both from and to dates (YYYY-MM-DD), not a mix.".into(),
                    ))
                }
            };
            let req = ledger_core::SummaryRequest {
                from,
                to,
                category: i.category,
                source: i.source,
                is_asset: i.is_asset,
            };
            let s = self.with_ledger(|l| Ok(l.summarize(&req)?))?;
            let mut v = serde_json::to_value(&s).expect("summary serializes");
            add_dollars(&mut v);
            let mut note = format!(
                "From {} to {}: you took in {} and spent {}, leaving {} profit before taxes; voided entries are left out.",
                s.from,
                s.to,
                format_dollars(s.income_total_cents),
                format_dollars(s.expenses_total_cents),
                format_dollars(s.profit_cents)
            );
            if s.uncategorized.count > 0 {
                note = format!(
                    "{} entries still need a category; ask me to classify them before tax time. {note}",
                    s.uncategorized.count
                );
            }
            Ok(with_note(v, note))
        })())
    }

    #[tool(
        description = "Write a tax-year export folder for a preparer or another AI: categorized expenses on Schedule C lines, income by source, assets, the full correction trail, and a summary. Use for 'export my books', 'get my taxes ready'.",
        input_schema = schema_for_input::<ExportInput>().expect("schema")
    )]
    fn export_tax_year(&self, args: JsonObject) -> CallToolResult {
        respond((|| {
            let i: ExportInput = parse_args(args)?;
            let dest = match i.destination.as_deref().map(str::trim) {
                Some("") | None => None,
                Some(d) => {
                    let p = std::path::PathBuf::from(d);
                    if !p.is_absolute() {
                        return Err(ToolError::Input(format!(
                            "destination must be a full folder path starting from the root, for example /Users/you/Documents; \"{d}\" is relative. Omit it to use the default exports folder."
                        )));
                    }
                    Some(p)
                }
            };
            let r = self.with_ledger(|l| Ok(l.export_tax_year(i.year, dest.as_deref())?))?;
            let folder = r.folder.display().to_string();
            let note = format!(
                "Your {} books are in {folder}. Hand the whole folder to your preparer; README.txt explains every file.",
                i.year
            );
            Ok(with_note(
                json!({
                    "folder": folder,
                    "files": r.files,
                    "totals": serde_json::to_value(&r.summary).expect("summary serializes"),
                }),
                note,
            ))
        })())
    }
}

#[tool_handler(
    name = "freehold-ledger",
    instructions = "Freehold Ledger is the person's own bookkeeping file for a small business: expenses and income, kept for tax time. Amounts are US dollars (for example \"300\" or \"300.50\"); dates are YYYY-MM-DD and default to today. Nothing is ever deleted: mistakes are voided or corrected, and the history stays. Call list_categories when unsure of a category."
)]
impl ServerHandler for LedgerTools {}
