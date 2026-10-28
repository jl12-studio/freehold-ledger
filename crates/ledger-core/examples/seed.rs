use ledger_core::{seed_demo, Ledger};
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(arg) = std::env::args_os().nth(1) else {
        eprintln!("Usage: cargo run -p ledger-core --example seed -- <path-to-ledger-file>");
        return ExitCode::from(2);
    };
    let path = PathBuf::from(arg);
    let mut ledger = match Ledger::open(&path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    };
    match ledger.info() {
        Ok(info) if info.transaction_count > 0 => {
            eprintln!("{} already has transactions; refusing to seed it.", path.display());
            return ExitCode::from(2);
        }
        Ok(_) => {}
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    }
    if let Err(e) = seed_demo(&mut ledger) {
        eprintln!("{e}");
        return ExitCode::from(1);
    }
    match ledger.info() {
        Ok(info) => {
            println!("{}", info.path);
            println!("{} transactions", info.transaction_count);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}
