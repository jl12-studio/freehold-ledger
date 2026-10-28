use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Expense,
    Income,
}

impl Kind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Kind::Expense => "expense",
            Kind::Income => "income",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewExpense {
    pub occurred_on: Option<NaiveDate>,
    pub amount_cents: i64,
    pub description: String,
    pub category: String,
    pub is_asset: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewIncome {
    pub occurred_on: Option<NaiveDate>,
    pub amount_cents: i64,
    pub description: String,
    pub category: Option<String>,
    pub source: String,
}

/// Result of voiding a whole batch.
#[derive(Debug, Clone, Serialize)]
pub struct BatchVoid {
    pub batch_id: i64,
    pub label: String,
    pub voided: Vec<Transaction>,
    pub already_voided_ids: Vec<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Replacement {
    pub occurred_on: Option<NaiveDate>,
    pub amount_cents: Option<i64>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub is_asset: Option<bool>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VoidInfo {
    pub reason: String,
    pub recorded_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClassificationChange {
    pub category: String,
    pub reason: String,
    pub recorded_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Transaction {
    pub id: i64,
    pub kind: Kind,
    pub occurred_on: NaiveDate,
    pub amount_cents: i64,
    pub description: String,
    pub original_category: String,
    /// Effective category: the latest classification change, else the original.
    pub category: String,
    pub category_label: String,
    pub schedule_c_line: Option<String>,
    pub is_asset: bool,
    pub income_source: Option<String>,
    pub replaces_id: Option<i64>,
    pub replaced_by_id: Option<i64>,
    /// The import batch this entry was recorded in, if any.
    pub batch_id: Option<i64>,
    pub recorded_at: String,
    pub void: Option<VoidInfo>,
    pub classification_history: Vec<ClassificationChange>,
}

/// Both date bounds are inclusive. `category` matches the effective category.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Filter {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub kind: Option<Kind>,
    pub category: Option<String>,
    /// Case-insensitive.
    pub source: Option<String>,
    pub is_asset: Option<bool>,
    /// Only entries recorded in this import batch.
    pub batch_id: Option<i64>,
    pub include_voided: bool,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Correction {
    pub voided: Transaction,
    pub replacement: Transaction,
}

pub const MAX_BATCH: usize = 50;

#[derive(Debug, Clone)]
pub enum NewEntry {
    Expense(NewExpense),
    Income(NewIncome),
}

#[derive(Debug, Clone)]
pub struct BatchEntry {
    pub entry: NewEntry,
    pub allow_duplicate: bool,
}

#[derive(Debug, Clone)]
pub struct NewBatch {
    pub label: String,
    pub entries: Vec<BatchEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecordedEntry {
    /// 1-based position in the call.
    pub entry: usize,
    pub transaction: Transaction,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkippedDuplicate {
    /// 1-based position in the call.
    pub entry: usize,
    pub matches_id: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BatchResult {
    pub batch_id: Option<i64>,
    pub label: String,
    pub recorded: Vec<RecordedEntry>,
    pub skipped_duplicates: Vec<SkippedDuplicate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentKind {
    Card,
    BankAccount,
    Cash,
    Other,
}

impl PaymentKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            PaymentKind::Card => "card",
            PaymentKind::BankAccount => "bank_account",
            PaymentKind::Cash => "cash",
            PaymentKind::Other => "other",
        }
    }

    pub(crate) fn from_db(s: &str) -> PaymentKind {
        match s {
            "card" => PaymentKind::Card,
            "bank_account" => PaymentKind::BankAccount,
            "cash" => PaymentKind::Cash,
            _ => PaymentKind::Other,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PaymentKind::Card => "Card",
            PaymentKind::BankAccount => "Bank account",
            PaymentKind::Cash => "Cash",
            PaymentKind::Other => "Other",
        }
    }
}

/// How a caller names a payment method: its id, or its nickname in any letter case.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum PaymentMethodKey {
    Id(i64),
    Nickname(String),
}

#[derive(Debug, Clone)]
pub struct NewPaymentMethod {
    pub kind: PaymentKind,
    pub nickname: String,
    pub last4: Option<String>,
    pub make_default: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PaymentMethod {
    pub id: i64,
    pub kind: PaymentKind,
    /// Current nickname: the latest rename, else the original.
    pub nickname: String,
    pub last4: Option<String>,
    pub is_default: bool,
    pub retired: bool,
    pub created_at: String,
    pub retired_at: Option<String>,
    /// Live (not voided) transactions whose current payment method is this one.
    pub transaction_count: i64,
}

/// What a transaction shows about its payment method.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PaymentMethodRef {
    pub id: i64,
    pub kind: PaymentKind,
    pub nickname: String,
    pub last4: Option<String>,
    pub retired: bool,
}

impl From<&PaymentMethod> for PaymentMethodRef {
    fn from(p: &PaymentMethod) -> Self {
        PaymentMethodRef {
            id: p.id,
            kind: p.kind,
            nickname: p.nickname.clone(),
            last4: p.last4.clone(),
            retired: p.retired,
        }
    }
}
