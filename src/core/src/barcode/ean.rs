//! EAN-13, EAN-8 and UPC-A: digits, the last a check digit that is added when left out and checked when given.
//! A UPC-A is the EAN-13 of its digits with a 0 in front, so the two draw the same bars.

/// Which digits of the left half of an EAN-13 take the G code, by its first digit: bit 5 is the second digit.
static EAN13_PARITY: [u8; 10] = [
    0b000000, 0b001011, 0b001101, 0b001110, 0b010011, 0b011001, 0b011100, 0b010101, 0b010110, 0b011010,
];

/// The L code of each digit: seven modules, most significant bit first, 1 a bar. The R code is its complement, and
/// the G code the R code reversed.
static L_CODES: [u8; 10] = [
    0b0001101, 0b0011001, 0b0010011, 0b0111101, 0b0100011, 0b0110001, 0b0101111, 0b0111011, 0b0110111, 0b0001011,
];

const GUARD: [bool; 3] = [true, false, true];
const CENTER: [bool; 5] = [false, true, false, true, false];

/// Encode an EAN-13 of 12 digits, its check digit added, or 13, its last checked. Returns the bars and the 13 digits.
pub(super) fn encode_ean13(data: &str) -> Result<(Vec<u8>, String), String> {
    let digits = digits_with_check("EAN-13", data, 12)?;
    let mut modules = Vec::with_capacity(95);
    modules.extend_from_slice(&GUARD);
    let parity = EAN13_PARITY[digits[0] as usize];
    for (i, &d) in digits[1..7].iter().enumerate() {
        let left = l_code(d);
        if (parity >> (5 - i)) & 1 == 1 {
            modules.extend(left.iter().rev().map(|m| !m));
        } else {
            modules.extend_from_slice(&left);
        }
    }
    modules.extend_from_slice(&CENTER);
    for &d in &digits[7..13] {
        modules.extend(l_code(d).iter().map(|m| !m));
    }
    modules.extend_from_slice(&GUARD);
    Ok((super::run_lengths(&modules), text_of(&digits)))
}

/// Encode an EAN-8 of 7 digits, its check digit added, or 8, its last checked. Returns the bars and the 8 digits.
pub(super) fn encode_ean8(data: &str) -> Result<(Vec<u8>, String), String> {
    let digits = digits_with_check("EAN-8", data, 7)?;
    let mut modules = Vec::with_capacity(67);
    modules.extend_from_slice(&GUARD);
    for &d in &digits[..4] {
        modules.extend_from_slice(&l_code(d));
    }
    modules.extend_from_slice(&CENTER);
    for &d in &digits[4..] {
        modules.extend(l_code(d).iter().map(|m| !m));
    }
    modules.extend_from_slice(&GUARD);
    Ok((super::run_lengths(&modules), text_of(&digits)))
}

/// Encode a UPC-A of 11 digits, its check digit added, or 12, its last checked. Returns the bars and the 12 digits.
pub(super) fn encode_upca(data: &str) -> Result<(Vec<u8>, String), String> {
    let digits = digits_with_check("UPC-A", data, 11)?;
    let (bars, _) = encode_ean13(&format!("0{}", text_of(&digits)))?;
    Ok((bars, text_of(&digits)))
}

/// The digits of `data` with the check digit: `body` digits get it added, and `body + 1` have the last checked.
fn digits_with_check(name: &str, data: &str, body: usize) -> Result<Vec<u8>, String> {
    let data = data.trim();
    let count = data.chars().count();
    if count != body && count != body + 1 {
        return Err(format!("{name}: data must be {body} or {} digits, got {count}", body + 1));
    }
    if let Some(i) = data.chars().position(|c| !c.is_ascii_digit()) {
        return Err(format!("{name}: non-digit at position {i}"));
    }
    let mut digits: Vec<u8> = data.bytes().map(|b| b - b'0').collect();
    let check = super::gs1_check_digit(&digits[..body]);
    if digits.len() == body {
        digits.push(check);
    } else if digits[body] != check {
        return Err(format!("{name}: invalid check digit (got {}, expected {check})", digits[body]));
    }
    Ok(digits)
}

fn l_code(digit: u8) -> [bool; 7] {
    let pattern = L_CODES[digit as usize];
    std::array::from_fn(|i| (pattern >> (6 - i)) & 1 == 1)
}

fn text_of(digits: &[u8]) -> String {
    digits.iter().map(|d| char::from(b'0' + d)).collect()
}
