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

/// D30: refuse a full card or account number anywhere in a tool's input, before it is read,
/// stored or echoed. Field names in the message follow the input ("description",
/// "description of entry 3").
fn scan_for_full_numbers(args: &JsonObject) -> Result<(), ToolError> {
    fn walk(v: &Value, field: &str) -> Result<(), ToolError> {
        match v {
            Value::String(s) => ledger_core::check_no_full_number(field, s).map_err(ToolError::from),
            Value::Number(n) => ledger_core::check_no_full_number(field, &n.to_string()).map_err(ToolError::from),
            Value::Array(a) => a.iter().enumerate().try_for_each(|(i, x)| {
                let f = if field == "entries" { format!("entry {}", i + 1) } else { field.to_string() };
                walk(x, &f)
            }),
            // Keys are scanned before they can become a field label or a serde "unknown field" echo.
            Value::Object(m) => m.iter().try_for_each(|(k, x)| {
                let key_field = if field.is_empty() { "field name".to_string() } else { format!("field name in {field}") };
                ledger_core::check_no_full_number(&key_field, k).map_err(ToolError::from)?;
                let f = if field.is_empty() { k.clone() } else { format!("{k} of {field}") };
                walk(x, &f)
            }),
            _ => Ok(()),
        }
    }
    walk(&Value::Object(args.clone()), "")
}

