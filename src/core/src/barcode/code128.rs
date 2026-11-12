//! Code 128, and GS1-128, the Code 128 of GS1 fields.
//!
//! A plain Code 128 uses code set B (printable ASCII 32–126) alone, one character to a symbol. A GS1-128 starts with
//! FNC1 and switches to code set C for runs of digits, which packs two digits into a symbol, by the rules of ISO/IEC
//! 15417 Annex E, as GS1 data is mostly digits.

use super::gs1::{self, Field};

/// The symbols, indexed by value 0–105: each six elements, bar first, eleven modules in all.
static SYMBOLS: [[u8; 6]; 106] = [
    [2,1,2,2,2,2], [2,2,2,1,2,2], [2,2,2,2,2,1], [1,2,1,2,2,3],
    [1,2,1,3,2,2], [1,3,1,2,2,2], [1,2,2,2,1,3], [1,2,2,3,1,2],
    [1,3,2,2,1,2], [2,2,1,2,1,3], [2,2,1,3,1,2], [2,3,1,2,1,2],
    [1,1,2,2,3,2], [1,2,2,1,3,2], [1,2,2,2,3,1], [1,1,3,2,2,2],
    [1,2,3,1,2,2], [1,2,3,2,2,1], [2,2,3,2,1,1], [2,2,1,1,3,2],
    [2,2,1,2,3,1], [2,1,3,2,1,2], [2,2,3,1,1,2], [3,1,2,1,3,1],
    [3,1,1,2,2,2], [3,2,1,1,2,2], [3,2,1,2,2,1], [3,1,2,2,1,2],
    [3,2,2,1,1,2], [3,2,2,2,1,1], [2,1,2,1,2,3], [2,1,2,3,2,1],
    [2,3,2,1,2,1], [1,1,1,3,2,3], [1,3,1,1,2,3], [1,3,1,3,2,1],
    [1,1,2,3,1,3], [1,3,2,1,1,3], [1,3,2,3,1,1], [2,1,1,3,1,3],
    [2,3,1,1,1,3], [2,3,1,3,1,1], [1,1,2,1,3,3], [1,1,2,3,3,1],
    [1,3,2,1,3,1], [1,1,3,1,2,3], [1,1,3,3,2,1], [1,3,3,1,2,1],
    [3,1,3,1,2,1], [2,1,1,3,3,1], [2,3,1,1,3,1], [2,1,3,1,1,3],
    [2,1,3,3,1,1], [2,1,3,1,3,1], [3,1,1,1,2,3], [3,1,1,3,2,1],
    [3,3,1,1,2,1], [3,1,2,1,1,3], [3,1,2,3,1,1], [3,3,2,1,1,1],
    [3,1,4,1,1,1], [2,2,4,2,1,1], [4,3,1,1,1,1], [1,1,1,2,2,4],
    [1,1,1,4,2,2], [1,2,1,1,2,4], [1,2,1,4,2,1], [1,4,1,1,2,2],
    [1,4,1,2,2,1], [1,1,2,2,1,4], [1,1,2,4,1,2], [1,2,2,1,1,4],
    [1,2,2,4,1,1], [1,4,2,1,1,2], [1,4,2,2,1,1], [2,4,1,2,1,1],
    [2,2,1,1,1,4], [4,1,3,1,1,1], [2,4,1,1,1,2], [1,3,4,1,1,1],
    [1,1,1,2,4,2], [1,2,1,1,4,2], [1,2,1,2,4,1], [1,1,4,2,1,2],
    [1,2,4,1,1,2], [1,2,4,2,1,1], [4,1,1,2,1,2], [4,2,1,1,1,2],
    [4,2,1,2,1,1], [2,1,2,1,4,1], [2,1,4,1,2,1], [4,1,2,1,2,1],
    [1,1,1,1,4,3], [1,1,1,3,4,1], [1,3,1,1,4,1], [1,1,4,1,1,3],
    [1,1,4,3,1,1], [4,1,1,1,1,3], [4,1,1,3,1,1], [1,1,3,1,4,1],
    [1,1,4,1,3,1], [3,1,1,1,4,1], [4,1,1,1,3,1],                 // 100-102
    [2,1,1,4,1,2], [2,1,1,2,1,4], [2,1,1,2,3,2],                 // 103=START A, 104=START B, 105=START C
];

/// The stop symbol: seven elements, thirteen modules.
static STOP: [u8; 7] = [2,3,3,1,1,1,2];

const CODE_C: u8 = 99;
const CODE_B: u8 = 100;
const FNC1: u8 = 102;
const START_B: u8 = 104;
const START_C: u8 = 105;

