//! Reads the objects of a file the PDF writer wrote, or the encryption pass rewrote, for the linearization pass: the
//! value of each object, with its references, and where the parts that change when the object is copied under a new
//! number are written: the number of each reference, each string and the data of a stream.
//!
//! It reads what those two write, as `rewrite.rs` does: direct `/Length` values and no object streams. It does not
//! read a file written by anything else.

use std::ops::Range;

use super::encrypt::{read_u32, skip_ws};
use super::rewrite::{is_delimiter, is_whitespace, read_hex_string, read_literal_string, regular_run_end};

/// How deep arrays and dictionaries may nest in a value: far deeper than the writer goes, and a guard against a value
/// that would run the reader out of stack.
const MAX_DEPTH: usize = 64;

/// A value, with what the linearization pass needs of it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    /// A dictionary: its keys, without the `/`, and their values, in the order they are written.
    Dict(Vec<(Vec<u8>, Value)>),
    Array(Vec<Value>),
    /// A reference to the object with this number.
    Ref(u32),
    /// A name, without the `/`.
    Name(Vec<u8>),
    /// A whole number that is not part of a reference, as a `/Length` is.
    Integer(u64),
    /// A string, `(…)` or `<…>`: where it is written, its delimiters included.
    String(Range<usize>),
    /// Another number, a boolean or null.
    Other,
}

impl Value {
    /// The value of `key` in a dictionary.
    pub(crate) fn get(&self, key: &[u8]) -> Option<&Value> {
        match self {
            Value::Dict(entries) => entries.iter().find(|(name, _)| name == key).map(|(_, value)| value),
            _ => None,
        }
    }

    /// The object a dictionary's `key` refers to.
    pub(crate) fn reference(&self, key: &[u8]) -> Option<u32> {
        match self.get(key) {
            Some(Value::Ref(number)) => Some(*number),
            _ => None,
        }
    }

    /// Whether this is a dictionary whose `/Type` is the name `type_name`.
    pub(crate) fn is_type(&self, type_name: &[u8]) -> bool {
        matches!(self.get(b"Type"), Some(Value::Name(name)) if name == type_name)
    }
}

/// A part of an object that changes when the object is copied under new numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Part {
    /// The number of a reference to this object.
    Reference(u32),
    /// A string, `(…)` or `<…>`.
    String,
    /// The data of a stream.
    StreamData,
}

/// An object of the file.
#[derive(Debug)]
pub(crate) struct Object {
    pub(crate) generation: u32,
    /// The value: for a stream, its dictionary.
    pub(crate) value:      Value,
    /// What follows `N G obj`, up to and with its `endobj`.
    pub(crate) body:       Range<usize>,
    /// Where the parts that change are written, in the order they are.
    pub(crate) parts:      Vec<(Range<usize>, Part)>,
}

/// Reads the object at `offset`, whose `N G obj` has to be there.
pub(crate) fn read_object(src: &[u8], offset: usize) -> Result<Object, String> {
    let header = src.get(offset..).ok_or_else(|| format!("no object at offset {offset}, past the end of the file"))?;
    let (_, rest) = read_u32(skip_ws(header)).ok_or_else(|| format!("no object number at offset {offset}"))?;
    let (generation, rest) = read_u32(skip_ws(rest)).ok_or_else(|| format!("no generation at offset {offset}"))?;
    let rest = skip_ws(rest).strip_prefix(b"obj").ok_or_else(|| format!("no `obj` keyword at offset {offset}"))?;
    let start = src.len() - rest.len();

    let mut reader = Reader { src, pos: start, parts: Vec::new() };
    let value = reader.value(0)?;
    reader.skip_space();
    if reader.at_keyword(b"stream") {
        reader.stream_data(&value)?;
        reader.skip_space();
        reader.expect_keyword(b"endstream")?;
        reader.skip_space();
    }
    reader.expect_keyword(b"endobj")?;
    Ok(Object { generation, value, body: start..reader.pos, parts: reader.parts })
}

/// Reads the value at `pos`, such as the dictionary after a `trailer` keyword.
pub(crate) fn read_value(src: &[u8], pos: usize) -> Result<Value, String> {
    Reader { src, pos, parts: Vec::new() }.value(0)
}

struct Reader<'a> {
    src:   &'a [u8],
    pos:   usize,
    parts: Vec<(Range<usize>, Part)>,
}

