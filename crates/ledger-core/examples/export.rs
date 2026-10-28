use ledger_core::Ledger;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let year = args.first().and_then(|a| a.to_str()).and_then(|a| a.parse::<i32>().ok());
    let (Some(year), Some(parent)) = (year, args.get(1)) else {
        eprintln!("Usage: FREEHOLD_LEDGER_DB=<ledger> cargo run -p ledger-core --example export -- <year> <dest_parent>");
        return ExitCode::from(2);
    };
    let result = Ledger::open_default().and_then(|l| l.export_tax_year(year, Some(&PathBuf::from(parent))));
    match result {
        Ok(r) => {
            println!("{}", r.folder.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}
