#![allow(dead_code)]
use ledger_core::{seed_demo, Ledger};
use rusqlite::Connection;
use std::path::Path;
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

pub const INSERT_TX: &str = "INSERT INTO transactions (kind, occurred_on, amount_cents, description, category, recorded_at) \
     VALUES ('expense', '2026-01-02', 30000, 'device', 'supplies', '2026-01-02T00:00:00Z')";

/// A hand-built schema 3 file: 3 transactions (2 voided, 3 replaces 2), a batch holding 1 and 3,
/// two payment methods, the default set to method 1 then 2, and one assignment.
pub fn v3_fixture(path: &Path, last_default_null: bool) {
    let c = Connection::open(path).unwrap();
    c.execute_batch(include_str!("../../migrations/0001_init.sql")).unwrap();
    c.execute_batch(include_str!("../../migrations/0002_batches.sql")).unwrap();
    c.execute_batch(include_str!("../../migrations/0003_payment_methods.sql")).unwrap();
    c.execute("INSERT INTO schema_meta (key, value) VALUES ('schema_version','3')", []).unwrap();
    c.execute(INSERT_TX, []).unwrap();
    c.execute(INSERT_TX, []).unwrap();
    c.execute(
        "INSERT INTO transactions (kind, occurred_on, amount_cents, description, category, replaces_id, recorded_at) \
         VALUES ('expense', '2026-01-02', 30000, 'device', 'supplies', 2, '2026-01-02T00:00:00Z')",
        [],
    )
    .unwrap();
    c.execute("INSERT INTO voids (transaction_id, reason, recorded_at) VALUES (2, 'wrong', '2026-01-03T00:00:00Z')", []).unwrap();
    c.execute("INSERT INTO batches (label, recorded_at) VALUES ('f.csv', '2026-01-03T00:00:00Z')", []).unwrap();
    c.execute("INSERT INTO transaction_batches (transaction_id, batch_id) VALUES (1, 1)", []).unwrap();
    c.execute("INSERT INTO transaction_batches (transaction_id, batch_id) VALUES (3, 1)", []).unwrap();
    c.execute("INSERT INTO payment_methods (kind, nickname, last4, created_at) VALUES ('card','Visa','4242','x')", []).unwrap();
    c.execute("INSERT INTO payment_methods (kind, nickname, created_at) VALUES ('cash','Cash','x')", []).unwrap();
    c.execute("INSERT INTO default_payment_method_changes (payment_method_id, recorded_at) VALUES (1,'2026-02-01T00:00:00Z')", []).unwrap();
    let last = if last_default_null { "NULL" } else { "2" };
    c.execute(
        &format!("INSERT INTO default_payment_method_changes (payment_method_id, recorded_at) VALUES ({last},'2026-02-02T00:00:00Z')"),
        [],
    )
    .unwrap();
    c.execute("INSERT INTO payment_method_assignments (transaction_id, payment_method_id, recorded_at) VALUES (1,1,'x')", []).unwrap();
}
