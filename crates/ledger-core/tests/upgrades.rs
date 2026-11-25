//! Freehold Standard point 8: every ledger format that ever shipped still opens, and a file from a
//! newer app is refused untouched.
mod common;

use common::{earlier_format_fixture, sha256_hex};
use ledger_core::{Filter, Ledger, LedgerError, SUPPORTED_SCHEMA};
use rusqlite::Connection;
use tempfile::TempDir;

#[test]
fn every_earlier_format_upgrades_with_entries_intact() {
    for schema in 1..SUPPORTED_SCHEMA {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("ledger.sqlite3");
        let expected = earlier_format_fixture(&path, schema);

        let l = Ledger::open(&path).unwrap_or_else(|e| panic!("schema {schema}: open failed: {e}"));
        assert_eq!(l.info().unwrap().schema_version, SUPPORTED_SCHEMA, "schema {schema}: not upgraded");

        let mut got: Vec<(String, String, i64, String)> = l
            .list(&Filter::default())
            .unwrap()
            .into_iter()
            .map(|t| (format!("{:?}", t.kind).to_lowercase(), t.occurred_on.to_string(), t.amount_cents, t.description))
            .collect();
        got.sort();
        let mut want: Vec<_> = expected
            .iter()
            .map(|(k, d, a, s)| (k.to_string(), d.to_string(), *a, s.to_string()))
            .collect();
        want.sort();
        assert_eq!(got, want, "schema {schema}: entries changed by the upgrade");

        let report = l.verify().unwrap();
        assert!(report.ok, "schema {schema}: verify found problems: {:?}", report.problems);
    }
}

#[test]
fn a_newer_file_is_refused_and_unchanged() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    drop(Ledger::open(&path).unwrap());
    Connection::open(&path)
        .unwrap()
        .execute("UPDATE schema_meta SET value=?1 WHERE key='schema_version'", [(SUPPORTED_SCHEMA + 1).to_string()])
        .unwrap();
    let before = sha256_hex(&path);
    match Ledger::open(&path) {
        Err(LedgerError::NewerSchema { found, supported }) => {
            assert_eq!((found, supported), (SUPPORTED_SCHEMA + 1, SUPPORTED_SCHEMA));
        }
        other => panic!("expected NewerSchema, got {:?}", other.map(|_| ())),
    }
    assert_eq!(sha256_hex(&path), before, "refused file was modified");
}

#[test]
fn opening_twice_changes_nothing_more() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    earlier_format_fixture(&path, 1);
    let first = Ledger::open(&path).unwrap().info().unwrap();
    let second = Ledger::open(&path).unwrap().info().unwrap();
    assert_eq!(first.schema_version, SUPPORTED_SCHEMA);
    assert_eq!(second.schema_version, first.schema_version);
    assert_eq!(second.transaction_count, first.transaction_count);
    assert_eq!(second.transaction_count, 2);
}
