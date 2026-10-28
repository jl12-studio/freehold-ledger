mod common;

use chrono::NaiveDate;
use ledger_core::{LedgerError, NewExpense, SummaryRequest};
use serde_json::Value;
use std::path::Path;

fn read_csv(path: &Path) -> (Vec<String>, Vec<Vec<String>>) {
    let mut rdr = csv::ReaderBuilder::new().from_path(path).unwrap();
    let headers = rdr.headers().unwrap().iter().map(String::from).collect();
    let rows = rdr
        .records()
        .map(|r| r.unwrap().iter().map(String::from).collect())
        .collect();
    (headers, rows)
}

fn col(h: &[String], name: &str) -> usize {
    h.iter().position(|c| c == name).unwrap_or_else(|| panic!("no column {name}"))
}

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

#[test]
fn s_exp_1_seed_2026_export_matches_summarize() {
    let (_d, l) = common::seeded();
    let out = tempfile::tempdir().unwrap();
    let r = l.export_tax_year(2026, Some(out.path())).unwrap();

    assert_eq!(r.folder, out.path().join("Freehold Ledger My business 2026 export"));
    let mut files = r.files.clone();
    files.sort();
    assert_eq!(
        files,
        [
            "README.txt",
            "assets.csv",
            "corrections.csv",
            "expenses-by-category.csv",
            "income-by-source.csv",
            "summary.json",
            "transactions.csv"
        ]
    );
    for f in &files {
        assert!(r.folder.join(f).is_file(), "{f} missing");
    }

    let (h, rows) = read_csv(&r.folder.join("transactions.csv"));
    assert_eq!(
        h.join(","),
        "id,date,kind,description,amount,amount_cents,category_key,category_label,schedule_c_line,is_asset,income_source,status,voided_reason,replaces_id,replaced_by_id,original_category_key,recorded_at,payment_method,payment_method_last4"
    );
    assert_eq!(rows.len(), 9);
    let status = col(&h, "status");
    assert_eq!(rows.iter().filter(|r| r[status] == "voided").count(), 2);
    assert!(rows.iter().all(|r| r[col(&h, "date")].starts_with("2026-")));

    let s = l
        .summarize(&SummaryRequest { from: d("2026-01-01"), to: d("2026-12-31"), category: None, source: None, is_asset: None, payment_method: None, business_id: None })
        .unwrap();

    let (h, rows) = read_csv(&r.folder.join("expenses-by-category.csv"));
    assert_eq!(h.join(","), "schedule_c_line,category_key,category_label,total,total_cents,count");
    let cents: i64 = rows.iter().map(|r| r[col(&h, "total_cents")].parse::<i64>().unwrap()).sum();
    assert_eq!(cents, 68709);
    assert_eq!(cents, s.expenses_total_cents);
    for c in s.by_category.iter().filter(|c| c.kind == ledger_core::Kind::Expense) {
        let row = rows.iter().find(|r| r[col(&h, "category_key")] == c.key).unwrap();
        assert_eq!(row[col(&h, "total_cents")].parse::<i64>().unwrap(), c.total_cents);
        assert_eq!(row[col(&h, "count")].parse::<i64>().unwrap(), c.count);
    }
    let supplies = rows.iter().find(|r| r[col(&h, "category_key")] == "supplies").unwrap();
    assert_eq!(supplies[col(&h, "schedule_c_line")], "22");
    assert_eq!(supplies[col(&h, "total")], "600.00");

    let (h, rows) = read_csv(&r.folder.join("income-by-source.csv"));
    assert_eq!(h.join(","), "income_source,category_key,total,total_cents,count");
    let get = |name: &str| rows.iter().find(|r| r[0] == name).unwrap()[col(&h, "total")].clone();
    assert_eq!(get("Upwork"), "1200.00");
    assert_eq!(get("Etsy"), "85.00");

    let (h, rows) = read_csv(&r.folder.join("assets.csv"));
    assert_eq!(h.join(","), "id,date,description,amount,category_key,schedule_c_line");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0][col(&h, "description")], "Laptop for client work");
    assert_eq!(rows[0][col(&h, "amount")], "300.00");

    let (h, rows) = read_csv(&r.folder.join("corrections.csv"));
    assert_eq!(h.join(","), "transaction_id,action,detail,reason,recorded_at,replacement_id");
    assert_eq!(rows.len(), 3);
    let a = col(&h, "action");
    assert_eq!(rows.iter().filter(|r| r[a] == "voided").count(), 2);
    assert_eq!(rows.iter().filter(|r| r[a] == "reclassified").count(), 1);

    let mut j: Value = serde_json::from_str(&std::fs::read_to_string(r.folder.join("summary.json")).unwrap()).unwrap();
    let o = j.as_object_mut().unwrap();
    for k in ["generated_at", "app_version", "schema_version"] {
        assert!(o.remove(k).is_some(), "{k} missing");
    }
    assert_eq!(o.remove("business"), Some(Value::from("My business")));
    assert_eq!(o.remove("is_test_business"), Some(Value::from(false)));
    assert!(o.get("by_payment_method").is_none());
    let mut expected = serde_json::to_value(&s).unwrap();
    expected.as_object_mut().unwrap().remove("by_payment_method");
    assert_eq!(j, expected);

    let readme = std::fs::read_to_string(r.folder.join("README.txt")).unwrap();
    assert!(!readme.contains('\u{2014}'));
    assert!(readme.contains("does not give tax advice"));
    assert!(readme.contains("Schedule C"));
}

