//! D30: the ledger never stores a full card or account number, only ever the last 4 digits.
use crate::error::LedgerError;

/// Shortest digit chain treated as a full card or account number. Shorter numbers
/// (amounts, dates, phone numbers, invoice numbers) pass.
pub const MIN_FULL_NUMBER_DIGITS: usize = 13;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FullNumber {
    Card,
    Account,
}

/// Runs of ASCII digits, joined across one space or dash when the groups on both sides have
/// at least 3 digits (4242 4242 4242 4242, 3782-822463-10005). Dates (2026-10-28) and
/// phone numbers (+1 512 555 0100) stay short, separate chains.
fn digit_chains(text: &str) -> Vec<String> {
    let b = text.as_bytes();
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() {
            let s = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            runs.push((s, i));
        } else {
            i += 1;
        }
    }
    let mut chains: Vec<String> = Vec::new();
    let mut prev: Option<(usize, usize)> = None;
    for (s, e) in runs {
        let joins = match prev {
            Some((ps, pe)) => s == pe + 1 && matches!(b[pe], b' ' | b'-') && pe - ps >= 3 && e - s >= 3,
            None => false,
        };
        match chains.last_mut() {
            Some(last) if joins => last.push_str(&text[s..e]),
            _ => chains.push(text[s..e].to_string()),
        }
        prev = Some((s, e));
    }
    chains
}

fn luhn(digits: &str) -> bool {
    let mut sum = 0u32;
    for (i, c) in digits.bytes().rev().enumerate() {
        let mut d = u32::from(c - b'0');
        if i % 2 == 1 {
            d *= 2;
            if d > 9 {
                d -= 9;
            }
        }
        sum += d;
    }
    sum % 10 == 0
}

pub(crate) fn find_full_number(text: &str) -> Option<FullNumber> {
    let mut found = None;
    for c in digit_chains(text) {
        if c.len() >= MIN_FULL_NUMBER_DIGITS {
            if c.len() <= 19 && luhn(&c) {
                return Some(FullNumber::Card);
            }
            found = Some(FullNumber::Account);
        }
    }
    found
}

/// Refuse `text` when it holds a full card number or a long account or routing-like number.
/// `field` names the input in the message ("description", "description of entry 3").
/// The message never contains the digits.
pub fn check_no_full_number(field: &str, text: &str) -> Result<(), LedgerError> {
    match find_full_number(text) {
        Some(FullNumber::Card) => Err(LedgerError::CardNumberInText { field: field.to_string() }),
        Some(FullNumber::Account) => Err(LedgerError::AccountNumberInText { field: field.to_string() }),
        None => Ok(()),
    }
}
