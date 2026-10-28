mod common;

use ledger_core::{Ledger, LedgerError, NewPaymentMethod, PaymentKind, PaymentMethodKey};
use rusqlite::Connection;

fn new(kind: PaymentKind, nickname: &str, last4: Option<&str>, make_default: bool) -> NewPaymentMethod {
    NewPaymentMethod { kind, nickname: nickname.into(), last4: last4.map(String::from), make_default }
}

fn card(l: &mut Ledger, name: &str, last4: &str) -> ledger_core::PaymentMethod {
    l.create_payment_method(new(PaymentKind::Card, name, Some(last4), false)).unwrap()
}

fn nick(s: &str) -> PaymentMethodKey {
    PaymentMethodKey::Nickname(s.into())
}

fn count(l: &Ledger, table: &str) -> i64 {
    Connection::open(l.path()).unwrap().query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0)).unwrap()
}

#[test]
fn s_pmt_1_create_and_list() {
    let (_d, mut l) = common::empty();
    l.create_payment_method(new(PaymentKind::Card, "Business Visa", Some("4242"), true)).unwrap();
    l.create_payment_method(new(PaymentKind::BankAccount, "Checking", Some("6789"), false)).unwrap();
    l.create_payment_method(new(PaymentKind::Cash, "Cash", None, false)).unwrap();
    let list = l.payment_methods(false).unwrap();
    let names: Vec<_> = list.iter().map(|p| p.nickname.as_str()).collect();
    assert_eq!(names, ["Business Visa", "Cash", "Checking"]);
    assert!(list[0].is_default && !list[1].is_default && !list[2].is_default);
    assert!(list.iter().all(|p| p.transaction_count == 0));
    assert_eq!(l.default_payment_method().unwrap().unwrap().nickname, "Business Visa");
}

#[test]
fn s_pmt_2_last4_is_exactly_4_digits() {
    let (_d, mut l) = common::empty();
    for bad in ["42424", "42a2", "42-4", ""] {
        let e = l.create_payment_method(new(PaymentKind::Card, "X", Some(bad), false)).unwrap_err();
        assert!(matches!(e, LedgerError::InvalidLast4), "{bad}: {e}");
    }
    let p = l.create_payment_method(new(PaymentKind::Card, "X", Some(" 4242 "), false)).unwrap();
    assert_eq!(p.last4.as_deref(), Some("4242"));
    let e = l.create_payment_method(new(PaymentKind::Card, "Y", Some("4242 4242 4242 4242"), false)).unwrap_err();
    assert!(matches!(e, LedgerError::CardNumberInText { ref field } if field == "last4"), "{e}");
    let e = l.create_payment_method(new(PaymentKind::Cash, "Z", Some("1234"), false)).unwrap_err();
    assert!(matches!(e, LedgerError::Last4NotForCash), "{e}");
    assert_eq!(count(&l, "payment_methods"), 1);
}

#[test]
fn s_pmt_3_nickname_rules() {
    let (_d, mut l) = common::empty();
    card(&mut l, "Business Visa", "4242");
    let mk = |l: &mut Ledger, n: &str| l.create_payment_method(new(PaymentKind::Other, n, None, false)).unwrap_err();
    assert!(matches!(mk(&mut l, "   "), LedgerError::MissingNickname));
    assert!(matches!(mk(&mut l, &"a".repeat(41)), LedgerError::NicknameTooLong));
    assert!(matches!(mk(&mut l, "None"), LedgerError::NicknameReserved));
    assert!(matches!(mk(&mut l, "business visa"), LedgerError::NicknameTaken(ref n) if n == "Business Visa"));
    assert!(matches!(
        mk(&mut l, "Visa 4242 4242 4242 4242"),
        LedgerError::CardNumberInText { ref field } if field == "nickname"
    ));
    assert_eq!(count(&l, "payment_methods"), 1);
}

