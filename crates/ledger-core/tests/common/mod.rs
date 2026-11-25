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

/// Migration files in order; index 0 is migration 1. Add a line here when a migration is added.
pub const MIGRATION_SQL: [&str; 4] = [
    include_str!("../../migrations/0001_init.sql"),
    include_str!("../../migrations/0002_batches.sql"),
    include_str!("../../migrations/0003_payment_methods.sql"),
    include_str!("../../migrations/0004_businesses.sql"),
];

/// One entry a historic fixture holds: (kind, date, cents, description).
pub type FixtureEntry = (&'static str, &'static str, i64, &'static str);

/// Builds a ledger file as the app shipped at `schema`: only migrations 1..=schema are applied, the
/// version is stored the way db.rs does, and two entries (an expense and an income) are inserted.
/// Returns the entries the file now holds. Panics with the fixture to add when `schema` has none.
pub fn earlier_format_fixture(path: &Path, schema: u32) -> Vec<FixtureEntry> {
    assert!(
        (1..=3).contains(&schema),
        "no fixture for schema {schema}: add its entry builder to earlier_format_fixture in tests/common/mod.rs \
         (and its migration file to MIGRATION_SQL)"
    );
    let c = Connection::open(path).unwrap();
    for sql in MIGRATION_SQL.iter().take(schema as usize) {
        c.execute_batch(sql).unwrap();
    }
    c.execute("INSERT INTO schema_meta (key, value) VALUES ('schema_version', ?1)", [schema.to_string()]).unwrap();
    c.execute("INSERT INTO income_sources (name, created_at) VALUES ('Acme', 'x')", []).unwrap();
    c.execute(
        "INSERT INTO transactions (kind, occurred_on, amount_cents, description, category, recorded_at) \
         VALUES ('expense', '2026-01-02', 30000, 'Standing desk', 'supplies', '2026-01-02T00:00:00Z')",
        [],
    )
    .unwrap();
    c.execute(
        "INSERT INTO transactions (kind, occurred_on, amount_cents, description, category, income_source_id, recorded_at) \
         VALUES ('income', '2026-02-03', 125050, 'Invoice 7', 'sales', 1, '2026-02-03T00:00:00Z')",
        [],
    )
    .unwrap();
    vec![
        ("expense", "2026-01-02", 30000, "Standing desk"),
        ("income", "2026-02-03", 125050, "Invoice 7"),
    ]
}

/// SHA-256 of a file, hex, so tests can prove a file was not touched (no extra crate needed).
pub fn sha256_hex(path: &Path) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];
    let mut msg = std::fs::read(path).unwrap();
    let bits = (msg.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bits.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(chunk[i * 4..i * 4 + 4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), v[0], v[1], v[2], v[3].wrapping_add(t1), v[4], v[5], v[6]];
        }
        for i in 0..8 {
            h[i] = h[i].wrapping_add(v[i]);
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}
