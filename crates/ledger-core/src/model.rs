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
    pub include_voided: bool,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Correction {
    pub voided: Transaction,
    pub replacement: Transaction,
}
