mod common;

use ledger_core::{
    check_no_full_number, BatchEntry, Filter, LedgerError, NewBatch, NewEntry, NewExpense, NewIncome, NewPaymentMethod,
    PaymentKind, PaymentMethodKey, Replacement,
};

fn expense(description: &str) -> NewExpense {
    NewExpense {
        occurred_on: None,
        amount_cents: 1000,
        description: description.to_string(),
        category: "office_expense".to_string(),
        is_asset: false,
        payment_method: None,
    }
}

fn is_card(text: &str) -> bool {
    matches!(check_no_full_number("description", text), Err(LedgerError::CardNumberInText { .. }))
}

fn is_account(text: &str) -> bool {
    matches!(check_no_full_number("description", text), Err(LedgerError::AccountNumberInText { .. }))
}

#[test]
fn s_pan_1_card_numbers_refused() {
    for t in [
        "4242424242424242",
        "4242 4242 4242 4242",
        "4242-4242-4242-4242",
        "Visa 4242 4242 4242 4242 exp 12/27 cvv 123",
        "5555 5555 5555 4444",
        "3782 822463 10005",
        "6011111111111117",
        "4000 0566 5566 5556",
        "4222222222222",
        "paid with card4111111111111111",
    ] {
        assert!(is_card(t), "expected card refusal for {t}");
    }
}

#[test]
fn s_pan_2_account_numbers_refused() {
    for t in [
        "4242 4242 4242 4241",
        "acct 000123456789012",
        "021000021 1234567890",
        "1234567890123",
        "42424242424242424242",
    ] {
        assert!(is_account(t), "expected account refusal for {t}");
    }
}

#[test]
fn s_pan_3_ordinary_numbers_pass() {
    for t in [
        "$1,200.50",
        "2026-10-28",
        "Paid 2026-01-04 and 2026-01-05",
        "Invoice 123456789012",
        "Order 1234-5678-9012",
        "Call +1 512 555 0100",
        "ZIP 78701-1234",
        "Card ending 4242",
        "routing 021000021 then account 123456789",
        "",
    ] {
        assert!(check_no_full_number("description", t).is_ok(), "expected pass for {t:?}");
    }
}

#[test]
fn s_pan_4_message_teaches_and_hides_digits() {
    let card = check_no_full_number("description", "Visa 4242 4242 4242 4242").unwrap_err().to_string();
    let account = check_no_full_number("description", "acct 000123456789012").unwrap_err().to_string();
    println!("{card}");
    for m in [card, account] {
        assert!(m.contains("last 4 digits"), "{m}");
        assert!(m.contains("nothing was stored"), "{m}");
        // The mandated wording itself says "last 4 digits" and "last4"; nothing else may be a digit.
        let rest = m.replace("last 4 digits", "").replace("last4", "");
        assert!(!rest.chars().any(|c| c.is_ascii_digit()), "{m}");
        assert!(!m.contains('\u{2014}'), "{m}");
    }
}

#[test]
fn s_pan_5_core_writes_store_nothing() {
    let (dir, mut l) = common::empty();
    let field = |e: LedgerError| match e {
        LedgerError::CardNumberInText { field } | LedgerError::AccountNumberInText { field } => field,
        other => panic!("unexpected error {other:?}"),
    };

    let e = l.record_expense(expense("Visa 4242 4242 4242 4242")).unwrap_err();
    assert!(matches!(e, LedgerError::CardNumberInText { .. }));
    assert_eq!(field(e), "description");

    let e = l
        .record_income(NewIncome {
            occurred_on: None,
            amount_cents: 5000,
            description: "Client payment".to_string(),
            category: None,
            source: "Acct 000123456789012".to_string(),
            payment_method: None,
        })
        .unwrap_err();
    assert!(matches!(e, LedgerError::AccountNumberInText { .. }));
    assert_eq!(field(e), "source");

    let t = l.record_expense(expense("Printer paper")).unwrap();
    let bad = "dup of 4242424242424242";
    assert_eq!(field(l.void(t.id, bad).unwrap_err()), "reason");
    assert_eq!(field(l.reclassify(t.id, "office_expense", bad).unwrap_err()), "reason");
    let change = Replacement {
        occurred_on: None,
        amount_cents: Some(2000),
        description: None,
        category: None,
        is_asset: None,
        source: None,
        payment_method: None,
    };
    assert_eq!(field(l.correct(t.id, bad, change).unwrap_err()), "reason");

    let e = l
        .record_batch(NewBatch {
            label: "statement 4242 4242 4242 4242".to_string(),
            payment_method: None,
            entries: vec![BatchEntry { entry: NewEntry::Expense(expense("Stamps")), allow_duplicate: false, payment_method: None }],
        })
        .unwrap_err();
    assert!(matches!(e, LedgerError::CardNumberInText { .. }));
    assert_eq!(field(e), "label");

    let e = l
        .record_batch(NewBatch {
            label: "statement".to_string(),
            payment_method: None,
            entries: vec![BatchEntry {
                entry: NewEntry::Expense(expense("4242 4242 4242 4242")),
                allow_duplicate: false, payment_method: None,
            }],
        })
        .unwrap_err();
    match e {
        LedgerError::BatchEntry { index, ref reason } => {
            assert_eq!(index, 1);
            assert!(matches!(**reason, LedgerError::CardNumberInText { .. }));
        }
        ref other => panic!("unexpected error {other:?}"),
    }
    assert!(e.to_string().contains("full card number"), "{e}");

    assert_eq!(l.info().unwrap().transaction_count, 1);
    let rows = l.list(&Filter { include_voided: true, ..Default::default() }).unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].void.is_none());
    assert!(rows[0].classification_history.is_empty());
    assert!(l.income_sources().unwrap().is_empty());
    let conn = rusqlite::Connection::open(dir.path().join("ledger.sqlite3")).unwrap();
    let batches: i64 = conn.query_row("SELECT count(*) FROM batches", [], |r| r.get(0)).unwrap();
    assert_eq!(batches, 0);
}

