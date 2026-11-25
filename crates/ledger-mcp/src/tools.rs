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
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct LedgerTools {
    ledger: Arc<Mutex<Books>>,
}

/// What the server holds in place of the ledger.
enum Books {
    Open(Ledger),
    /// The file was opened by a newer Freehold Ledger, so this version cannot open it. The server
    /// keeps serving the admin tools, which work on the file's path, so the way back stays open.
    Newer(NewerFile),
    /// The ledger could not be opened for another reason; carries the `LedgerError` text.
    Unavailable(String),
}

#[derive(Clone)]
struct NewerFile {
    path: PathBuf,
    found: u32,
    supported: u32,
}

impl LedgerTools {
    /// `opened` is the result of opening the ledger at `path`.
    pub fn new(path: PathBuf, opened: Result<Ledger, LedgerError>) -> Self {
        let books = match opened {
            Ok(l) => Books::Open(l),
            Err(LedgerError::NewerSchema { found, supported }) => Books::Newer(NewerFile { path, found, supported }),
            Err(e) => Books::Unavailable(e.to_string()),
        };
        Self { ledger: Arc::new(Mutex::new(books)) }
    }
}

/// Look again at a file that a newer version had opened: if it is now at a format this version can
/// open (it was restored, from here or elsewhere), serve it again without a restart.
fn refresh(books: &mut Books) {
    let Books::Newer(n) = books else { return };
    match Ledger::open(&n.path) {
        Ok(l) => *books = Books::Open(l),
        Err(LedgerError::NewerSchema { found, supported }) => {
            n.found = found;
            n.supported = supported;
        }
        Err(_) => {}
    }
}

/// What to do about books a newer version opened, in the words the agent passes on.
const NEWER_WAY_OUT: &str = "To go back, restore the backup taken before the upgrade: call list_backups, then restore_ledger with that backup's path and a reason. To go forward, reinstall the newer Freehold Ledger.";

/// What can go wrong inside a tool. Every variant becomes an `isError: true` tool result.
enum ToolError {
    Ledger(LedgerError),
    /// The ledger could not be opened at startup; carries the `LedgerError` text.
    Unavailable(String),
    /// The books were opened by a newer Freehold Ledger; only the admin tools can act on them.
    Newer(NewerFile),
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
            ToolError::Newer(n) => format!(
                "These books were opened by a newer Freehold Ledger (file format {}; this version supports up to {}), so this version cannot read them. {NEWER_WAY_OUT} Nothing was changed.",
                n.found, n.supported
            ),
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
    /// Run `f` on the open ledger. When a newer version opened the file, refuse in plain words.
    fn with_ledger<T>(&self, f: impl FnOnce(&mut Ledger) -> Result<T, ToolError>) -> Result<T, ToolError> {
        self.books(f, |n| Err(ToolError::Newer(n.clone())))
    }

    /// Run `open` on the open ledger, or `newer` on the file a newer version opened (the admin
    /// tools, which work from its path). After `newer` the file is looked at again, so a restore
    /// that brought it back to a format this version can open is served on the next call.
    fn books<T>(
        &self,
        open: impl FnOnce(&mut Ledger) -> Result<T, ToolError>,
        newer: impl FnOnce(&NewerFile) -> Result<T, ToolError>,
    ) -> Result<T, ToolError> {
        let mut guard = self.ledger.lock().unwrap_or_else(|p| p.into_inner());
        refresh(&mut guard);
        let out = match &mut *guard {
            Books::Open(l) => return open(l),
            Books::Newer(n) => newer(n),
            Books::Unavailable(msg) => Err(ToolError::Unavailable(msg.clone())),
        };
        refresh(&mut guard);
        out
    }

