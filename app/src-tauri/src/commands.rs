use ledger_core::{Category, Filter, Kind, Ledger, LedgerError, LedgerInfo, Transaction};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::State;

use crate::AppState;

/// What the window shows when the ledger could not be opened.
#[derive(Debug, Clone, Serialize)]
pub struct AppError {
    /// "not_a_ledger" | "newer_schema" | "io" | "invalid_input" | "other"
    pub kind: &'static str,
    pub message: String,
    pub path: String,
}

impl AppError {
    pub fn from_ledger(err: &LedgerError, path: &Path) -> AppError {
        let kind = match err {
            LedgerError::NotALedger { .. } => "not_a_ledger",
            LedgerError::NewerSchema { .. } => "newer_schema",
            LedgerError::Io(_) => "io",
            LedgerError::InvalidDate(_) => "invalid_input",
            _ => "other",
        };
        AppError {
            kind,
            message: err.to_string(),
            path: path.display().to_string(),
        }
    }
}

#[tauri::command]
pub fn ledger_info(state: State<'_, AppState>) -> Result<LedgerInfo, AppError> {
    if let Some(err) = &state.open_error {
        return Err(err.clone());
    }
    let guard = state.ledger.lock().map_err(|_| AppError {
        kind: "other",
        message: "The ledger is busy. Close and reopen the app.".into(),
        path: state.path.display().to_string(),
    })?;
    match guard.as_ref() {
        Some(ledger) => ledger
            .info()
            .map_err(|e| AppError::from_ledger(&e, &state.path)),
        None => Err(AppError {
            kind: "other",
            message: "The ledger is not open.".into(),
            path: state.path.display().to_string(),
        }),
    }
}

/// Filter as the window sends it. Dates are "YYYY-MM-DD" strings.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct FilterDto {
    pub from: Option<String>,
    pub to: Option<String>,
    pub kind: Option<String>,
    pub category: Option<String>,
    pub source: Option<String>,
    #[serde(default)]
    pub include_voided: bool,
}

fn non_empty(v: &Option<String>) -> Option<&str> {
    v.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

impl FilterDto {
    pub fn to_filter(&self) -> Result<Filter, LedgerError> {
        let kind = match non_empty(&self.kind) {
            None => None,
            Some("expense") => Some(Kind::Expense),
            Some("income") => Some(Kind::Income),
            Some(other) => {
                return Err(LedgerError::InvalidDate(format!("kind {other}")));
            }
        };
        Ok(Filter {
            from: non_empty(&self.from).map(ledger_core::parse_date).transpose()?,
            to: non_empty(&self.to).map(ledger_core::parse_date).transpose()?,
            kind,
            category: non_empty(&self.category).map(str::to_string),
            source: non_empty(&self.source).map(str::to_string),
            is_asset: None,
            include_voided: self.include_voided,
            limit: None,
        })
    }
}

fn with_ledger<T>(
    state: &State<'_, AppState>,
    f: impl FnOnce(&Ledger) -> Result<T, LedgerError>,
) -> Result<T, AppError> {
    if let Some(err) = &state.open_error {
        return Err(err.clone());
    }
    let guard = state.ledger.lock().map_err(|_| AppError {
        kind: "other",
        message: "The ledger is busy. Close and reopen the app.".into(),
        path: state.path.display().to_string(),
    })?;
    match guard.as_ref() {
        Some(ledger) => f(ledger).map_err(|e| AppError::from_ledger(&e, &state.path)),
        None => Err(AppError {
            kind: "other",
            message: "The ledger is not open.".into(),
            path: state.path.display().to_string(),
        }),
    }
}

#[tauri::command]
pub fn list_transactions(
    state: State<'_, AppState>,
    filter: FilterDto,
) -> Result<Vec<Transaction>, AppError> {
    let f = filter.to_filter().map_err(|e| AppError::from_ledger(&e, &state.path))?;
    with_ledger(&state, |l| l.list(&f))
}

#[tauri::command]
pub fn list_categories() -> Vec<Category> {
    ledger_core::categories().to_vec()
}

#[tauri::command]
pub fn list_income_sources(state: State<'_, AppState>) -> Result<Vec<String>, AppError> {
    with_ledger(&state, |l| l.income_sources())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ledger_core::Ledger;

    #[test]
    fn bad_filter_date_is_invalid_input() {
        let dto = FilterDto { from: Some("07/01/2026".into()), ..Default::default() };
        let err = dto.to_filter().unwrap_err();
        let path = std::path::PathBuf::from("/x");
        assert_eq!(AppError::from_ledger(&err, &path).kind, "invalid_input");
    }

    #[test]
    fn blank_filter_fields_mean_no_filter() {
        let dto = FilterDto { from: Some("".into()), kind: Some("income".into()), include_voided: true, ..Default::default() };
        let f = dto.to_filter().unwrap();
        assert!(f.from.is_none() && f.kind == Some(Kind::Income) && f.include_voided);
    }

    #[test]
    fn garbage_file_maps_to_not_a_ledger_naming_the_path() {
        let dir = std::env::temp_dir().join(format!("fl-app-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.sqlite3");
        std::fs::write(&path, [7u8; 64]).unwrap();
        let err = match Ledger::open(&path) {
            Ok(_) => panic!("garbage file must not open"),
            Err(e) => AppError::from_ledger(&e, &path),
        };
        assert_eq!(err.kind, "not_a_ledger");
        assert!(err.message.contains("Nothing was changed."));
        assert_eq!(err.path, path.display().to_string());
        std::fs::remove_dir_all(&dir).ok();
    }
}