#[test]
fn s_pan_6_ordinary_text_records() {
    let (_d, mut l) = common::empty();
    let t = l.record_expense(expense("Invoice 123456789012 from 2026-01-04, $1,200.50")).unwrap();
    assert_eq!(t.description, "Invoice 123456789012 from 2026-01-04, $1,200.50");
}

#[test]
fn s_pan_8_unicode_card_numbers_refused() {
    for t in [
        "４２４２ ４２４２ ４２４２ ４２４２",
        "４２４２４２４２４２４２４２４２",
        "Visa ４２４２ ４２４２ ４２４２ ４２４２ exp 12/27",
        "٤٢٤٢ ٤٢٤٢ ٤٢٤٢ ٤٢٤٢",
        "۴۲۴۲-۴۲۴۲-۴۲۴۲-۴۲۴۲",
        "४२४२४२४२४२४२४२४२",
        "4242 ４２４２ ٤٢٤٢ ۴۲۴۲",
        "42４2424242424242",
    ] {
        assert!(is_card(t), "expected card refusal for {t}");
    }
}

#[test]
fn s_pan_9_unicode_account_numbers_refused() {
    for t in ["１２３４５６７８９０１２３", "acct ٠٠٠١٢٣٤٥٦٧٨٩٠١٢", "４２４２ ４２４２ ４２４２ ４２４１"] {
        assert!(is_account(t), "expected account refusal for {t}");
    }
}

#[test]
fn s_pan_10_unicode_ordinary_numbers_pass() {
    for t in ["Invoice １２３４５６７８９０１２", "２０２６-１０-２８", "٢٠٢٦-١٠-٢٨", "Card ending ４２４２", "Order ١٢٣٤-٥٦٧٨-٩٠١٢"] {
        assert!(check_no_full_number("description", t).is_ok(), "expected pass for {t:?}");
    }
}

#[test]
fn s_pan_11_unicode_refusals_hide_every_digit() {
    for t in ["Visa ４２４２ ４２４２ ４２４２ ４２４２", "acct ٠٠٠١٢٣٤٥٦٧٨٩٠١٢", "४२४२४२४२४२४२४२४२"] {
        let m = check_no_full_number("description", t).unwrap_err().to_string();
        assert!(m.contains("last 4 digits") && m.contains("nothing was stored"), "{m}");
        let rest = m.replace("last 4 digits", "").replace("last4", "");
        assert!(!rest.chars().any(char::is_numeric), "{m}");
        assert!(!m.contains('\u{2014}'), "{m}");
    }
}

#[test]
fn s_pan_12_unicode_digits_in_payment_method_fields() {
    let (_d, mut l) = common::empty();
    let pm = |nickname: &str, last4: Option<&str>| NewPaymentMethod {
        kind: PaymentKind::Card,
        nickname: nickname.to_string(),
        last4: last4.map(String::from),
        make_default: false,
    };
    let e = l.create_payment_method(pm("Visa ４２４２ ４２４２ ４２４２ ４２４２", None)).unwrap_err();
    assert!(matches!(e, LedgerError::CardNumberInText { ref field } if field == "nickname"), "{e}");
    let e = l.create_payment_method(pm("Acct ١٢٣٤٥٦٧٨٩٠١٢٣", None)).unwrap_err();
    assert!(matches!(e, LedgerError::AccountNumberInText { ref field } if field == "nickname"), "{e}");
    // last4 stays exactly 4 ASCII digits (D29); a fullwidth full number is refused by the guard first.
    let e = l.create_payment_method(pm("Visa", Some("４２４２"))).unwrap_err();
    assert!(matches!(e, LedgerError::InvalidLast4), "{e}");
    let e = l.create_payment_method(pm("Visa", Some("４２４２ ４２４２ ４２４２ ４２４２"))).unwrap_err();
    assert!(matches!(e, LedgerError::CardNumberInText { ref field } if field == "last4"), "{e}");
    l.create_payment_method(pm("Biz Visa", Some("4242"))).unwrap();
    let e = l
        .rename_payment_method(&PaymentMethodKey::Nickname("Biz Visa".into()), "۴۲۴۲-۴۲۴۲-۴۲۴۲-۴۲۴۲")
        .unwrap_err();
    assert!(matches!(e, LedgerError::CardNumberInText { ref field } if field == "nickname"), "{e}");
    let conn = rusqlite::Connection::open(l.path()).unwrap();
    let methods: i64 = conn.query_row("SELECT count(*) FROM payment_methods", [], |r| r.get(0)).unwrap();
    let changes: i64 = conn.query_row("SELECT count(*) FROM payment_method_changes", [], |r| r.get(0)).unwrap();
    assert_eq!((methods, changes), (1, 0));
}