    /// Refuse when a newer version opened the books, for a tool that does not read the ledger
    /// (its answer may not match what the newer version keeps). Any other state lets it run.
    fn refuse_if_newer(&self) -> Result<(), ToolError> {
        let mut guard = self.ledger.lock().unwrap_or_else(|p| p.into_inner());
        refresh(&mut guard);
        match &*guard {
            Books::Newer(n) => Err(ToolError::Newer(n.clone())),
            _ => Ok(()),
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
    /// The card, account or cash it was paid with: nickname or id. Omit to use the default payment method, if one is set.
    payment_method: Option<PaymentMethodArg>,
    /// The business to use: its name or id. Omit for the current business (see ledger_info).
    business: Option<BusinessArg>,
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
    /// The account or method the money came into: nickname or id. Omit for none; income never uses the default.
    payment_method: Option<PaymentMethodArg>,
    /// The business to use: its name or id. Omit for the current business (see ledger_info).
    business: Option<BusinessArg>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RecordTransactionsInput {
    /// Short label for this batch, for example the file name the rows came from ("bank-export-jan.csv"). Kept so the batch can be found and undone later.
    label: String,
    /// 1 to 50 entries, in file order.
    entries: Vec<BatchEntryInput>,
    /// Payment method for every entry in this batch, for example the card the statement is for. An entry's own payment_method wins. Expenses with neither use the default.
    payment_method: Option<PaymentMethodArg>,
    /// The business to use: its name or id. Omit for the current business (see ledger_info).
    business: Option<BusinessArg>,
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
    /// This entry's payment method, overriding the batch's.
    payment_method: Option<PaymentMethodArg>,
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
    let pm: Option<ledger_core::PaymentMethodKey> = e.payment_method.map(Into::into);
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
                payment_method: pm.clone(),
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
                payment_method: pm.clone(),
            })
        }
    };
    Ok(ledger_core::BatchEntry { entry, allow_duplicate: e.allow_duplicate.unwrap_or(false), payment_method: pm })
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
    /// Only entries on this payment method (nickname or id), or "none" for entries with no payment method.
    payment_method: Option<PaymentMethodArg>,
    /// Also list voided entries, each with its void reason. Defaults to false.
    include_voided: Option<bool>,
    /// Most entries to return, newest first. Defaults to 50; the maximum is 500.
    limit: Option<u32>,
    /// The business to use: its name or id. Omit for the current business (see ledger_info).
    business: Option<BusinessArg>,
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
    /// New payment method: nickname or id. Omit to keep the original's.
    payment_method: Option<PaymentMethodArg>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct AssignPaymentMethodInput {
    /// The payment method these entries were paid with: its nickname or id.
    payment_method: PaymentMethodArg,
    /// Ids of the entries, 1 to 200. list_transactions finds them.
    ids: Vec<i64>,
    /// The business to use: its name or id. Omit for the current business (see ledger_info).
    business: Option<BusinessArg>,
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
    /// Only entries on this payment method (nickname or id), or "none" for entries with no payment method.
    payment_method: Option<PaymentMethodArg>,
    /// The business to use: its name or id. Omit for the current business (see ledger_info).
    business: Option<BusinessArg>,
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
    /// The business to use: its name or id. Omit for the current business (see ledger_info).
    business: Option<BusinessArg>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct BackupInput {
    /// Full path of the folder to put the backup in. Omit for the backups folder beside the ledger.
    folder: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct VerifyInput {
    /// Full path of a backup file to check instead of the open ledger. Omit to check the open ledger.
    path: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ListBackupsInput {
    /// Full path of the folder to look in. Omit for the backups folder beside the ledger.
    folder: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RestoreInput {
    /// Full path of the backup file to go back to (see list_backups).
    path: String,
    /// Short reason for going back, for example "the person asked to go back to yesterday's backup".
    reason: String,
}

/// A path the agent passes must be a full path: a relative one would resolve against wherever the
/// server was started, which can be a project folder that is later shared or published.
fn full_path<'a>(field: &str, p: &'a str) -> Result<&'a std::path::Path, ToolError> {
    let path = std::path::Path::new(p);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(ToolError::Input(format!(
            "{field} must be a full path, such as /Users/you/Documents/Ledger backups. Nothing was changed."
        )))
    }
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

fn method_of(t: &ledger_core::Transaction) -> Option<String> {
    t.payment_method.as_ref().map(|p| describe_method(&p.nickname, p.kind, p.last4.as_deref()))
}

/// The sentence every record and correct note ends with: which payment method was stored, or that none was.
fn method_sentence(t: &ledger_core::Transaction, used_default: bool, kept: bool) -> String {
    match method_of(t) {
        Some(d) => {
            let suffix = if used_default {
                " (your default)"
            } else if kept {
                " (kept from the original)"
            } else {
                ""
            };
            format!(" Payment method: {d}{suffix}.")
        }
        None => " No payment method is set; say what it was paid with and I will add it.".to_string(),
    }
}

/// A filter argument: "none" (any case) means entries without a payment method.
fn filter_arg(l: &Ledger, biz: &ledger_core::Business, a: PaymentMethodArg) -> Result<ledger_core::PaymentMethodFilter, ToolError> {
    if let PaymentMethodArg::Nickname(s) = &a {
        if s.trim().eq_ignore_ascii_case("none") {
            return Ok(ledger_core::PaymentMethodFilter::Unassigned);
        }
    }
    let key: ledger_core::PaymentMethodKey = a.into();
    Ok(ledger_core::PaymentMethodFilter::Id(l.find_payment_method_in(Some(&bkey(biz)), &key)?.id))
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
    /// The business to use: its name or id. Omit for the current business (see ledger_info).
    business: Option<BusinessArg>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ListPaymentMethodsInput {
    /// Also list retired payment methods. Defaults to false.
    include_retired: Option<bool>,
    /// The business to use: its name or id. Omit for the current business (see ledger_info).
    business: Option<BusinessArg>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RenamePaymentMethodInput {
    /// The payment method to rename: its id or its nickname.
    payment_method: PaymentMethodArg,
    /// The new nickname.
    nickname: String,
    /// The business to use: its name or id. Omit for the current business (see ledger_info).
    business: Option<BusinessArg>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RetirePaymentMethodInput {
    /// The payment method to retire: its id or its nickname.
    payment_method: PaymentMethodArg,
    /// The business to use: its name or id. Omit for the current business (see ledger_info).
    business: Option<BusinessArg>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SetDefaultPaymentMethodInput {
    /// The payment method new expenses should use when none is named.
    payment_method: Option<PaymentMethodArg>,
    /// Set true to have no default.
    clear: Option<bool>,
    /// The business to use: its name or id. Omit for the current business (see ledger_info).
    business: Option<BusinessArg>,
}

/// A business as the wire carries it: its id (a number) or its name (text, any letter case).
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(untagged)]
enum BusinessArg {
    Id(i64),
    Name(String),
}

impl From<BusinessArg> for ledger_core::BusinessKey {
    fn from(a: BusinessArg) -> Self {
        match a {
            BusinessArg::Id(n) => ledger_core::BusinessKey::Id(n),
            BusinessArg::Name(s) => ledger_core::BusinessKey::Name(s),
        }
    }
}

fn business_json(b: &ledger_core::Business) -> Value {
    serde_json::to_value(b).expect("business serializes")
}

/// The reply object of a single-business tool: the business plus its stored note as `business_note`,
/// because the top-level `note` is the reply sentence and would hide the stored one.
fn business_reply(b: &ledger_core::Business, sentence: impl Into<String>) -> Value {
    let mut v = business_json(b);
    v["business_note"] = match &b.note {
        Some(n) => Value::String(n.clone()),
        None => Value::Null,
    };
    with_note(v, sentence)
}

/// Which business a call acts in: the one named, else the current one. A write refuses an archived business.
fn scope(l: &Ledger, b: Option<BusinessArg>, write: bool) -> Result<ledger_core::Business, ToolError> {
    match b {
        Some(a) => {
            let biz = l.find_business(&a.into())?;
            if write && biz.archived {
                return Err(LedgerError::ArchivedBusiness(biz.name).into());
            }
            Ok(biz)
        }
        None => Ok(l.current_business()?.ok_or(LedgerError::NoCurrentBusiness)?),
    }
}

fn bkey(b: &ledger_core::Business) -> ledger_core::BusinessKey {
    ledger_core::BusinessKey::Id(b.id)
}

/// "In Pat's Prints: ..." or "In Test shop (test business): ...".
fn in_business(b: &ledger_core::Business, note: String) -> String {
    format!("In {}: {note}", biz_label(&b.name, b.is_test))
}

/// A success reply for a business-scoped tool: the business it used and the prefixed note.
fn stamped(mut v: Value, b: &ledger_core::Business, note: impl Into<String>) -> Value {
    v["business"] = json!({ "id": b.id, "name": b.name, "is_test": b.is_test });
    with_note(v, in_business(b, note.into()))
}

/// The business an existing entry lives in.
fn business_of(l: &Ledger, business_id: i64) -> Result<ledger_core::Business, ToolError> {
    Ok(l.find_business(&ledger_core::BusinessKey::Id(business_id))?)
}

fn biz_label(name: &str, is_test: bool) -> String {
    if is_test {
        format!("{name} (test business)")
    } else {
        name.to_string()
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CreateBusinessInput {
    /// The business name, 1 to 60 characters, for example "Pat's Prints".
    name: String,
    /// An optional short note about the business, up to 200 characters.
    note: Option<String>,
    /// True for a business used only for trying things out; the app marks it Test.
    is_test: Option<bool>,
    /// True to make the new business the one new entries go to.
    switch_to: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ListBusinessesInput {
    /// True to include archived businesses.
    include_archived: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct BusinessOnlyInput {
    /// The business: its name or id.
    business: BusinessArg,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RenameBusinessInput {
    /// The business: its name or id.
    business: BusinessArg,
    /// The new name, 1 to 60 characters.
    name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct UpdateBusinessInput {
    /// The business: its name or id.
    business: BusinessArg,
    /// A new note, up to 200 characters.
    note: Option<String>,
    /// True to remove the note.
    clear_note: Option<bool>,
    /// True for a business used only for trying things out; the app marks it Test.
    is_test: Option<bool>,
}

#[tool_router]
impl LedgerTools {
    #[tool(
        description = "Show which ledger file is open, its schema version and how many transactions it holds. Call this first to confirm the books are reachable.",
        annotations(read_only_hint = true)
    )]
    fn ledger_info(&self) -> CallToolResult {
        respond(self.books(|l| {
            let info = l.info()?;
            let active = l.businesses(false)?;
            let current = l.current_business()?;
            let list: Vec<Value> = active
                .iter()
                .map(|b| {
                    json!({
                        "id": b.id,
                        "name": b.name,
                        "is_test": b.is_test,
                        "is_current": b.is_current,
                        "transaction_count": b.transaction_count,
                    })
                })
                .collect();
            let n = active.len();
            let tail = "Amounts are US dollars and nothing is ever deleted, only voided or corrected.";
            let note = match &current {
                Some(c) => format!(
                    "This one file holds all the books, in {n} businesses; new entries go to {}. {tail}",
                    c.name
                ),
                None => format!(
                    "This one file holds all the books, in {n} businesses; no business is current, so call switch_business before recording. {tail}"
                ),
            };
            let mut v = serde_json::to_value(info).expect("info serializes");
            v["businesses"] = Value::Array(list);
            v["app_version"] = Value::String(ledger_core::app_version().to_string());
            v["supported_schema"] = json!(ledger_core::SUPPORTED_SCHEMA);
            v["current_business"] = match &current {
                Some(c) => json!({ "id": c.id, "name": c.name, "is_test": c.is_test }),
                None => Value::Null,
            };
            Ok(with_note(v, note))
        }, |n| {
            let v = json!({
                "path": n.path.display().to_string(),
                "schema_version": n.found,
                "supported_schema": n.supported,
                "app_version": ledger_core::app_version(),
                "newer_version": true,
            });
            Ok(with_note(
                v,
                format!(
                    "This file is from a newer Freehold Ledger (file format {}; this version supports up to {}), so only list_backups, verify_ledger (with a path), restore_ledger and this call work. {NEWER_WAY_OUT}",
                    n.found, n.supported
                ),
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
            self.refuse_if_newer()?;
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
                payment_method: i.payment_method.map(Into::into),
            };
            let named = new.payment_method.is_some();
            let (biz, t) = self.with_ledger(|l| {
                let biz = scope(l, i.business, true)?;
                let t = l.record_expense_in(Some(&bkey(&biz)), new)?;
                Ok((biz, t))
            })?;
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
            let note = format!("{note}{}", method_sentence(&t, !named && t.payment_method.is_some(), false));
            Ok(stamped(txn_json(&t), &biz, note))
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
                payment_method: i.payment_method.map(Into::into),
            };
            let (biz, t) = self.with_ledger(|l| {
                let biz = scope(l, i.business, true)?;
                let t = l.record_income_in(Some(&bkey(&biz)), new)?;
                Ok((biz, t))
            })?;
            let note = format!(
                "Recorded as {}{}: income is reported for the year it was received, and the same source name is reused each time.{}",
                t.category_label,
                t.schedule_c_line.as_deref().map(|l| format!(" (Schedule C line {l})")).unwrap_or_default(),
                method_sentence(&t, false, false)
            );
            Ok(stamped(txn_json(&t), &biz, note))
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
            let (biz, r) = self.with_ledger(|l| {
                let biz = scope(l, i.business, true)?;
                let r = l.record_batch_in(
                    Some(&bkey(&biz)),
                    ledger_core::NewBatch { label: i.label, entries, payment_method: i.payment_method.map(Into::into) },
                )?;
                Ok((biz, r))
            })?;
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
            let mut groups: Vec<(String, usize)> = Vec::new();
            let mut none_count = 0;
            for x in &r.recorded {
                match &x.transaction.payment_method {
                    Some(p) => match groups.iter_mut().find(|g| g.0 == p.nickname) {
                        Some(g) => g.1 += 1,
                        None => groups.push((p.nickname.clone(), 1)),
                    },
                    None => none_count += 1,
                }
            }
            let mut parts: Vec<String> = groups.iter().map(|(n, c)| format!("{n} on {c}")).collect();
            if none_count > 0 {
                parts.push(format!("none on {none_count}"));
            }
            let note = match r.batch_id {
                Some(id) => {
                    let base = format!(
                        "Recorded {rc} entries as batch {id} (\"{}\"). Each one shows the date and category it was filed under; reclassify or correct any you disagree with.",
                        r.label
                    );
                    let base = format!("{base} To undo this whole batch, call void_batch with batch_id {id}.");
                    let base = format!("{base} Payment methods: {}.", parts.join(", "));
                    if sc == 0 {
                        base
                    } else {
                        format!("{base} {sc} looked like entries already in the books (same date, kind, amount and description) and were not recorded; if one really is a second charge, send it again with allow_duplicate set to true.")
                    }
                }
                None => format!("Nothing new was recorded: all {sc} entries match entries already in the books (same date, kind, amount and description). If one really is a second charge, send it again with allow_duplicate set to true."),
            };
            Ok(stamped(
                json!({
                    "batch_id": r.batch_id,
                    "label": r.label,
                    "recorded_count": rc,
                    "skipped_count": sc,
                    "recorded": recorded,
                    "skipped_duplicates": skipped,
                }),
                &biz,
                note,
            ))
        })())
    }

    #[tool(
        description = "List ledger entries, newest first, optionally filtered by date range, kind, category, income source, or asset flag. Voided entries are hidden unless include_voided is true. Use to answer 'what did I spend on...', to find an id before correcting something, or to review before tax time. Filter by batch_id to see one import. Filter by payment_method to see one card or account, or \"none\" for entries without one.",
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
            let biz = self.with_ledger(|l| scope(l, i.business, false))?;
            let pm_filter = i
                .payment_method
                .map(|a| self.with_ledger(|l| filter_arg(l, &biz, a)))
                .transpose()?;
            let filter = ledger_core::Filter {
                from: parse_date_arg(&i.from)?,
                to: parse_date_arg(&i.to)?,
                kind: i.kind.map(Into::into),
                category: i.category,
                source: i.source,
                is_asset: i.is_asset,
                batch_id: i.batch_id,
                payment_method: pm_filter,
                business_id: Some(biz.id),
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
            Ok(stamped(json!({ "transactions": transactions, "count": count }), &biz, note))
        })())
    }

    #[tool(
        description = "Change the category of an existing entry, for example from uncategorized to office_expense. The old category stays in the history. Use when the person says 'that should be filed under...'. For a wrong amount, date, or description use correct_transaction.",
        input_schema = schema_for_input::<ReclassifyInput>().expect("schema")
    )]
    fn reclassify_transaction(&self, args: JsonObject) -> CallToolResult {
        respond((|| {
            let i: ReclassifyInput = parse_args(args)?;
            let (biz, t) = self.with_ledger(|l| {
                let t = l.reclassify(i.id, &i.category, &i.reason)?;
                Ok((business_of(l, t.business_id)?, t))
            })?;
            Ok(stamped(
                txn_json(&t),
                &biz,
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
            let (biz, t) = self.with_ledger(|l| {
                let t = l.void(i.id, &i.reason)?;
                Ok((business_of(l, t.business_id)?, t))
            })?;
            Ok(stamped(
                txn_json(&t),
                &biz,
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
            let (biz, r) = self.with_ledger(|l| {
                let r = l.void_batch(i.batch_id, &i.reason)?;
                let biz = match (r.voided.first(), r.already_voided_ids.first()) {
                    (Some(t), _) => business_of(l, t.business_id)?,
                    (None, Some(id)) => business_of(l, l.get(*id)?.business_id)?,
                    (None, None) => scope(l, None, false)?,
                };
                Ok((biz, r))
            })?;
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
            Ok(stamped(
                json!({
                    "batch_id": r.batch_id,
                    "label": r.label,
                    "voided_count": n,
                    "already_voided_count": m,
                    "voided": r.voided.iter().map(txn_json).collect::<Vec<Value>>(),
                    "already_voided_ids": r.already_voided_ids,
                }),
                &biz,
                note,
            ))
        })())
    }

    #[tool(
        description = "Fix a wrong amount, date, description, category, asset flag, income source, or payment method on an existing entry. The original is voided with your reason and a linked replacement is recorded in one step, so the history shows what changed. Give at least one field to change.",
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
                && i.payment_method.is_none()
            {
                return Err(ToolError::Input(
                    "Say what to change: give at least one of amount, date, description, category, is_asset, source or payment_method.".into(),
                ));
            }
            let r = ledger_core::Replacement {
                occurred_on: parse_date_arg(&i.date)?,
                amount_cents: i.amount.as_ref().map(dollars_to_cents).transpose()?,
                description: i.description,
                category: i.category,
                is_asset: i.is_asset,
                source: i.source,
                payment_method: i.payment_method.map(Into::into),
            };
            let named = r.payment_method.is_some();
            let (biz, c) = self.with_ledger(|l| {
                let c = l.correct(i.id, &i.reason, r)?;
                Ok((business_of(l, c.replacement.business_id)?, c))
            })?;
            Ok(stamped(
                json!({ "voided": txn_json(&c.voided), "replacement": txn_json(&c.replacement) }),
                &biz,
                format!(
                    "Corrected by voiding the original and recording a linked replacement, so the history shows exactly what changed and why.{}",
                    method_sentence(&c.replacement, false, !named && c.replacement.payment_method.is_some())
                ),
            ))
        })())
    }

    #[tool(
        description = "Totals for a period: spending by tax category, income by source, profit. Use for 'what did I spend...', 'how much came in from...', 'how am I doing this quarter'. For equipment purchases use is_asset=true. Results include totals by payment method; filter by payment_method for one card.",
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
            let biz = self.with_ledger(|l| scope(l, i.business, false))?;
            let pm_filter = i
                .payment_method
                .map(|a| self.with_ledger(|l| filter_arg(l, &biz, a)))
                .transpose()?;
            let req = ledger_core::SummaryRequest {
                from,
                to,
                category: i.category,
                source: i.source,
                is_asset: i.is_asset,
                payment_method: pm_filter,
                business_id: Some(biz.id),
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
            Ok(stamped(v, &biz, note))
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
            let (biz, r) = self.with_ledger(|l| {
                let biz = scope(l, i.business, false)?;
                let r = l.export_tax_year_in(i.year, Some(&bkey(&biz)), dest.as_deref())?;
                Ok((biz, r))
            })?;
            let folder = r.folder.display().to_string();
            let note = format!(
                "Your {} books are in {folder}. Hand the whole folder to your preparer; README.txt explains every file.",
                i.year
            );
            Ok(stamped(
                json!({
                    "folder": folder,
                    "files": r.files,
                    "totals": serde_json::to_value(&r.summary).expect("summary serializes"),
                }),
                &biz,
                note,
            ))
        })())
    }

    #[tool(
        description = "Say which payment method one or more existing entries were paid with: give the payment method and the entry ids (1 to 200). Use for 'that was on my Visa' or 'everything in March was on the Amex': list_transactions finds the ids, for example by date range, or with payment_method \"none\" for entries that have no payment method yet. Each change is kept in the history. Voided entries cannot be changed.",
        input_schema = schema_for_input::<AssignPaymentMethodInput>().expect("schema")
    )]
    fn assign_payment_method(&self, args: JsonObject) -> CallToolResult {
        respond((|| {
            let i: AssignPaymentMethodInput = parse_args(args)?;
            let key: ledger_core::PaymentMethodKey = i.payment_method.into();
            let (biz, r) = self.with_ledger(|l| {
                let biz = scope(l, i.business, true)?;
                let r = l.assign_payment_method_in(Some(&bkey(&biz)), &key, &i.ids)?;
                Ok((biz, r))
            })?;
            let p = &r.payment_method;
            let d = describe_method(&p.nickname, p.kind, p.last4.as_deref());
            let (a, u) = (r.assigned.len(), r.unchanged_ids.len());
            let note = if a == 0 {
                format!("Nothing changed: all {u} entries were already on {d}.")
            } else {
                let mut n = format!("Put {a} entries on {d}.");
                if u > 0 {
                    n.push_str(&format!(" {u} were already on it and were left as they were."));
                }
                n.push_str(" Any earlier payment method stays in the history.");
                n
            };
            Ok(stamped(
                json!({
                    "payment_method": serde_json::to_value(p).expect("payment method serializes"),
                    "assigned_count": a,
                    "unchanged_count": u,
                    "assigned": r.assigned.iter().map(txn_json).collect::<Vec<Value>>(),
                    "unchanged_ids": r.unchanged_ids,
                }),
                &biz,
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
            let (biz, p, default) = self.with_ledger(|l| {
                let biz = scope(l, i.business, true)?;
                let p = l.create_payment_method_in(Some(&bkey(&biz)), new)?;
                let d = l.default_payment_method_in(Some(&bkey(&biz)))?;
                Ok((biz, p, d))
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
            Ok(stamped(serde_json::to_value(&p).expect("payment method serializes"), &biz, note))
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
            let (biz, list) = self.with_ledger(|l| {
                let biz = scope(l, i.business, false)?;
                let list = l.payment_methods_in(Some(&bkey(&biz)), i.include_retired.unwrap_or(false))?;
                Ok((biz, list))
            })?;
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
            Ok(stamped(json!({ "payment_methods": list, "count": n }), &biz, note))
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
            let (biz, before, p) = self.with_ledger(|l| {
                let biz = scope(l, i.business, true)?;
                let before = l.find_payment_method_in(Some(&bkey(&biz)), &key)?.nickname;
                let p = l.rename_payment_method_in(Some(&bkey(&biz)), &key, &i.nickname)?;
                Ok((biz, before, p))
            })?;
            let note = if p.nickname == before {
                format!("{} already has that name, so nothing changed.", p.nickname)
            } else {
                format!(
                    "Renamed to {}. Entries already on it now show the new name, and the old name stays in the history.",
                    p.nickname
                )
            };
            Ok(stamped(serde_json::to_value(&p).expect("payment method serializes"), &biz, note))
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
            let (biz, was_default, p) = self.with_ledger(|l| {
                let biz = scope(l, i.business, true)?;
                let was_default = l.find_payment_method_in(Some(&bkey(&biz)), &key)?.is_default;
                let p = l.retire_payment_method_in(Some(&bkey(&biz)), &key)?;
                Ok((biz, was_default, p))
            })?;
            let mut note = format!(
                "{} is retired, not deleted: its {} entries keep it, and nothing new can go on it.",
                p.nickname, p.transaction_count
            );
            if was_default {
                note.push_str(" It was the default, so there is no default now.");
            }
            Ok(stamped(serde_json::to_value(&p).expect("payment method serializes"), &biz, note))
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
                    let (biz, p) = self.with_ledger(|l| {
                        let biz = scope(l, i.business, true)?;
                        let p = l.set_default_payment_method_in(Some(&bkey(&biz)), Some(&key))?;
                        Ok((biz, p.expect("a chosen default is returned")))
                    })?;
                    let note = format!(
                        "{} is now the default: new expenses use it when no payment method is named.",
                        describe_method(&p.nickname, p.kind, p.last4.as_deref())
                    );
                    Ok(stamped(serde_json::to_value(&p).expect("payment method serializes"), &biz, note))
                }
                (None, true) => {
                    let biz = self.with_ledger(|l| {
                        let biz = scope(l, i.business, true)?;
                        l.set_default_payment_method_in(Some(&bkey(&biz)), None)?;
                        Ok(biz)
                    })?;
                    Ok(stamped(
                        json!({ "default": null }),
                        &biz,
                        "No default now: new expenses get no payment method unless one is named.",
                    ))
                }
                _ => Err(ToolError::Input(
                    "Give payment_method to choose the default, or clear set to true to have none, not both.".into(),
                )),
            }
        })())
    }

    #[tool(
        description = "Add a business to this ledger, for example a second shop, or a test business for trying things out so it never mixes with the real books. Each business keeps its own entries, payment methods and tax export; categories are shared. Set is_test=true for a test business: the app marks it Test. New entries keep going to the current business unless switch_to is true (a new business becomes current by itself when none is). Names are unique among businesses in use, in any letter case.",
        input_schema = schema_for_input::<CreateBusinessInput>().expect("schema")
    )]
    fn create_business(&self, args: JsonObject) -> CallToolResult {
        respond((|| {
            let i: CreateBusinessInput = parse_args(args)?;
            let (b, current) = self.with_ledger(|l| {
                let b = l.create_business(ledger_core::NewBusiness {
                    name: i.name,
                    note: i.note,
                    is_test: i.is_test.unwrap_or(false),
                    make_current: i.switch_to.unwrap_or(false),
                })?;
                let current = l.current_business()?;
                Ok((b, current))
            })?;
            let t = if b.is_test { " (a test business)" } else { "" };
            let note = if b.is_current {
                format!(
                    "Created {}{t} and switched to it: new entries go there until you switch again.",
                    b.name
                )
            } else {
                let cur = current.map(|c| c.name).unwrap_or_default();
                format!(
                    "Created {}{t}. New entries still go to {cur}; say \"switch to {}\" to record there.",
                    b.name, b.name
                )
            };
            Ok(business_reply(&b, note))
        })())
    }

    #[tool(
        description = "List the businesses in this ledger with their ids, which one is current (where new entries go), which are test businesses, and how many entries each holds. Use it for \"which businesses do I have?\" or before switch_business.",
        input_schema = schema_for_input::<ListBusinessesInput>().expect("schema"),
        annotations(read_only_hint = true)
    )]
    fn list_businesses(&self, args: JsonObject) -> CallToolResult {
        respond((|| {
            let i: ListBusinessesInput = parse_args(args)?;
            let list = self.with_ledger(|l| Ok(l.businesses(i.include_archived.unwrap_or(false))?))?;
            let n = list.len();
            let note = match list.iter().find(|b| b.is_current) {
                Some(c) => format!("{n} businesses. New entries go to {}.", c.name),
                None => format!("{n} businesses. None is current: call switch_business before recording."),
            };
            let items: Vec<Value> = list.iter().map(business_json).collect();
            Ok(with_note(json!({ "businesses": items, "count": n }), note))
        })())
    }

    #[tool(
        description = "Choose which business new entries are recorded in, for example when the person says \"switch to my test business\" or \"back to the real books\". This only changes where you record; the person's app window keeps showing whichever business they picked there. Tell the person which business you are now recording in.",
        input_schema = schema_for_input::<BusinessOnlyInput>().expect("schema")
    )]
    fn switch_business(&self, args: JsonObject) -> CallToolResult {
        respond((|| {
            let i: BusinessOnlyInput = parse_args(args)?;
            let key: ledger_core::BusinessKey = i.business.into();
            let b = self.with_ledger(|l| Ok(l.set_current_business(&key)?))?;
            let note = format!("New entries now go to {}.", biz_label(&b.name, b.is_test));
            Ok(business_reply(&b, note))
        })())
    }

    #[tool(
        description = "Give a business a new name. Its entries, payment methods and exports follow the new name. A change of letter case alone is a rename too.",
        input_schema = schema_for_input::<RenameBusinessInput>().expect("schema")
    )]
    fn rename_business(&self, args: JsonObject) -> CallToolResult {
        respond((|| {
            let i: RenameBusinessInput = parse_args(args)?;
            let key: ledger_core::BusinessKey = i.business.into();
            let (old, b) = self.with_ledger(|l| {
                let old = l.find_business(&key)?.name;
                let b = l.rename_business(&key, &i.name)?;
                Ok((old, b))
            })?;
            let note = if old == b.name {
                format!("{} already has that name.", b.name)
            } else {
                format!("Renamed {old} to {}.", b.name)
            };
            Ok(business_reply(&b, note))
        })())
    }

    #[tool(
        description = "Change a business's note, or mark it as a test business or a real one. Use rename_business to change its name.",
        input_schema = schema_for_input::<UpdateBusinessInput>().expect("schema")
    )]
    fn update_business(&self, args: JsonObject) -> CallToolResult {
        respond((|| {
            let i: UpdateBusinessInput = parse_args(args)?;
            let clear = i.clear_note.unwrap_or(false);
            if i.note.is_none() && !clear && i.is_test.is_none() {
                return Err(ToolError::Input("Give a note, clear_note or is_test to change.".into()));
            }
            if i.note.is_some() && clear {
                return Err(ToolError::Input("Give note to set one, or clear_note to remove it, not both.".into()));
            }
            let key: ledger_core::BusinessKey = i.business.into();
            let mut changed: Vec<&str> = Vec::new();
            let b = self.with_ledger(|l| {
                let mut b = l.find_business(&key)?;
                if let Some(n) = i.note.as_deref() {
                    let was = b.note.clone();
                    b = l.set_business_note(&key, Some(n))?;
                    if b.note != was {
                        changed.push(if b.note.is_some() { "note set" } else { "note cleared" });
                    }
                } else if clear {
                    let was = b.note.clone();
                    b = l.set_business_note(&key, None)?;
                    if b.note != was {
                        changed.push("note cleared");
                    }
                }
                if let Some(t) = i.is_test {
                    let was = b.is_test;
                    b = l.set_business_test(&key, t)?;
                    if b.is_test != was {
                        changed.push(if t { "marked as a test business" } else { "marked as a real business" });
                    }
                }
                Ok(b)
            })?;
            let note = if changed.is_empty() {
                format!("{} already looked like that, so nothing changed.", b.name)
            } else {
                format!("Updated {}: {}.", b.name, changed.join(", "))
            };
            Ok(business_reply(&b, note))
        })())
    }

    #[tool(
        description = "Archive a business that is no longer used. Nothing is deleted: its entries, payment methods and exports stay, the app can still show it, and restore_business brings it back. Nothing new can be recorded in it. If it was the current business, none is current until you switch.",
        input_schema = schema_for_input::<BusinessOnlyInput>().expect("schema")
    )]
    fn archive_business(&self, args: JsonObject) -> CallToolResult {
        respond((|| {
            let i: BusinessOnlyInput = parse_args(args)?;
            let key: ledger_core::BusinessKey = i.business.into();
            let (was_current, b) = self.with_ledger(|l| {
                let was_current = l.find_business(&key)?.is_current;
                let b = l.archive_business(&key)?;
                Ok((was_current, b))
            })?;
            let mut note = format!("Archived {}; nothing was deleted.", b.name);
            if was_current {
                note.push_str(" No business is current now: say which one to record in.");
            }
            Ok(business_reply(&b, note))
        })())
    }

    #[tool(
        description = "Bring back an archived business so entries can be recorded in it again. It does not become current; use switch_business for that.",
        input_schema = schema_for_input::<BusinessOnlyInput>().expect("schema")
    )]
    fn restore_business(&self, args: JsonObject) -> CallToolResult {
        respond((|| {
            let i: BusinessOnlyInput = parse_args(args)?;
            let key: ledger_core::BusinessKey = i.business.into();
            let b = self.with_ledger(|l| Ok(l.restore_business(&key)?))?;
            Ok(business_reply(&b, format!("Restored {}.", b.name)))
        })())
    }
    #[tool(
        description = "Copy the whole books (every business, entry and the history) to a backup file. Use it when the person asks to back up, and before an upgrade or a large import. Goes to the backups folder beside the ledger unless folder is given. It acts on the whole ledger file, not one business.",
        input_schema = schema_for_input::<BackupInput>().expect("schema")
    )]
    fn backup_ledger(&self, args: JsonObject) -> CallToolResult {
        respond((|| {
            let i: BackupInput = parse_args(args)?;
            let folder = i.folder.as_deref().map(|f| full_path("folder", f)).transpose()?;
            let b = self.with_ledger(|l| Ok(l.backup_to(folder)?))?;
            let note = format!(
                "Your books were copied to {}. This copy holds everything: all businesses, entries and history. Keep a copy somewhere other than this computer too, such as a USB drive.",
                b.path.display()
            );
            Ok(with_note(serde_json::to_value(&b).expect("backup serializes"), note))
        })())
    }

    #[tool(
        description = "Check that the books are intact: the file's own integrity check, its file format and how many entries and businesses it holds. Use it when the person asks if the books are okay, and before relying on a backup. Give path to check a backup file instead of the open ledger. Changes nothing.",
        input_schema = schema_for_input::<VerifyInput>().expect("schema"),
        annotations(read_only_hint = true)
    )]
    fn verify_ledger(&self, args: JsonObject) -> CallToolResult {
        respond((|| {
            let i: VerifyInput = parse_args(args)?;
            let r = match i.path.as_deref() {
                Some(p) => ledger_core::Ledger::verify_file(full_path("path", p)?)?,
                None => self.with_ledger(|l| Ok(l.verify()?))?,
            };
            let note = if r.ok {
                format!(
                    "The books are intact: {} entries in {} businesses, file format {}.",
                    r.transaction_count, r.business_count, r.schema_version
                )
            } else {
                let what = if r.problems.is_empty() {
                    "the file has no recorded file format".to_string()
                } else {
                    r.problems.join("; ")
                };
                format!(
                    "The books have problems: {what}. Do not record anything new. Restore the most recent good backup with restore_ledger."
                )
            };
            Ok(with_note(serde_json::to_value(&r).expect("report serializes"), note))
        })())
    }

    #[tool(
        description = "List the backups that exist, newest first, with when each was made and how many entries it holds. Use it before restore_ledger, and to see whether a recent backup exists (suggest backup_ledger if the newest is more than 30 days old). Looks in the backups folder beside the ledger unless folder is given.",
        input_schema = schema_for_input::<ListBackupsInput>().expect("schema"),
        annotations(read_only_hint = true)
    )]
    fn list_backups(&self, args: JsonObject) -> CallToolResult {
        respond((|| {
            let i: ListBackupsInput = parse_args(args)?;
            let folder = i.folder.as_deref().map(|f| full_path("folder", f)).transpose()?;
            let shown_for = |ledger: &Path| match folder {
                Some(f) => f.to_path_buf(),
                None => Ledger::default_backup_folder(ledger),
            };
            let listed = |shown: PathBuf| Ok((Ledger::list_backups_in(&shown)?, shown));
            let (backups, shown) = self.books(|l| listed(shown_for(l.path())), |n| listed(shown_for(&n.path)))?;
            let n = backups.len();
            let note = if n == 0 {
                format!("There are no backups in {} yet; backup_ledger makes one.", shown.display())
            } else {
                format!("{n} backup{} in {}, newest first.", if n == 1 { "" } else { "s" }, shown.display())
            };
            Ok(with_note(json!({ "backups": backups, "folder": shown }), note))
        })())
    }

    #[tool(
        description = "Put the books back to a backup file. Use it only when the person asks to go back to a backup. It replaces all businesses' books with the backup's contents, after saving the current state first to a safety backup, so it can be undone by restoring that file. Needs a reason. It also works when a newer Freehold Ledger has opened the books and every other tool refuses: that is how to go back after an upgrade.",
        input_schema = schema_for_input::<RestoreInput>().expect("schema")
    )]
    fn restore_ledger(&self, args: JsonObject) -> CallToolResult {
        const SUFFIX: &str = "Nothing was changed.";
        let plain = |m: String| ToolError::Input(if m.ends_with(SUFFIX) { m } else { format!("{m} {SUFFIX}") });
        respond((|| {
            let i: RestoreInput = parse_args(args).map_err(|e| match e {
                ToolError::Input(m) => plain(m),
                other => other,
            })?;
            if i.reason.trim().is_empty() {
                return Err(plain("A reason is needed to restore a backup.".into()));
            }
            let path = full_path("path", &i.path)?;
            let r = self.books(
                |l| l.restore_from(path).map_err(|e| plain(e.to_string())),
                |n| Ledger::restore_file(&n.path, path).map_err(|e| plain(e.to_string())),
            )?;
            let mut note = if r.safety_backup_problems.is_empty() {
                format!(
                    "Your books are now as they were in {}. What was there a moment ago is saved at {}, so this can be undone by restoring that file.",
                    r.restored_from.display(),
                    r.safety_backup.path.display()
                )
            } else {
                format!(
                    "Your books are now as they were in {}. What was there a moment ago is saved at {}. It had problems ({}), so it is kept for safekeeping but cannot be restored.",
                    r.restored_from.display(),
                    r.safety_backup.path.display(),
                    r.safety_backup_problems.join("; ")
                )
            };
            if r.safety_backup.schema_version > ledger_core::SUPPORTED_SCHEMA {
                note.push_str(&format!(
                    " That saved copy is from a newer Freehold Ledger (file format {}), so this version cannot restore it. Reinstall the newer version to go forward again.",
                    r.safety_backup.schema_version
                ));
            }
            Ok(with_note(serde_json::to_value(&r).expect("restore serializes"), note))
        })())
    }
}

