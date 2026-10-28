use ledger_core::{
    Category, Filter, Kind, Ledger, LedgerError, LedgerInfo, PaymentMethod, PaymentMethodFilter, Transaction,
};
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
    pub payment_method: Option<String>,
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
        let payment_method = match non_empty(&self.payment_method) {
            None => None,
            Some("none") => Some(PaymentMethodFilter::Unassigned),
            Some(other) => match other.parse::<i64>() {
                Ok(n) => Some(PaymentMethodFilter::Id(n)),
                Err(_) => {
                    return Err(LedgerError::InvalidDate(format!("payment method {other}")));
                }
            },
        };
        Ok(Filter {
            from: non_empty(&self.from).map(ledger_core::parse_date).transpose()?,
            to: non_empty(&self.to).map(ledger_core::parse_date).transpose()?,
            kind,
            category: non_empty(&self.category).map(str::to_string),
            source: non_empty(&self.source).map(str::to_string),
            is_asset: None,
            batch_id: None,
            payment_method,
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

#[tauri::command]
pub fn list_payment_methods(state: State<'_, AppState>) -> Result<Vec<PaymentMethod>, AppError> {
    with_ledger(&state, |l| l.payment_methods(true))
}

/// Current calendar year (UTC), from the system clock.
fn current_year() -> i32 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    // Civil-from-days (Howard Hinnant); only the year is needed.
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let year = yoe + era * 400 + i64::from(mp >= 10);
    year as i32
}

/// Distinct years with any entry (voided included), newest first; the current year is always present.
#[tauri::command]
pub fn ledger_years(state: State<'_, AppState>) -> Result<Vec<i32>, AppError> {
    let f = FilterDto { include_voided: true, ..Default::default() }
        .to_filter()
        .map_err(|e| AppError::from_ledger(&e, &state.path))?;
    let rows = with_ledger(&state, |l| l.list(&f))?;
    let mut years: Vec<i32> = rows
        .iter()
        .filter_map(|t| t.occurred_on.to_string().get(..4)?.parse().ok())
        .collect();
    years.push(current_year());
    years.sort_unstable_by(|a, b| b.cmp(a));
    years.dedup();
    Ok(years)
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportResultDto {
    pub folder: String,
    pub files: Vec<String>,
}

/// Writes the year's export to the default folder; nothing is written on failure.
#[tauri::command]
pub fn export_tax_year(
    state: State<'_, AppState>,
    year: i32,
) -> Result<ExportResultDto, AppError> {
    let r = with_ledger(&state, |l| l.export_tax_year(year, None))?;
    Ok(ExportResultDto { folder: r.folder.display().to_string(), files: r.files })
}

#[derive(Debug, Clone, Serialize)]
pub struct ConnectionInfo {
    pub server_path: String,
    pub bundled: bool,
    pub ledger_path: String,
    pub ledger_path_overridden: bool,
    pub claude_desktop_json: String,
    pub claude_code_command: String,
    pub generic: String,
}

/// Double-quote for a POSIX shell: the paths here may hold spaces, `$`, backticks or quotes.
fn shell_quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        if matches!(c, '"' | '\\' | '$' | '`') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

fn build_connection_info(server: &Path, ledger: &Path, overridden: bool) -> ConnectionInfo {
    let server_path = server.display().to_string();
    let ledger_path = ledger.display().to_string();
    let bundled = server_path.contains(".app/Contents/MacOS");

    let mut entry = serde_json::json!({ "command": server_path });
    if overridden {
        entry["env"] = serde_json::json!({ "FREEHOLD_LEDGER_DB": ledger_path });
    }
    let claude_desktop_json = serde_json::to_string_pretty(
        &serde_json::json!({ "mcpServers": { "freehold-ledger": entry } }),
    )
    .unwrap_or_default();

    let env_flag = if overridden {
        format!("-e FREEHOLD_LEDGER_DB={} ", shell_quote(&ledger_path))
    } else {
        String::new()
    };
    let claude_code_command = format!(
        "claude mcp add freehold-ledger {env_flag}-- {}",
        shell_quote(&server_path)
    );
    let generic = format!(
        "Any MCP client that can launch a stdio server: command {server_path}, no arguments."
    );
    ConnectionInfo {
        server_path,
        bundled,
        ledger_path,
        ledger_path_overridden: overridden,
        claude_desktop_json,
        claude_code_command,
        generic,
    }
}

/// The exact settings a person pastes into their agent. Nothing is written anywhere.
#[tauri::command]
pub fn mcp_connection_info(state: State<'_, AppState>) -> Result<ConnectionInfo, AppError> {
    let io = |e: std::io::Error| AppError {
        kind: "io",
        message: format!("Could not find this app's own location: {e}"),
        path: state.path.display().to_string(),
    };
    let exe = std::env::current_exe().map_err(io)?;
    let dir = exe.parent().ok_or_else(|| io(std::io::ErrorKind::NotFound.into()))?;
    let overridden = std::env::var_os("FREEHOLD_LEDGER_DB").is_some_and(|v| !v.is_empty());
    Ok(build_connection_info(&dir.join("ledger-mcp"), &state.path, overridden))
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
    fn payment_method_filter_parses() {
        let mk = |v: &str| FilterDto { payment_method: Some(v.into()), ..Default::default() };
        assert_eq!(mk("none").to_filter().unwrap().payment_method, Some(PaymentMethodFilter::Unassigned));
        assert_eq!(mk("3").to_filter().unwrap().payment_method, Some(PaymentMethodFilter::Id(3)));
        assert_eq!(mk("").to_filter().unwrap().payment_method, None);
        let err = mk("visa").to_filter().unwrap_err();
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

    #[test]
    fn claude_code_command_quotes_a_bundle_path_with_spaces() {
        let info = build_connection_info(
            Path::new("/Applications/Freehold Ledger.app/Contents/MacOS/ledger-mcp"),
            Path::new("/Users/pat/Ledger/l.sqlite3"),
            false,
        );
        assert!(info.bundled);
        assert_eq!(
            info.claude_code_command,
            "claude mcp add freehold-ledger -- \"/Applications/Freehold Ledger.app/Contents/MacOS/ledger-mcp\""
        );
        assert!(!info.claude_desktop_json.contains("FREEHOLD_LEDGER_DB"));
    }

    #[test]
    fn override_adds_env_to_both_snippets_and_dev_build_is_not_bundled() {
        let info = build_connection_info(
            Path::new("/repo/target/debug/ledger-mcp"),
            Path::new("/tmp/a b/l.sqlite3"),
            true,
        );
        assert!(!info.bundled);
        assert_eq!(
            info.claude_code_command,
            "claude mcp add freehold-ledger -e FREEHOLD_LEDGER_DB=\"/tmp/a b/l.sqlite3\" -- \"/repo/target/debug/ledger-mcp\""
        );
        let v: serde_json::Value = serde_json::from_str(&info.claude_desktop_json).unwrap();
        assert_eq!(v["mcpServers"]["freehold-ledger"]["env"]["FREEHOLD_LEDGER_DB"], "/tmp/a b/l.sqlite3");
        assert_eq!(v["mcpServers"]["freehold-ledger"]["command"], "/repo/target/debug/ledger-mcp");
    }
}
