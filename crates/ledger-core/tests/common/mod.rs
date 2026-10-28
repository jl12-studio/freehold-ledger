#![allow(dead_code)]
use ledger_core::{seed_demo, Ledger};
use tempfile::TempDir;

pub fn empty() -> (TempDir, Ledger) {
    let dir = TempDir::new().unwrap();
    let l = Ledger::open(&dir.path().join("ledger.sqlite3")).unwrap();
    (dir, l)
}

pub fn seeded() -> (TempDir, Ledger) {
    let (d, mut l) = empty();
    seed_demo(&mut l).unwrap();
    (d, l)
}