#[tool_handler(
    name = "freehold-ledger",
    instructions = "Freehold Ledger is the person's own bookkeeping file for a small business: expenses and income, kept for tax time. Amounts are US dollars (for example \"300\" or \"300.50\"); dates are YYYY-MM-DD and default to today. Nothing is ever deleted: mistakes are voided or corrected, and the history stays. Act on the person's words: record or correct straight away using sensible defaults (today's date, the most likely category) and say what you chose, rather than asking questions first. When the person adds detail later (\"it's a laptop\"), fix the existing entry with correct_transaction instead of adding a new one. Call list_categories when unsure of a category. To import rows from a file or statement, use record_transactions, up to 50 rows per call. Payment methods (cards, bank accounts, cash) are kept by nickname and last 4 digits only: never ask for, repeat, or send a full card or account number, expiry date or security code. One ledger file can hold several businesses, for example the real books and a test business. Every recording, list, summary, payment method and export works in one business: the current one, unless you pass business. Switch with switch_business only when the person asks (\"switch to my test business\"), and always tell the person which business you recorded into. Create, rename, mark as test, archive and restore businesses by conversation; nothing is ever deleted. Before an upgrade or a large import, back up the books with backup_ledger; check them with verify_ledger; restore with restore_ledger only when the person asks. If a tool says the books were opened by a newer Freehold Ledger, only list_backups, verify_ledger (with a path), restore_ledger and ledger_info work; follow the way back the message gives."
)]
impl ServerHandler for LedgerTools {}
