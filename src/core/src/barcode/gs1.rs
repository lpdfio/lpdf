//! GS1 data, written as people write it: each field's application identifier (AI) in parentheses, then its value,
//! as in `(01)09501101530003(17)261231(10)AB12`. GS1-128 and GS1 Data Matrix read it here.

/// One GS1 field: its application identifier and its value, without the parentheses.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Field<'a> {
    pub ai: &'a str,
    pub value: &'a str,
}

/// The fields of GS1 data, checked: a 2 to 4 digit AI in parentheses before each value, a value of GS1's characters,
/// the length of each field whose AI has a predefined one, and the check digit of a GTIN, an SSCC and a GLN.
pub(super) fn parse(data: &str) -> Result<Vec<Field<'_>>, String> {
    let data = data.trim();
    if !data.starts_with('(') {
        return Err("GS1: write the data as fields, each AI in parentheses: (01)09501101530003(17)261231".to_string());
    }
    let mut fields = Vec::new();
    let mut rest = data;
    while let Some(after_open) = rest.strip_prefix('(') {
        let close = after_open.find(')').ok_or_else(|| format!("GS1: the AI at '{rest}' has no closing parenthesis"))?;
        let ai = &after_open[..close];
        if !(2..=4).contains(&ai.len()) || !ai.bytes().all(|b| b.is_ascii_digit()) {
            return Err(format!("GS1: '({ai})' is not an AI: an AI is 2 to 4 digits"));
        }
        let after_ai = &after_open[close + 1..];
        let end = after_ai.find('(').unwrap_or(after_ai.len());
        let value = &after_ai[..end];
        check_field(ai, value)?;
        fields.push(Field { ai, value });
        rest = &after_ai[end..];
    }
    Ok(fields)
}

/// Whether a field of this AI has a length GS1 predefines, so that no FNC1 has to end it before the next field.
pub(super) fn has_predefined_length(ai: &str) -> bool {
    predefined_length(ai).is_some()
}

/// The length of a field of this AI, AI and value together, when GS1 predefines it: by the AI's first two digits.
fn predefined_length(ai: &str) -> Option<usize> {
    let prefix: u8 = ai.get(..2)?.parse().ok()?;
    Some(match prefix {
        0 => 20,
        1..=3 => 16,
        4 => 18,
        11..=19 => 8,
        20 => 4,
        31..=36 => 10,
        41 => 16,
        _ => return None,
    })
}

/// Check one field: a value of GS1's characters, the length a predefined field has, all digits there, and the check
/// digit of a GTIN (01, 02), an SSCC (00) and a GLN (410 to 417).
fn check_field(ai: &str, value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err(format!("GS1: ({ai}) has no value"));
    }
    if let Some(bad) = value.chars().find(|&c| !is_gs1_character(c)) {
        return Err(format!("GS1: ({ai}) has '{bad}', which is not one of GS1's characters"));
    }
    let Some(length) = predefined_length(ai) else { return Ok(()) };
    if ai.len() + value.len() != length || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("GS1: ({ai}) takes {} digits, and has '{value}'", length - ai.len()));
    }
    let checked = matches!(ai, "00" | "01" | "02") || (ai.len() == 3 && ai.starts_with("41") && ai.as_bytes()[2] <= b'7');
    if checked {
        let digits: Vec<u8> = value.bytes().map(|b| b - b'0').collect();
        let (body, last) = digits.split_at(digits.len() - 1);
        let expected = super::gs1_check_digit(body);
        if last[0] != expected {
            return Err(format!("GS1: ({ai}){value} has the check digit {}, and {expected} is right", last[0]));
        }
    }
    Ok(())
}

/// GS1's character set 82, less the parentheses that mark the AIs here.
fn is_gs1_character(c: char) -> bool {
    c.is_ascii_alphanumeric() || "!\"%&'*+,-./:;<=>?_".contains(c)
}