/// Parse the raw tool arguments into `T`, turning a serde failure into a readable tool error.
fn parse_args<T: for<'de> Deserialize<'de>>(args: JsonObject) -> Result<T, ToolError> {
    scan_for_full_numbers(&args)?;
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

/// A category used as a filter must be a real key: an unknown one would silently match nothing
/// and read as "you spent $0 on that".
fn check_filter_category(c: &Option<String>) -> Result<(), ToolError> {
    match c.as_deref() {
        Some(k) if ledger_core::category(k).is_none() => {
            let keys: Vec<&str> = ledger_core::categories().iter().map(|c| c.key).collect();
            Err(ToolError::Input(format!(
                "\"{k}\" is not a category key. Valid categories: {}. Call list_categories for labels and Schedule C lines.",
                keys.join(", ")
            )))
        }
        _ => Ok(()),
    }
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
struct RecordTransactionsInput {
    /// Short label for this batch, for example the file name the rows came from ("bank-export-jan.csv"). Kept so the batch can be found and undone later.
    label: String,
    /// 1 to 50 entries, in file order.
    entries: Vec<BatchEntryInput>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct BatchEntryInput {
    /// "expense" or "income".
    kind: KindArg,
    /// Dollars, more than zero, for example "18.17". A refund is not a negative amount: correct the original purchase with correct_transaction.
    amount: AmountArg,
    /// What it was, as written in the file is fine (for example "GODADDY.COM").
    description: String,
    /// Date of the entry, YYYY-MM-DD. Required: an import never defaults to today.
    date: String,
    /// Category key. Required for expenses; for income "gross_receipts" (default) or "other_income".
    category: Option<String>,
    /// Expenses only: equipment kept over a year.
    is_asset: Option<bool>,
    /// Income only: who paid.
    source: Option<String>,
    /// Record this entry even if it matches one already in the books (a real second charge). Defaults to false.
    allow_duplicate: Option<bool>,
}

/// An entry-level failure in the same words core uses for its own `BatchEntry` errors.
fn entry_error(n: usize, message: &str) -> ToolError {
    ToolError::Input(format!("Entry {n}: {message} Nothing in this batch was recorded."))
}

fn batch_entry(n: usize, e: BatchEntryInput) -> Result<ledger_core::BatchEntry, ToolError> {
    let occurred_on = ledger_core::parse_date(&e.date)
        .map_err(|err| ToolError::Ledger(LedgerError::BatchEntry { index: n, reason: Box::new(err) }))?;
    let amount_cents = dollars_to_cents(&e.amount).map_err(|err| entry_error(n, &err.message()))?;
    if amount_cents <= 0 {
        return Err(entry_error(
            n,
            "Amount must be more than $0.00. A refund or return is recorded by correcting the original purchase with correct_transaction, not as a negative amount.",
        ));
    }
    let entry = match e.kind {
        KindArg::Expense => {
            let Some(category) = e.category else {
                return Err(entry_error(n, "Expenses need a category key; use uncategorized to decide later."));
            };
            if e.source.is_some() {
                return Err(entry_error(n, "source is for income only."));
            }
            ledger_core::NewEntry::Expense(ledger_core::NewExpense {
                occurred_on: Some(occurred_on),
                amount_cents,
                description: e.description,
                category,
                is_asset: e.is_asset.unwrap_or(false),
            })
        }
        KindArg::Income => {
            if e.is_asset == Some(true) {
                return Err(entry_error(n, "is_asset is for expenses only."));
            }
            ledger_core::NewEntry::Income(ledger_core::NewIncome {
                occurred_on: Some(occurred_on),
                amount_cents,
                description: e.description,
                category: e.category,
                source: e.source.unwrap_or_default(),
            })
        }
    };
    Ok(ledger_core::BatchEntry { entry, allow_duplicate: e.allow_duplicate.unwrap_or(false) })
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
    /// Only entries recorded in this batch (the batch_id from record_transactions).
    batch_id: Option<i64>,
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
struct VoidBatchInput {
    /// The batch_id from record_transactions.
    batch_id: i64,
    /// Short reason, kept on every voided entry, for example "imported the wrong file".
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

/// A payment method as the wire carries it: its id (a number) or its nickname (text, any letter case).
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(untagged)]
enum PaymentMethodArg {
    Id(i64),
    Nickname(String),
}

impl From<PaymentMethodArg> for ledger_core::PaymentMethodKey {
    fn from(a: PaymentMethodArg) -> Self {
        match a {
            PaymentMethodArg::Id(n) => ledger_core::PaymentMethodKey::Id(n),
            PaymentMethodArg::Nickname(s) => ledger_core::PaymentMethodKey::Nickname(s),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum PaymentKindArg {
    Card,
    BankAccount,
    Cash,
    Other,
}

impl From<PaymentKindArg> for ledger_core::PaymentKind {
    fn from(k: PaymentKindArg) -> Self {
        match k {
            PaymentKindArg::Card => ledger_core::PaymentKind::Card,
            PaymentKindArg::BankAccount => ledger_core::PaymentKind::BankAccount,
            PaymentKindArg::Cash => ledger_core::PaymentKind::Cash,
            PaymentKindArg::Other => ledger_core::PaymentKind::Other,
        }
    }
}

/// "Business Visa (card ending 4242)", "Cash (cash)". Used by every note that names a method.
fn describe_method(nickname: &str, kind: ledger_core::PaymentKind, last4: Option<&str>) -> String {
    format!(
        "{nickname} ({}{})",
        kind.label().to_lowercase(),
        last4.map(|l| format!(" ending {l}")).unwrap_or_default()
    )
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CreatePaymentMethodInput {
    /// "card", "bank_account", "cash" or "other".
    kind: PaymentKindArg,
    /// Short name the person uses, for example "Business Visa" or "Chase checking". Unique among payment methods in use, any letter case.
    nickname: String,
    /// Only the last 4 digits of the card or account number, for example "4242". Never send a full number. Leave out for cash.
    last4: Option<String>,
    /// Use this payment method for new expenses when none is named. Defaults to false.
    make_default: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ListPaymentMethodsInput {
    /// Also list retired payment methods. Defaults to false.
    include_retired: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RenamePaymentMethodInput {
    /// The payment method to rename: its id or its nickname.
    payment_method: PaymentMethodArg,
    /// The new nickname.
    nickname: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RetirePaymentMethodInput {
    /// The payment method to retire: its id or its nickname.
    payment_method: PaymentMethodArg,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SetDefaultPaymentMethodInput {
    /// The payment method new expenses should use when none is named.
    payment_method: Option<PaymentMethodArg>,
    /// Set true to have no default.
    clear: Option<bool>,
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
        description = "Record money the business spent: a purchase, bill, or fee. Use for 'I bought...', 'I paid...'. Record it right away with sensible defaults (today's date, the most likely category) and tell the person your choices afterward; do not ask first. A device, computer, camera or tool the business will keep longer than a year is equipment: set is_asset=true with category depreciation_section_179, and mention that supplies is the alternative their preparer may prefer for small items. Use uncategorized only when the person says to decide later. For several rows from a statement or file, use record_transactions instead: it skips entries already in the books.",
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
            // The agent records with defaults instead of asking first, so every note names the
            // date and category that were actually stored, for the person to correct.
            let date = t.occurred_on;
            let filed_under = format!(
                "{}{}",
                t.category_label,
                t.schedule_c_line.as_deref().map(|l| format!(" (Schedule C line {l})")).unwrap_or_default()
            );
            let note = if t.is_asset {
                format!("Recorded on {date} and marked as an asset under {filed_under}: equipment kept over a year can often be deducted all at once or over several years; your tax preparer will choose, and the export lists assets separately.")
            } else if t.category == "uncategorized" {
                format!("Recorded on {date} as uncategorized: reclassify it once you know the category so it lands on the right Schedule C line.")
            } else {
                format!("Recorded on {date} as {filed_under}: it counts toward your deductions for the year.")
            };
            Ok(with_note(txn_json(&t), note))
        })())
    }
    #[tool(
        description = "Record money the business received: a payment, invoice, or sale. Use for 'a client paid me...', 'I got paid...'. Income needs a source (who paid), for example the client or platform. For several rows from a statement or file, use record_transactions instead: it skips entries already in the books.",
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
        description = "Record several expenses and income at once, for example rows from a bank export or a spreadsheet: up to 50 entries per call, all or nothing (one bad entry records none and names it). Each entry needs kind, amount, description and date; expenses need a category, income needs a source. An entry that matches a live entry already in the books (same date, kind, amount and description, any letter case) is skipped and reported, so importing the same file twice or two overlapping exports does not count anything twice; set allow_duplicate on an entry only when it really is a second charge. Give the batch a label such as the file name. A refund is not a negative amount: correct the original purchase with correct_transaction.",
        input_schema = schema_for_input::<RecordTransactionsInput>().expect("schema")
    )]
    fn record_transactions(&self, args: JsonObject) -> CallToolResult {
        respond((|| {
            let i: RecordTransactionsInput = parse_args(args)?;
            if i.entries.is_empty() {
                return Err(LedgerError::EmptyBatch.into());
            }
            if i.entries.len() > ledger_core::MAX_BATCH {
                return Err(LedgerError::BatchTooLarge { count: i.entries.len() }.into());
            }
            let entries = i
                .entries
                .into_iter()
                .enumerate()
                .map(|(idx, e)| batch_entry(idx + 1, e))
                .collect::<Result<Vec<_>, _>>()?;
            let r = self.with_ledger(|l| Ok(l.record_batch(ledger_core::NewBatch { label: i.label, entries })?))?;
            let recorded: Vec<Value> = r
                .recorded
                .iter()
                .map(|x| {
                    let mut v = txn_json(&x.transaction);
                    v["entry"] = json!(x.entry);
                    v
                })
                .collect();
            let mut skipped: Vec<Value> = Vec::new();
            for s in &r.skipped_duplicates {
                let m = self.with_ledger(|l| Ok(l.get(s.matches_id)?))?;
                skipped.push(json!({
                    "entry": s.entry,
                    "matches_id": s.matches_id,
                    "date": m.occurred_on,
                    "amount": format_dollars(m.amount_cents),
                    "description": m.description,
                }));
            }
            let (rc, sc) = (recorded.len(), skipped.len());
            let note = match r.batch_id {
                Some(id) => {
                    let base = format!(
                        "Recorded {rc} entries as batch {id} (\"{}\"). Each one shows the date and category it was filed under; reclassify or correct any you disagree with.",
                        r.label
                    );
                    let base = format!("{base} To undo this whole batch, call void_batch with batch_id {id}.");
                    if sc == 0 {
                        base
                    } else {
                        format!("{base} {sc} looked like entries already in the books (same date, kind, amount and description) and were not recorded; if one really is a second charge, send it again with allow_duplicate set to true.")
                    }
                }
                None => format!("Nothing new was recorded: all {sc} entries match entries already in the books (same date, kind, amount and description). If one really is a second charge, send it again with allow_duplicate set to true."),
            };
            Ok(with_note(
                json!({
                    "batch_id": r.batch_id,
                    "label": r.label,
                    "recorded_count": rc,
                    "skipped_count": sc,
                    "recorded": recorded,
                    "skipped_duplicates": skipped,
                }),
                note,
            ))
        })())
    }

    #[tool(
        description = "List ledger entries, newest first, optionally filtered by date range, kind, category, income source, or asset flag. Voided entries are hidden unless include_voided is true. Use to answer 'what did I spend on...', to find an id before correcting something, or to review before tax time. Filter by batch_id to see one import.",
        input_schema = schema_for_input::<ListTransactionsInput>().expect("schema"),
        annotations(read_only_hint = true)
    )]
    fn list_transactions(&self, args: JsonObject) -> CallToolResult {
        respond((|| {
            let i: ListTransactionsInput = parse_args(args)?;
            check_filter_category(&i.category)?;
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
                batch_id: i.batch_id,
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
        description = "Undo a whole import in one call: void every entry recorded by one record_transactions batch, including replacements made by correcting those entries. Nothing is deleted: each entry stays in the history with your reason and leaves the totals. Entries already voided are skipped and counted. Use when the person says a file was imported by mistake or twice; list_transactions with batch_id shows what it holds.",
        input_schema = schema_for_input::<VoidBatchInput>().expect("schema")
    )]
    fn void_batch(&self, args: JsonObject) -> CallToolResult {
        respond((|| {
            let i: VoidBatchInput = parse_args(args)?;
            let r = self.with_ledger(|l| Ok(l.void_batch(i.batch_id, &i.reason)?))?;
            let (n, m) = (r.voided.len(), r.already_voided_ids.len());
            let note = if n > 0 {
                let base = format!(
                    "Voided {n} entries from batch {} (\"{}\"), not deleted: they stay in your history with the reason and no longer count in totals.",
                    r.batch_id, r.label
                );
                if m > 0 {
                    format!("{base} {m} were already voided and were left as they were.")
                } else {
                    base
                }
            } else {
                format!("Nothing left to void in batch {} (\"{}\"): all {m} entries were already voided.", r.batch_id, r.label)
            };
            Ok(with_note(
                json!({
                    "batch_id": r.batch_id,
                    "label": r.label,
                    "voided_count": n,
                    "already_voided_count": m,
                    "voided": r.voided.iter().map(txn_json).collect::<Vec<Value>>(),
                    "already_voided_ids": r.already_voided_ids,
                }),
                note,
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
            check_filter_category(&i.category)?;
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

    #[tool(
        description = "Add a card, bank account, cash, or other way the business pays or gets paid, under a nickname such as \"Business Visa\", so entries can say what they were paid with. Never ask for or send a full card or account number, expiry date or security code: give only the last 4 digits as last4, if any. Set make_default to use it automatically for new expenses.",
        input_schema = schema_for_input::<CreatePaymentMethodInput>().expect("schema")
    )]
    fn create_payment_method(&self, args: JsonObject) -> CallToolResult {
        respond((|| {
            let i: CreatePaymentMethodInput = parse_args(args)?;
            let new = ledger_core::NewPaymentMethod {
                kind: i.kind.into(),
                nickname: i.nickname,
                last4: i.last4,
                make_default: i.make_default.unwrap_or(false),
            };
            let (p, default) = self.with_ledger(|l| {
                let p = l.create_payment_method(new)?;
                let d = l.default_payment_method()?;
                Ok((p, d))
            })?;
            let tail = if p.is_default {
                " It is now the default for new expenses.".to_string()
            } else if let Some(d) = default {
                format!(" {} stays the default for new expenses.", d.nickname)
            } else {
                " There is no default yet; ask to make one the default and new expenses will use it.".to_string()
            };
            let note = format!(
                "Saved {}. Only the last 4 digits are kept, never a full number.{tail}",
                describe_method(&p.nickname, p.kind, p.last4.as_deref())
            );
            Ok(with_note(serde_json::to_value(&p).expect("payment method serializes"), note))
        })())
    }

    #[tool(
        description = "List the cards, bank accounts, cash and other payment methods with their nicknames, last 4 digits, which one is the default, and how many live entries use each. Retired ones are hidden unless include_retired is true. Call this to find a nickname or id before assigning or recording.",
        input_schema = schema_for_input::<ListPaymentMethodsInput>().expect("schema"),
        annotations(read_only_hint = true)
    )]
    fn list_payment_methods(&self, args: JsonObject) -> CallToolResult {
        respond((|| {
            let i: ListPaymentMethodsInput = parse_args(args)?;
            let list = self.with_ledger(|l| Ok(l.payment_methods(i.include_retired.unwrap_or(false))?))?;
            let n = list.len();
            let note = if n == 0 {
                "No payment methods yet. Create one with create_payment_method, for example a card nicknamed \"Business Visa\" with its last 4 digits.".to_string()
            } else {
                let default = match list.iter().find(|p| p.is_default) {
                    Some(d) => format!("Default for new expenses: {}.", d.nickname),
                    None => "No default is set.".to_string(),
                };
                format!("{n} payment methods. {default} Only nicknames and last 4 digits are kept, never full numbers.")
            };
            Ok(with_note(json!({ "payment_methods": list, "count": n }), note))
        })())
    }

    #[tool(
        description = "Change a payment method's nickname, for example after a card is renamed. Entries already on it show the new name; the old name stays in the history.",
        input_schema = schema_for_input::<RenamePaymentMethodInput>().expect("schema")
    )]
    fn rename_payment_method(&self, args: JsonObject) -> CallToolResult {
        respond((|| {
            let i: RenamePaymentMethodInput = parse_args(args)?;
            let key: ledger_core::PaymentMethodKey = i.payment_method.into();
            let p = self.with_ledger(|l| Ok(l.rename_payment_method(&key, &i.nickname)?))?;
            let note = format!(
                "Renamed to {}. Entries already on it now show the new name, and the old name stays in the history.",
                p.nickname
            );
            Ok(with_note(serde_json::to_value(&p).expect("payment method serializes"), note))
        })())
    }

    #[tool(
        description = "Retire a payment method that is no longer used, such as a closed card. It is never deleted: entries already on it keep it, but nothing new can be recorded on or assigned to it. If it was the default, there is no default afterward.",
        input_schema = schema_for_input::<RetirePaymentMethodInput>().expect("schema")
    )]
    fn retire_payment_method(&self, args: JsonObject) -> CallToolResult {
        respond((|| {
            let i: RetirePaymentMethodInput = parse_args(args)?;
            let key: ledger_core::PaymentMethodKey = i.payment_method.into();
            let (was_default, p) = self.with_ledger(|l| {
                let was_default = l.find_payment_method(&key)?.is_default;
                let p = l.retire_payment_method(&key)?;
                Ok((was_default, p))
            })?;
            let mut note = format!(
                "{} is retired, not deleted: its {} entries keep it, and nothing new can go on it.",
                p.nickname, p.transaction_count
            );
            if was_default {
                note.push_str(" It was the default, so there is no default now.");
            }
            Ok(with_note(serde_json::to_value(&p).expect("payment method serializes"), note))
        })())
    }

    #[tool(
        description = "Choose the payment method new expenses use when none is named, or clear the default with clear set to true. There is at most one default, and income never uses it.",
        input_schema = schema_for_input::<SetDefaultPaymentMethodInput>().expect("schema")
    )]
    fn set_default_payment_method(&self, args: JsonObject) -> CallToolResult {
        respond((|| {
            let i: SetDefaultPaymentMethodInput = parse_args(args)?;
            let clear = i.clear.unwrap_or(false);
            match (i.payment_method, clear) {
                (Some(pm), false) => {
                    let key: ledger_core::PaymentMethodKey = pm.into();
                    let p = self
                        .with_ledger(|l| Ok(l.set_default_payment_method(Some(&key))?))?
                        .expect("a chosen default is returned");
                    let note = format!(
                        "{} is now the default: new expenses use it when no payment method is named.",
                        describe_method(&p.nickname, p.kind, p.last4.as_deref())
                    );
                    Ok(with_note(serde_json::to_value(&p).expect("payment method serializes"), note))
                }
                (None, true) => {
                    self.with_ledger(|l| Ok(l.set_default_payment_method(None)?))?;
                    Ok(with_note(
                        json!({ "default": null }),
                        "No default now: new expenses get no payment method unless one is named.",
                    ))
                }
                _ => Err(ToolError::Input(
                    "Give payment_method to choose the default, or clear set to true to have none, not both.".into(),
                )),
            }
        })())
    }
}

#[tool_handler(
    name = "freehold-ledger",
    instructions = "Freehold Ledger is the person's own bookkeeping file for a small business: expenses and income, kept for tax time. Amounts are US dollars (for example \"300\" or \"300.50\"); dates are YYYY-MM-DD and default to today. Nothing is ever deleted: mistakes are voided or corrected, and the history stays. Act on the person's words: record or correct straight away using sensible defaults (today's date, the most likely category) and say what you chose, rather than asking questions first. When the person adds detail later (\"it's a laptop\"), fix the existing entry with correct_transaction instead of adding a new one. Call list_categories when unsure of a category. To import rows from a file or statement, use record_transactions, up to 50 rows per call. Payment methods (cards, bank accounts, cash) are kept by nickname and last 4 digits only: never ask for, repeat, or send a full card or account number, expiry date or security code."
)]
impl ServerHandler for LedgerTools {}
