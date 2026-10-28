use crate::db::Ledger;
use crate::error::LedgerError;
use crate::model::{NewExpense, NewIncome, Replacement};
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