#[test]
fn s_exp_2_empty_year_has_headers_and_says_so() {
    let (_d, l) = common::seeded();
    let out = tempfile::tempdir().unwrap();
    let r = l.export_tax_year(2024, Some(out.path())).unwrap();
    for f in ["transactions.csv", "expenses-by-category.csv", "income-by-source.csv", "assets.csv", "corrections.csv"] {
        let (h, rows) = read_csv(&r.folder.join(f));
        assert!(!h.is_empty(), "{f} header");
        assert!(rows.is_empty(), "{f} should be header only");
    }
    let readme = std::fs::read_to_string(r.folder.join("README.txt")).unwrap();
    assert_eq!(readme.lines().next().unwrap(), "No entries were recorded for 2024.");
}

#[cfg(unix)]
#[test]
fn s_exp_3_unwritable_destination_leaves_nothing() {
    use std::os::unix::fs::PermissionsExt;
    let (_d, l) = common::seeded();
    let out = tempfile::tempdir().unwrap();
    let ro = out.path().join("locked");
    std::fs::create_dir(&ro).unwrap();
    std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o555)).unwrap();

    let res = l.export_tax_year(2026, Some(&ro));
    let left: Vec<_> = std::fs::read_dir(&ro).unwrap().collect();
    std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o755)).unwrap();

    match res {
        Err(LedgerError::ExportFailed { .. }) => {}
        other => panic!("expected ExportFailed, got {other:?}"),
    }
    assert!(left.is_empty(), "nothing should be left behind");
}

#[test]
fn s_exp_4_second_export_gets_numbered_folder() {
    let (_d, l) = common::seeded();
    let out = tempfile::tempdir().unwrap();
    let first = l.export_tax_year(2026, Some(out.path())).unwrap();
    let marker = first.folder.join("transactions.csv");
    let before = std::fs::read(&marker).unwrap();
    let second = l.export_tax_year(2026, Some(out.path())).unwrap();
    assert_eq!(second.folder, out.path().join("Freehold Ledger My business 2026 export (2)"));
    let third = l.export_tax_year(2026, Some(out.path())).unwrap();
    assert_eq!(third.folder, out.path().join("Freehold Ledger My business 2026 export (3)"));
    assert_eq!(std::fs::read(&marker).unwrap(), before);
}

