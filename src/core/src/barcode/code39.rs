//! Code 39: the 43 characters of its standard, ISO/IEC 16388, between start and stop characters `*`, with an optional
//! mod 43 check character. Full ASCII is left out: its pairs come back from most scanners as written.

/// The 43 characters, in the order of their values for the check character.
const CHARACTERS: &str = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ-. $/+%";

/// The pattern of each character: nine elements, bar first, a 1 bit (from bit 8) a wide one.
static PATTERNS: [u16; 43] = [
    0x034, 0x121, 0x061, 0x160, 0x031, 0x130, 0x070, 0x025, 0x124, 0x064, // 0-9
    0x109, 0x049, 0x148, 0x019, 0x118, 0x058, 0x00D, 0x10C, 0x04C, 0x01C, // A-J
    0x103, 0x043, 0x142, 0x013, 0x112, 0x052, 0x007, 0x106, 0x046, 0x016, // K-T
    0x181, 0x0C1, 0x1C0, 0x091, 0x190, 0x0D0,                             // U-Z
    0x085, 0x184, 0x0C4, 0x0A8, 0x0A2, 0x08A, 0x02A,                      // - . space $ / + %
];

/// The start and stop character, `*`.
const START_STOP: u16 = 0x094;

/// How many modules a wide element takes, a narrow one taking one: the standard allows 2 to 3.
const WIDE: u8 = 3;

/// Encode `data`, with the mod 43 check character when `check` is set.
pub(super) fn encode(data: &str, check: bool) -> Result<Vec<u8>, String> {
    if data.is_empty() {
        return Err("Code 39: no data to encode".to_string());
    }
    let mut values = Vec::with_capacity(data.len() + 1);
    for c in data.chars() {
        let value = CHARACTERS.find(c).ok_or_else(|| {
            format!(
                "Code 39: '{c}' is not one of its characters (0-9, A-Z, space and - . $ / + %); use code128 for other text"
            )
        })?;
        values.push(value);
    }
    if check {
        values.push(values.iter().sum::<usize>() % 43);
    }
    let mut bars = Vec::with_capacity((values.len() + 2) * 10);
    push_character(&mut bars, START_STOP);
    for value in values {
        bars.push(1);
        push_character(&mut bars, PATTERNS[value]);
    }
    bars.push(1);
    push_character(&mut bars, START_STOP);
    Ok(bars)
}

/// Add the nine elements of a character's pattern.
fn push_character(bars: &mut Vec<u8>, pattern: u16) {
    bars.extend((0..9).rev().map(|bit| if (pattern >> bit) & 1 == 1 { WIDE } else { 1 }));
}