#[test]
fn s_pmt_4_rename() {
    let (_d, mut l) = common::empty();
    card(&mut l, "Business Visa", "4242");
    l.create_payment_method(new(PaymentKind::BankAccount, "Checking", None, false)).unwrap();
    let p = l.rename_payment_method(&nick("Business Visa"), "Biz Visa").unwrap();
    assert_eq!(p.nickname, "Biz Visa");
    assert_eq!(l.payment_methods(false).unwrap()[0].nickname, "Biz Visa");
    let original: String = Connection::open(l.path())
        .unwrap()
        .query_row("SELECT nickname FROM payment_methods WHERE id = 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(original, "Business Visa");
    assert_eq!(count(&l, "payment_method_changes"), 1);
    let e = l.rename_payment_method(&PaymentMethodKey::Id(1), "checking").unwrap_err();
    assert!(matches!(e, LedgerError::NicknameTaken(ref n) if n == "Checking"), "{e}");
    assert_eq!(l.find_payment_method(&nick("biz visa")).unwrap().id, 1);
    assert_eq!(l.find_payment_method(&PaymentMethodKey::Id(1)).unwrap().nickname, "Biz Visa");
    // D45 (S-REN-1): a change of letter case alone is a rename and writes one row.
    let p = l.rename_payment_method(&PaymentMethodKey::Id(1), "BIZ VISA").unwrap();
    assert_eq!(p.nickname, "BIZ VISA");
    assert_eq!(count(&l, "payment_method_changes"), 2);
    assert_eq!(l.find_payment_method(&nick("biz visa")).unwrap().nickname, "BIZ VISA");
    // S-REN-2: the identical name, even with surrounding spaces, writes nothing.
    let p = l.rename_payment_method(&nick("BIZ VISA"), "  BIZ VISA ").unwrap();
    assert_eq!(p.nickname, "BIZ VISA");
    assert_eq!(count(&l, "payment_method_changes"), 2);
    // S-REN-3: another active method's name in a different case is still taken.
    let e = l.rename_payment_method(&PaymentMethodKey::Id(1), "CHECKING").unwrap_err();
    assert!(matches!(e, LedgerError::NicknameTaken(ref n) if n == "Checking"), "{e}");
    assert_eq!(count(&l, "payment_method_changes"), 2);
}

#[test]
fn s_pmt_5_retire() {
    let (_d, mut l) = common::empty();
    l.create_payment_method(new(PaymentKind::Card, "Biz Visa", Some("4242"), true)).unwrap();
    l.create_payment_method(new(PaymentKind::Cash, "Cash", None, false)).unwrap();
    let r = l.retire_payment_method(&nick("Biz Visa")).unwrap();
    assert!(r.retired && r.retired_at.is_some());
    assert!(l.default_payment_method().unwrap().is_none());
    assert_eq!(l.payment_methods(false).unwrap().len(), 1);
    let all = l.payment_methods(true).unwrap();
    assert_eq!(all.last().unwrap().nickname, "Biz Visa");
    let e = l.retire_payment_method(&PaymentMethodKey::Id(1)).unwrap_err();
    assert!(matches!(e, LedgerError::AlreadyRetired(_)), "{e}");
    let e = l.rename_payment_method(&PaymentMethodKey::Id(1), "Other name").unwrap_err();
    assert!(matches!(e, LedgerError::RetiredPaymentMethod(_)), "{e}");
    let e = l.set_default_payment_method(Some(&PaymentMethodKey::Id(1))).unwrap_err();
    assert!(matches!(e, LedgerError::RetiredPaymentMethod(_)), "{e}");
    let fresh = card(&mut l, "Biz Visa", "1111");
    let found = l.find_payment_method(&nick("biz visa")).unwrap();
    assert_eq!(found.id, fresh.id);
    assert!(!found.retired);
}

#[test]
fn s_pmt_6_at_most_one_default() {
    let (_d, mut l) = common::empty();
    l.create_payment_method(new(PaymentKind::BankAccount, "Checking", None, false)).unwrap();
    l.create_payment_method(new(PaymentKind::Cash, "Cash", None, false)).unwrap();
    l.set_default_payment_method(Some(&nick("Checking"))).unwrap();
    l.set_default_payment_method(Some(&nick("cash"))).unwrap();
    let defaults: Vec<_> = l.payment_methods(false).unwrap().into_iter().filter(|p| p.is_default).collect();
    assert_eq!(defaults.len(), 1);
    assert_eq!(defaults[0].nickname, "Cash");
    let rows = count(&l, "default_payment_method_changes");
    l.set_default_payment_method(Some(&nick("Cash"))).unwrap();
    assert_eq!(count(&l, "default_payment_method_changes"), rows);
    l.set_default_payment_method(None).unwrap();
    assert!(l.default_payment_method().unwrap().is_none());
    let rows = count(&l, "default_payment_method_changes");
    l.set_default_payment_method(None).unwrap();
    assert_eq!(count(&l, "default_payment_method_changes"), rows);
}

#[test]
fn s_pmt_7_ambiguity() {
    let (_d, mut l) = common::empty();
    card(&mut l, "Old Amex", "0005");
    l.retire_payment_method(&PaymentMethodKey::Id(1)).unwrap();
    card(&mut l, "Old Amex", "0006");
    l.retire_payment_method(&PaymentMethodKey::Id(2)).unwrap();
    match l.find_payment_method(&nick("old amex")).unwrap_err() {
        LedgerError::AmbiguousPaymentMethod { ids, .. } => assert_eq!(ids, "1, 2"),
        e => panic!("{e}"),
    }
    assert_eq!(l.find_payment_method(&PaymentMethodKey::Id(2)).unwrap().last4.as_deref(), Some("0006"));
    assert!(matches!(l.find_payment_method(&nick("nope")), Err(LedgerError::UnknownPaymentMethod(ref s)) if s == "nope"));
    assert!(matches!(l.find_payment_method(&PaymentMethodKey::Id(9)), Err(LedgerError::UnknownPaymentMethod(ref s)) if s == "id 9"));
}

// ---- Payment methods per business (D53, D55) ----

use chrono::NaiveDate;
use ledger_core::{BusinessKey, NewBusiness, NewExpense};

fn shop_key() -> BusinessKey {
    BusinessKey::Name("Test shop".into())
}

/// My business with a default "Business Visa" (id 1) and a default-less "Checking" (id 2), plus a Test shop.
fn two_businesses() -> (tempfile::TempDir, Ledger) {
    let (d, mut l) = common::empty();
    l.create_payment_method(new(PaymentKind::Card, "Business Visa", Some("4242"), true)).unwrap();
    card(&mut l, "Checking", "6789");
    l.create_business(NewBusiness { name: "Test shop".into(), note: None, is_test: true, make_current: false })
        .unwrap();
    (d, l)
}

fn expense(pm: Option<PaymentMethodKey>) -> NewExpense {
    NewExpense {
        occurred_on: Some(NaiveDate::from_ymd_opt(2026, 6, 1).unwrap()),
        amount_cents: 1999,
        description: "paper".into(),
        category: "supplies".into(),
        is_asset: false,
        payment_method: pm,
    }
}

#[test]
fn s_pmb_1_nicknames_are_per_business() {
    let (_d, mut l) = two_businesses();
    let p = l
        .create_payment_method_in(Some(&shop_key()), new(PaymentKind::Card, "Business Visa", Some("1111"), false))
        .unwrap();
    assert_eq!(p.business_id, 2);
    let listed = l.payment_methods_in(Some(&shop_key()), true).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, p.id);
    let e = l
        .create_payment_method_in(Some(&shop_key()), new(PaymentKind::Card, "business visa", Some("2222"), false))
        .unwrap_err();
    assert!(matches!(e, LedgerError::NicknameTaken(_)), "{e:?}");
}

