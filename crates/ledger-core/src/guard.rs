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

/// The zero of every block of decimal digits (Unicode general category Nd, Unicode 16.0).
/// Each block is that zero followed by the digits one to nine. Sorted ascending.
/// Generated with Python 3.14 `unicodedata` (category "Nd", decimal value 0).
const DECIMAL_DIGIT_ZEROS: [u32; 76] = [
    0x0030, 0x0660, 0x06F0, 0x07C0, 0x0966, 0x09E6, 0x0A66, 0x0AE6, 0x0B66, 0x0BE6,
    0x0C66, 0x0CE6, 0x0D66, 0x0DE6, 0x0E50, 0x0ED0, 0x0F20, 0x1040, 0x1090, 0x17E0,
    0x1810, 0x1946, 0x19D0, 0x1A80, 0x1A90, 0x1B50, 0x1BB0, 0x1C40, 0x1C50, 0xA620,
    0xA8D0, 0xA900, 0xA9D0, 0xA9F0, 0xAA50, 0xABF0, 0xFF10, 0x104A0, 0x10D30, 0x10D40,
    0x11066, 0x110F0, 0x11136, 0x111D0, 0x112F0, 0x11450, 0x114D0, 0x11650, 0x116C0, 0x116D0,
    0x116DA, 0x11730, 0x118E0, 0x11950, 0x11BF0, 0x11C50, 0x11D50, 0x11DA0, 0x11F50, 0x16130,
    0x16A60, 0x16AC0, 0x16B50, 0x16D70, 0x1CCF0, 0x1D7CE, 0x1D7D8, 0x1D7E2, 0x1D7EC, 0x1D7F6,
    0x1E140, 0x1E2F0, 0x1E4F0, 0x1E5F1, 0x1E950, 0x1FBF0,
];

/// The value of `c` when it is a decimal digit in any script: ASCII, fullwidth, Arabic-Indic,
/// Devanagari and the rest of Unicode category Nd. Superscripts, fractions, circled and Roman
/// numerals are not decimal digits and return `None`.
fn decimal_digit(c: char) -> Option<u8> {
    let cp = u32::from(c);
    let i = DECIMAL_DIGIT_ZEROS.partition_point(|&z| z <= cp);
    let zero = *DECIMAL_DIGIT_ZEROS.get(i.checked_sub(1)?)?;
    let d = cp - zero;
    if d < 10 {
        Some(d as u8)
    } else {
        None
    }
}

/// Runs of decimal digits in any script, each folded to ASCII, joined across one space or dash
/// when the groups on both sides have at least 3 digits (4242 4242 4242 4242, 3782-822463-10005,
/// and the same in fullwidth or Arabic-Indic digits). Dates (2026-10-28) and phone numbers
/// (+1 512 555 0100) stay short, separate chains.
fn digit_chains(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut runs: Vec<(usize, usize, String)> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if decimal_digit(chars[i]).is_some() {
            let s = i;
            let mut folded = String::new();
            while let Some(d) = chars.get(i).copied().and_then(decimal_digit) {
                folded.push(char::from(b'0' + d));
                i += 1;
            }
            runs.push((s, i, folded));
        } else {
            i += 1;
        }
    }
    let mut chains: Vec<String> = Vec::new();
    let mut prev: Option<(usize, usize)> = None;
    for (s, e, folded) in runs {
        let joins = match prev {
            Some((ps, pe)) => s == pe + 1 && matches!(chars[pe], ' ' | '-') && pe - ps >= 3 && e - s >= 3,
            None => false,
        };
        match chains.last_mut() {
            Some(last) if joins => last.push_str(&folded),
            _ => chains.push(folded),
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

#[cfg(test)]
mod tests {
    use super::*;

    // S-PAN-13: the table is sorted, blocks do not overlap, and every listed char is numeric
    // to the toolchain's own Unicode tables and folds to its digit.
    #[test]
    fn decimal_digit_table_is_sorted_and_every_block_is_ten_numeric_chars() {
        assert!(DECIMAL_DIGIT_ZEROS.windows(2).all(|w| w[0] + 10 <= w[1]));
        for zero in DECIMAL_DIGIT_ZEROS {
            for d in 0..10u32 {
                let c = char::from_u32(zero + d).expect("a valid char");
                assert!(c.is_numeric(), "U+{:04X} is not numeric", zero + d);
                assert_eq!(decimal_digit(c), Some(d as u8));
            }
        }
    }

    #[test]
    fn other_numerals_and_separators_are_not_digits() {
        for c in ['a', ' ', '-', '/', ':', '\u{00B2}', '\u{00BD}', '\u{2160}', '\u{2460}', '\u{FF0D}', '\u{3000}'] {
            assert_eq!(decimal_digit(c), None, "{c:?}");
        }
    }
}