/// The most characters of data, application identifiers included, a GS1-128 may hold.
const GS1_MAX_CHARACTERS: usize = 48;

/// Encode `data` in code set B, one character to a symbol.
pub(super) fn encode(data: &str) -> Result<Vec<u8>, String> {
    let mut values = vec![START_B];
    for c in data.chars() {
        if !(32..=126).contains(&(c as u32)) {
            return Err(format!(
                "Code 128: character '{}' (U+{:04X}) is not in subset B (ASCII 32\u{2013}126)",
                c, c as u32
            ));
        }
        values.push(c as u8 - 32);
    }
    Ok(bars(&values))
}

/// What a GS1-128 encodes: the characters of its fields, and FNC1 where one ends.
#[derive(Clone, Copy, PartialEq)]
enum Item {
    Char(u8),
    Fnc1,
}

/// Encode GS1 fields as a GS1-128: FNC1 first, then the fields, with FNC1 after each field of varying length that
/// isn't the last, in code set C for runs of digits and B for the rest.
pub(super) fn encode_gs1(fields: &[Field]) -> Result<Vec<u8>, String> {
    let characters: usize = fields.iter().map(|f| f.ai.len() + f.value.len()).sum();
    if characters > GS1_MAX_CHARACTERS {
        return Err(format!(
            "GS1-128: the fields have {characters} characters, and a GS1-128 holds {GS1_MAX_CHARACTERS} at most"
        ));
    }
    Ok(bars(&values_of(&items_of(fields))))
}

/// FNC1, then the characters of the fields, with FNC1 after each field of varying length that isn't the last.
fn items_of(fields: &[Field]) -> Vec<Item> {
    let mut items = vec![Item::Fnc1];
    for (i, field) in fields.iter().enumerate() {
        items.extend(field.ai.bytes().chain(field.value.bytes()).map(Item::Char));
        if i + 1 < fields.len() && !gs1::has_predefined_length(field.ai) {
            items.push(Item::Fnc1);
        }
    }
    items
}

/// The number of digits in a row from `at`.
fn digits_from(items: &[Item], at: usize) -> usize {
    items[at..].iter().take_while(|item| matches!(item, Item::Char(c) if c.is_ascii_digit())).count()
}

/// The symbol values of `items`, start symbol first, in code sets B and C. A run of four or more digits goes in C,
/// two to a symbol; an odd one has its first digit in B.
fn values_of(items: &[Item]) -> Vec<u8> {
    let first_char = items.iter().position(|item| *item != Item::Fnc1).unwrap_or(items.len());
    let run = digits_from(items, first_char);
    let mut in_c = run >= 4 || (run >= 2 && run % 2 == 0 && first_char + run == items.len());
    let mut values = vec![if in_c { START_C } else { START_B }];
    let mut at = 0;
    while at < items.len() {
        match items[at] {
            Item::Fnc1 => {
                values.push(FNC1);
                at += 1;
            }
            Item::Char(_) if in_c => {
                if digits_from(items, at) >= 2 {
                    let Item::Char(tens) = items[at] else { unreachable!("a digit") };
                    let Item::Char(ones) = items[at + 1] else { unreachable!("a digit") };
                    values.push((tens - b'0') * 10 + (ones - b'0'));
                    at += 2;
                } else {
                    values.push(CODE_B);
                    in_c = false;
                }
            }
            Item::Char(c) => {
                let run = digits_from(items, at);
                if run >= 4 && run % 2 == 0 {
                    values.push(CODE_C);
                    in_c = true;
                } else {
                    values.push(c - 32);
                    at += 1;
                }
            }
        }
    }
    values
}

/// The bars of the symbols `values`, start symbol first, with the check symbol and the stop added.
fn bars(values: &[u8]) -> Vec<u8> {
    let check = values
        .iter()
        .enumerate()
        .map(|(i, &v)| i.max(1) as u32 * v as u32)
        .sum::<u32>()
        % 103;
    let mut bars = Vec::with_capacity((values.len() + 1) * 6 + 7);
    for &value in values {
        bars.extend_from_slice(&SYMBOLS[value as usize]);
    }
    bars.extend_from_slice(&SYMBOLS[check as usize]);
    bars.extend_from_slice(&STOP);
    bars
}

/// The symbol values of a GS1-128, without its check symbol, for the tests.
#[cfg(test)]
pub(super) fn gs1_values(fields: &[Field]) -> Vec<u8> {
    values_of(&items_of(fields))
}

/// The value of FNC1 in code sets B and C.
#[cfg(test)]
pub(super) const FNC1_VALUE: u8 = FNC1;