#[test]
fn s_pmb_2_each_business_uses_its_own_default() {
    let (_d, mut l) = two_businesses();
    let shop_card = l
        .create_payment_method_in(Some(&shop_key()), new(PaymentKind::Card, "Shop card", Some("1111"), true))
        .unwrap();
    let in_shop = l.record_expense_in(Some(&shop_key()), expense(None)).unwrap();
    assert_eq!(in_shop.payment_method.unwrap().id, shop_card.id);
    let in_main = l.record_expense_in(None, expense(None)).unwrap();
    assert_eq!(in_main.payment_method.unwrap().id, 1);
}

#[test]
fn s_pmb_3_a_method_of_another_business_is_refused() {
    let (_d, mut l) = two_businesses();
    let before = count(&l, "transactions");
    let e = l.record_expense_in(Some(&shop_key()), expense(Some(PaymentMethodKey::Id(1)))).unwrap_err();
    match e {
        LedgerError::PaymentMethodInOtherBusiness { nickname, business } => {
            assert_eq!(nickname, "Business Visa");
            assert_eq!(business, "My business");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(count(&l, "transactions"), before);
    let e = l.record_expense_in(Some(&shop_key()), expense(Some(nick("Checking")))).unwrap_err();
    assert!(matches!(e, LedgerError::UnknownPaymentMethod(_)), "{e:?}");
}

#[test]
fn s_pmb_4_assigning_an_entry_of_another_business_writes_nothing() {
    let (_d, mut l) = two_businesses();
    let mine = l.record_expense_in(None, expense(Some(nick("Checking")))).unwrap();
    let theirs = l.record_expense_in(Some(&shop_key()), expense(None)).unwrap();
    let before = count(&l, "payment_method_assignments");
    let e = l.assign_payment_method_in(None, &nick("Business Visa"), &[mine.id, theirs.id]).unwrap_err();
    match e {
        LedgerError::TransactionInOtherBusiness { id, business } => {
            assert_eq!(id, theirs.id);
            assert_eq!(business, "Test shop");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(count(&l, "payment_method_assignments"), before);
}

#[test]
fn s_pmb_5_retiring_a_default_clears_only_its_business() {
    let (_d, mut l) = two_businesses();
    l.create_payment_method_in(Some(&shop_key()), new(PaymentKind::Card, "Shop card", Some("1111"), true))
        .unwrap();
    l.retire_payment_method(&nick("Business Visa")).unwrap();
    assert!(l.default_payment_method_in(None).unwrap().is_none());
    assert_eq!(l.default_payment_method_in(Some(&shop_key())).unwrap().unwrap().nickname, "Shop card");
}
