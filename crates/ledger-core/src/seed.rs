use crate::db::Ledger;
use crate::error::LedgerError;
use crate::model::{BusinessKey, NewBusiness, NewExpense, NewIncome, NewPaymentMethod, PaymentKind, PaymentMethodKey, Replacement};
use crate::ops::parse_date;

/// Records the fixed demo ledger used by tests, smokes and the seed example.
/// Produces 10 transactions, 2 voids and 1 classification change.
pub fn seed_demo(l: &mut Ledger) -> Result<(), LedgerError> {
    let exp = |d: &str, c: i64, desc: &str, cat: &str, asset: bool| -> Result<NewExpense, LedgerError> {
        Ok(NewExpense {
            occurred_on: Some(parse_date(d)?),
            amount_cents: c,
            description: desc.to_string(),
            category: cat.to_string(),
            is_asset: asset,
            payment_method: None,
        })
    };
    let inc = |d: &str, c: i64, desc: &str, src: &str| -> Result<NewIncome, LedgerError> {
        Ok(NewIncome {
            occurred_on: Some(parse_date(d)?),
            amount_cents: c,
            description: desc.to_string(),
            category: None,
            source: src.to_string(),
            payment_method: None,
        })
    };
    l.record_expense(exp("2026-10-20", 30000, "Laptop for client work", "supplies", true)?)?;
    l.record_income(inc("2026-10-15", 120000, "Website build", "Upwork")?)?;
    l.record_income(inc("2026-08-03", 8500, "Print sale", "Etsy")?)?;
    let ink = l.record_expense(exp("2026-07-12", 4210, "Printer ink and paper", "uncategorized", false)?)?;
    l.reclassify(ink.id, "office_expense", "Ink and paper are office supplies")?;
    l.record_expense(exp("2026-06-01", 1999, "Domain renewal", "advertising", false)?)?;
    let dup = l.record_expense(exp("2026-06-01", 1999, "Domain renewal", "advertising", false)?)?;
    l.void(dup.id, "Duplicate entry")?;
    let typo = l.record_expense(exp("2026-05-05", 3000, "Monitor", "supplies", false)?)?;
    l.correct(
        typo.id,
        "Typo: was $300.00",
        Replacement { amount_cents: Some(30000), ..Default::default() },
    )?;
    l.record_expense(exp("2025-12-15", 90000, "Booth rent, winter market", "rent_other", false)?)?;
    l.record_expense(exp("2026-09-30", 2500, "Flyers", "advertising", false)?)?;
    Ok(())
}

/// Adds the demo payment methods on top of `seed_demo` (run that first): Business Visa (card 4242,
/// default) on entries 1 and 10, Checking (bank account 6789) on 2 and 3, Cash on 4, and Old Amex
/// (card 0005) on 9, then retired. Entries 5 and 8 stay without one; 6 and 7 are voided.
pub fn seed_demo_payment_methods(l: &mut Ledger) -> Result<(), LedgerError> {
    let pm = |kind: PaymentKind, nickname: &str, last4: Option<&str>, make_default: bool| NewPaymentMethod {
        kind,
        nickname: nickname.to_string(),
        last4: last4.map(str::to_string),
        make_default,
    };
    l.create_payment_method(pm(PaymentKind::Card, "Business Visa", Some("4242"), true))?;
    l.create_payment_method(pm(PaymentKind::BankAccount, "Checking", Some("6789"), false))?;
    l.create_payment_method(pm(PaymentKind::Cash, "Cash", None, false))?;
    l.create_payment_method(pm(PaymentKind::Card, "Old Amex", Some("0005"), false))?;
    let key = |n: &str| PaymentMethodKey::Nickname(n.to_string());
    l.assign_payment_method(&key("Business Visa"), &[1, 10])?;
    l.assign_payment_method(&key("Checking"), &[2, 3])?;
    l.assign_payment_method(&key("Cash"), &[4])?;
    l.assign_payment_method(&key("Old Amex"), &[9])?;
    l.retire_payment_method(&key("Old Amex"))?;
    Ok(())
}

/// Adds a test business on top of `seed_demo` and `seed_demo_payment_methods` (run those first):
/// "Test shop" (id 2, a test business) with card "Test card" ending 1111 (its default, id 5),
/// expense 11 "Test sticker order" $12.00 supplies on 2026-10-01 (on Test card by default) and
/// income 12 "Test sale" $30.00 from "Test buyer" on 2026-10-02. "My business" stays current.
pub fn seed_demo_businesses(l: &mut Ledger) -> Result<(), LedgerError> {
    let shop = l.create_business(NewBusiness {
        name: "Test shop".to_string(),
        note: Some("For trying things out".to_string()),
        is_test: true,
        make_current: false,
    })?;
    let key = BusinessKey::Id(shop.id);
    l.create_payment_method_in(
        Some(&key),
        NewPaymentMethod {
            kind: PaymentKind::Card,
            nickname: "Test card".to_string(),
            last4: Some("1111".to_string()),
            make_default: true,
        },
    )?;
    l.record_expense_in(
        Some(&key),
        NewExpense {
            occurred_on: Some(parse_date("2026-10-01")?),
            amount_cents: 1200,
            description: "Test sticker order".to_string(),
            category: "supplies".to_string(),
            is_asset: false,
            payment_method: None,
        },
    )?;
    l.record_income_in(
        Some(&key),
        NewIncome {
            occurred_on: Some(parse_date("2026-10-02")?),
            amount_cents: 3000,
            description: "Test sale".to_string(),
            category: None,
            source: "Test buyer".to_string(),
            payment_method: None,
        },
    )?;
    Ok(())
}