impl Reader<'_> {
    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    /// Skips white space and comments.
    fn skip_space(&mut self) {
        while let Some(byte) = self.peek() {
            if is_whitespace(byte) {
                self.pos += 1;
            } else if byte == b'%' {
                self.pos += self.src[self.pos..].iter().take_while(|&&b| b != b'\n' && b != b'\r').count();
            } else {
                break;
            }
        }
    }

    fn at_keyword(&self, keyword: &[u8]) -> bool {
        regular_run_end(self.src, self.pos) == self.pos + keyword.len() && self.src[self.pos..].starts_with(keyword)
    }

    fn expect_keyword(&mut self, keyword: &[u8]) -> Result<(), String> {
        if !self.at_keyword(keyword) {
            return Err(format!("expected `{}` at offset {}", String::from_utf8_lossy(keyword), self.pos));
        }
        self.pos += keyword.len();
        Ok(())
    }

    fn value(&mut self, depth: usize) -> Result<Value, String> {
        if depth > MAX_DEPTH {
            return Err(format!("values nest more than {MAX_DEPTH} deep at offset {}", self.pos));
        }
        self.skip_space();
        let start = self.pos;
        match self.peek() {
            None => Err("the file ends inside a value".to_string()),
            Some(b'<') if self.src.get(start + 1) == Some(&b'<') => self.dict(depth),
            Some(b'<') => {
                let (_, end) = read_hex_string(self.src, start).ok_or_else(|| format!("unterminated hex string at offset {start}"))?;
                self.string(start..end)
            }
            Some(b'(') => {
                let (_, end) = read_literal_string(self.src, start).ok_or_else(|| format!("unterminated string at offset {start}"))?;
                self.string(start..end)
            }
            Some(b'[') => {
                self.pos += 1;
                let mut items = Vec::new();
                loop {
                    self.skip_space();
                    if self.peek() == Some(b']') {
                        self.pos += 1;
                        return Ok(Value::Array(items));
                    }
                    items.push(self.value(depth + 1)?);
                }
            }
            Some(b'/') => {
                let end = regular_run_end(self.src, start + 1);
                self.pos = end;
                Ok(Value::Name(self.src[start + 1..end].to_vec()))
            }
            Some(_) => self.token(),
        }
    }

    fn string(&mut self, span: Range<usize>) -> Result<Value, String> {
        self.pos = span.end;
        self.parts.push((span.clone(), Part::String));
        Ok(Value::String(span))
    }

    fn dict(&mut self, depth: usize) -> Result<Value, String> {
        self.pos += 2;
        let mut entries = Vec::new();
        loop {
            self.skip_space();
            if self.src[self.pos..].starts_with(b">>") {
                self.pos += 2;
                return Ok(Value::Dict(entries));
            }
            let Value::Name(key) = self.value(depth + 1)? else {
                return Err(format!("a dictionary key is not a name, before offset {}", self.pos));
            };
            let value = self.value(depth + 1)?;
            entries.push((key, value));
        }
    }

    /// A number, a reference (`N G R`), or a keyword such as `true` or `null`.
    fn token(&mut self) -> Result<Value, String> {
        let start = self.pos;
        let end = regular_run_end(self.src, start);
        if end == start {
            return Err(format!("unexpected `{}` at offset {start}", char::from(self.src[start])));
        }
        self.pos = end;
        let token = &self.src[start..end];
        if !token.iter().all(u8::is_ascii_digit) {
            return Ok(Value::Other);
        }
        if let Some((number, after)) = self.reference_after(start, end) {
            self.parts.push((start..end, Part::Reference(number)));
            self.pos = after;
            return Ok(Value::Ref(number));
        }
        let value = std::str::from_utf8(token).ok().and_then(|text| text.parse().ok())
            .ok_or_else(|| format!("number too large at offset {start}"))?;
        Ok(Value::Integer(value))
    }

    /// When the whole number at `start..end` is the object number of a reference, `N G R`: the number, and where the
    /// reference ends.
    fn reference_after(&self, start: usize, end: usize) -> Option<(u32, usize)> {
        let number: u32 = std::str::from_utf8(&self.src[start..end]).ok()?.parse().ok()?;
        let rest = skip_ws(&self.src[end..]);
        let (_, rest) = read_u32(rest)?;
        let rest = skip_ws(rest);
        let after_r = rest.strip_prefix(b"R")?;
        if after_r.first().is_some_and(|&b| !is_whitespace(b) && !is_delimiter(b)) {
            return None;
        }
        Some((number, self.src.len() - after_r.len()))
    }

    /// Reads past the data of the stream whose dictionary is `dict`, which the `stream` keyword at `pos` starts.
    fn stream_data(&mut self, dict: &Value) -> Result<(), String> {
        let after_keyword = self.pos + b"stream".len();
        let data_start = match (self.src.get(after_keyword), self.src.get(after_keyword + 1)) {
            (Some(b'\r'), Some(b'\n')) => after_keyword + 2,
            (Some(b'\n'), _) => after_keyword + 1,
            _ => return Err(format!("`stream` is not followed by an end of line at offset {}", self.pos)),
        };
        let Some(&Value::Integer(length)) = dict.get(b"Length") else {
            return Err(format!("the stream at offset {} has no direct /Length", self.pos));
        };
        let data_end = usize::try_from(length).ok().and_then(|length| data_start.checked_add(length))
            .filter(|&end| end <= self.src.len())
            .ok_or_else(|| format!("the stream at offset {} runs past the end of the file", self.pos))?;
        self.parts.push((data_start..data_end, Part::StreamData));
        self.pos = data_end;
        Ok(())
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn object(text: &str) -> Object {
        read_object(text.as_bytes(), 0).unwrap()
    }

    #[test]
    fn a_dictionary_keeps_its_keys_and_values_in_order() {
        let read = object("3 0 obj\n<< /Type /Page /Parent 2 0 R /Rotate 90 >>\nendobj\n");
        assert_eq!(read.value, Value::Dict(vec![
            (b"Type".to_vec(), Value::Name(b"Page".to_vec())),
            (b"Parent".to_vec(), Value::Ref(2)),
            (b"Rotate".to_vec(), Value::Integer(90)),
        ]));
        assert!(read.value.is_type(b"Page"));
        assert_eq!(read.value.reference(b"Parent"), Some(2));
    }

    #[test]
    fn each_reference_is_a_part_at_its_number() {
        let text = "1 0 obj\n[12 0 R 7 0 R 5]\nendobj\n";
        let read = object(text);
        let references: Vec<_> = read.parts.iter().map(|(range, part)| (&text[range.clone()], *part)).collect();
        assert_eq!(references, [("12", Part::Reference(12)), ("7", Part::Reference(7))]);
        assert_eq!(read.value, Value::Array(vec![Value::Ref(12), Value::Ref(7), Value::Integer(5)]));
    }

    #[test]
    fn two_numbers_without_an_r_are_not_a_reference() {
        let read = object("1 0 obj\n[0 0 595 842]\nendobj\n");
        assert!(read.parts.is_empty());
    }

    #[test]
    fn an_r_that_starts_a_longer_word_is_not_a_reference() {
        let read = object("1 0 obj\n[1 0 Rx]\nendobj\n");
        assert!(read.parts.is_empty());
    }

    #[test]
    fn strings_are_parts_with_their_delimiters() {
        let text = "1 0 obj\n<< /T (a\\)b) /U <4142> >>\nendobj\n";
        let read = object(text);
        let strings: Vec<_> = read.parts.iter().map(|(range, _)| &text[range.clone()]).collect();
        assert_eq!(strings, ["(a\\)b)", "<4142>"]);
    }

    #[test]
    fn a_stream_has_its_data_as_a_part() {
        let text = "4 0 obj\n<< /Length 5 >>\nstream\nab>>c\nendstream\nendobj\n";
        let read = object(text);
        assert_eq!(read.parts.len(), 1);
        let (range, part) = &read.parts[0];
        assert_eq!((&text[range.clone()], *part), ("ab>>c", Part::StreamData));
        assert!(text[..read.body.end].ends_with("endobj"));
    }

    #[test]
    fn a_stream_without_a_direct_length_is_refused() {
        let error = read_object(b"4 0 obj\n<< /Length 9 0 R >>\nstream\nab\nendstream\nendobj\n", 0).unwrap_err();
        assert!(error.contains("direct /Length"), "{error}");
    }

    #[test]
    fn an_object_without_endobj_is_refused() {
        let error = read_object(b"4 0 obj\n<< >>\n", 0).unwrap_err();
        assert!(error.contains("endobj"), "{error}");
    }
}