#[test]
fn csv_escapes_commas_quotes_and_newlines() {
    let (_d, mut l) = common::empty();
    let desc = "Booth rent, \"winter\" market\nsecond line";
    l.record_expense(NewExpense {
        occurred_on: Some(d("2026-02-02")),
        amount_cents: 12345,
        description: desc.to_string(),
        category: "rent_other".to_string(),
        is_asset: false,
        payment_method: None,
    })
    .unwrap();
    let out = tempfile::tempdir().unwrap();
    let r = l.export_tax_year(2026, Some(out.path())).unwrap();
    let raw = std::fs::read_to_string(r.folder.join("transactions.csv")).unwrap();
    assert!(raw.contains("\"Booth rent, \"\"winter\"\" market\nsecond line\""), "{raw}");
    let (h, rows) = read_csv(&r.folder.join("transactions.csv"));
    assert_eq!(rows[0][col(&h, "description")], desc);
    assert_eq!(rows[0][col(&h, "amount")], "123.45");
}

#[test]
fn readme_alone_answers_line_22_and_which_entries_are_assets() {
    let (_d, l) = common::seeded();
    let out = tempfile::tempdir().unwrap();
    let r = l.export_tax_year(2026, Some(out.path())).unwrap();
    let readme = std::fs::read_to_string(r.folder.join("README.txt")).unwrap();
    // Seeded 2026: Supplies (line 22) is the $300 laptop plus the $300 corrected monitor.
    assert!(readme.contains("- Line 22 Supplies (supplies): $600.00 from 2 entries"), "{readme}");
    assert!(readme.contains("- Line 18 Office expense (office_expense): $42.10 from 1 entry"), "{readme}");
    assert!(readme.contains("- Upwork (gross_receipts): $1200.00 from 1 entry"), "{readme}");
    assert!(
        readme.contains("- Entry 1, 2026-10-20, Laptop for client work: $300.00, category supplies, Schedule C line 22"),
        "{readme}"
    );
    assert!(readme.contains("also included in its category's total"), "{readme}");
    assert!(!readme.contains('\u{2014}'));

    let empty = l.export_tax_year(2024, Some(out.path())).unwrap();
    let readme = std::fs::read_to_string(empty.folder.join("README.txt")).unwrap();
    assert_eq!(readme.matches("- None.").count(), 3, "{readme}");
}

#[test]
fn s_exp_5_transactions_csv_names_the_payment_method() {
    use ledger_core::{NewPaymentMethod, PaymentKind, PaymentMethodKey};
    let (_d, mut l) = common::seeded();
    l.create_payment_method(NewPaymentMethod { kind: PaymentKind::Card, nickname: "Business Visa".into(), last4: Some("4242".into()), make_default: false }).unwrap();
    l.create_payment_method(NewPaymentMethod { kind: PaymentKind::Cash, nickname: "Cash".into(), last4: None, make_default: false }).unwrap();
    l.assign_payment_method(&PaymentMethodKey::Nickname("Business Visa".into()), &[1, 10]).unwrap();
    l.assign_payment_method(&PaymentMethodKey::Nickname("Cash".into()), &[4]).unwrap();
    let out = tempfile::tempdir().unwrap();
    let r = l.export_tax_year(2026, Some(out.path())).unwrap();
    let (h, rows) = read_csv(&r.folder.join("transactions.csv"));
    let (id, pm, l4) = (col(&h, "id"), col(&h, "payment_method"), col(&h, "payment_method_last4"));
    let cell = |n: &str, c: usize| rows.iter().find(|r| r[id] == n).unwrap()[c].clone();
    assert_eq!((cell("1", pm), cell("1", l4)), ("Business Visa".to_string(), "4242".to_string()));
    assert_eq!((cell("10", pm), cell("10", l4)), ("Business Visa".to_string(), "4242".to_string()));
    assert_eq!((cell("4", pm), cell("4", l4)), ("Cash".to_string(), String::new()));
    assert_eq!((cell("5", pm), cell("5", l4)), (String::new(), String::new()));
    let readme = std::fs::read_to_string(r.folder.join("README.txt")).unwrap();
    assert!(readme.contains("payment_method_last4"));
    assert!(!readme.contains('\u{2014}'));
    let j: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(r.folder.join("summary.json")).unwrap()).unwrap();
    assert!(j.get("by_payment_method").is_none());
}

