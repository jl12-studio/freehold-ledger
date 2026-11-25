mod commands;

use std::path::PathBuf;
use std::sync::Mutex;

use commands::AppError;
use ledger_core::{resolve_db_path, Ledger};

pub struct AppState {
    pub ledger: Mutex<Option<Ledger>>,
    pub open_error: Option<AppError>,
    pub path: PathBuf,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let path = resolve_db_path();
    let state = match Ledger::open_default() {
        Ok(ledger) => AppState {
            ledger: Mutex::new(Some(ledger)),
            open_error: None,
            path,
        },
        Err(err) => AppState {
            ledger: Mutex::new(None),
            open_error: Some(AppError::from_ledger(&err, &path)),
            path,
        },
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::ledger_info,
            commands::list_transactions,
            commands::list_categories,
            commands::list_income_sources,
            commands::list_payment_methods,
            commands::list_businesses,
            commands::ledger_years,
            commands::year_entries,
            commands::export_tax_year,
            commands::mcp_connection_info
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