fn test_shop_with_expense(l: &mut ledger_core::Ledger) {
    l.create_business(ledger_core::NewBusiness { name: "Test shop".into(), note: None, is_test: true, make_current: false })
        .unwrap();
    l.record_expense_in(
        Some(&ledger_core::BusinessKey::Name("test shop".into())),
        NewExpense {
            occurred_on: Some(d("2026-06-01")),
            amount_cents: 1200,
            description: "Shop thing".into(),
            category: "supplies".into(),
            is_asset: false,
            payment_method: None,
        },
    )
    .unwrap();
}

#[test]
fn s_exp_7_export_names_the_business() {
    let (_d, mut l) = common::seeded();
    test_shop_with_expense(&mut l);
    let out = tempfile::tempdir().unwrap();
    let r = l.export_tax_year(2026, Some(out.path())).unwrap();
    assert_eq!(r.folder, out.path().join("Freehold Ledger My business 2026 export"));
    assert_eq!(r.business.name, "My business");
    let (h, rows) = read_csv(&r.folder.join("transactions.csv"));
    assert_eq!(rows.len(), 9);
    assert!(rows.iter().all(|row| row[col(&h, "description")] != "Shop thing"));
    let readme = std::fs::read_to_string(r.folder.join("README.txt")).unwrap();
    assert!(readme.contains("Business: My business."));
    assert!(!readme.contains("This is a test business"));
}

#[test]
fn s_exp_8_test_business_export_says_test() {
    let (_d, mut l) = common::seeded();
    test_shop_with_expense(&mut l);
    let out = tempfile::tempdir().unwrap();
    let key = ledger_core::BusinessKey::Name("test shop".into());
    let r = l.export_tax_year_in(2026, Some(&key), Some(out.path())).unwrap();
    assert_eq!(r.folder, out.path().join("Freehold Ledger Test shop (test) 2026 export"));
    let (_h, rows) = read_csv(&r.folder.join("transactions.csv"));
    assert_eq!(rows.len(), 1);
    let j: Value = serde_json::from_str(&std::fs::read_to_string(r.folder.join("summary.json")).unwrap()).unwrap();
    assert_eq!(j["business"], "Test shop");
    assert_eq!(j["is_test_business"], true);
    assert_eq!(j["expenses_total_cents"], 1200);
    let readme = std::fs::read_to_string(r.folder.join("README.txt")).unwrap();
    assert!(readme.contains("Business: Test shop."));
    assert!(readme.contains(
        "This is a test business: its entries are for trying things out and are not real books."
    ));
    assert!(!readme.contains('\u{2014}'));
}

#[test]
fn s_exp_9_unsafe_name_is_made_folder_safe() {
    let (_d, mut l) = common::seeded();
    let out = tempfile::tempdir().unwrap();
    let key = ledger_core::BusinessKey::Id(1);
    l.rename_business(&key, "A/B: \"C\"?").unwrap();
    let r = l.export_tax_year(2026, Some(out.path())).unwrap();
    assert_eq!(r.folder, out.path().join("Freehold Ledger A-B- -C-- 2026 export"));
    l.rename_business(&key, "...").unwrap();
    let r = l.export_tax_year(2026, Some(out.path())).unwrap();
    assert_eq!(r.folder, out.path().join("Freehold Ledger Business 2026 export"));
}

#[test]
fn s_exp_10_no_current_business_writes_nothing() {
    let (_d, mut l) = common::seeded();
    l.archive_business(&ledger_core::BusinessKey::Id(1)).unwrap();
    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("exports");
    assert!(matches!(l.export_tax_year(2026, Some(&dest)), Err(LedgerError::NoCurrentBusiness)));
    assert!(!dest.exists() || std::fs::read_dir(&dest).unwrap().next().is_none());
}
