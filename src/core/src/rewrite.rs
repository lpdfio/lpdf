//! Copies a pdf-writer file object by object, passing every string and stream through a
//! transform that may change its length.
//!
//! RC4 keeps lengths, so `encrypt.rs` encrypts in place. AES-CBC does not (an IV and
//! padding on each string and stream), so offsets, `/Length` values and the xref have to
//! be rebuilt. This pass does the first two and returns each object's new offset for
//! the xref writer.
//!
//! It reads what pdf-writer writes: a classic xref table, direct `/Length` values and
//! no object streams. It does not read a file written by anything else.

use std::ops::Range;

use super::encrypt::{ObjectEntry, hex_nibble, read_u32, skip_ws};

const HEX: [u8; 16] = *b"0123456789ABCDEF";

/// A file copied through [`rewrite_objects`].
pub(crate) struct Rewritten {
    pub bytes:   Vec<u8>,
    /// The objects at their new offsets, in the order they are written.
    pub entries: Vec<ObjectEntry>,
}

/// Copies the header and every object in `entries` from `src` and drops what follows the
/// last object (the old xref and trailer). Each string is replaced by a hex string of
/// `transform(its bytes)` and each stream's data by `transform(its data)`, with the
/// stream's `/Length` set to the new size.
pub(crate) fn rewrite_objects(
    src: &[u8],
    entries: &[ObjectEntry],
    transform: &mut dyn FnMut(&[u8]) -> Vec<u8>,
) -> Result<Rewritten, String> {
    let mut ordered: Vec<&ObjectEntry> = entries.iter().collect();
    ordered.sort_by_key(|entry| entry.offset);

    let header_end = ordered.first().map_or(src.len(), |entry| entry.offset);
    let header = src.get(..header_end).ok_or_else(|| "first object lies past the end of the file".to_string())?;

    let mut out = Vec::with_capacity(src.len() + src.len() / 8);
    out.extend_from_slice(header);

    let mut moved = Vec::with_capacity(ordered.len());
    for entry in ordered {
        moved.push(ObjectEntry { obj_num: entry.obj_num, gen_num: entry.gen_num, offset: out.len() });
        rewrite_object(src, entry, &mut out, transform)
            .map_err(|reason| format!("object {} {}: {reason}", entry.obj_num, entry.gen_num))?;
    }
    Ok(Rewritten { bytes: out, entries: moved })
}

fn rewrite_object(
    src: &[u8],
    entry: &ObjectEntry,
    out: &mut Vec<u8>,
    transform: &mut dyn FnMut(&[u8]) -> Vec<u8>,
) -> Result<(), String> {
    let body_start = object_body_start(src, entry.offset).ok_or_else(|| "no `N G obj` at its xref offset".to_string())?;
    out.extend_from_slice(format!("{} {} obj", entry.obj_num, entry.gen_num).as_bytes());
    copy_body(src, body_start, out, transform)?;
    out.push(b'\n');
    Ok(())
}

/// The position after the `obj` keyword of the object header at `offset`.
fn object_body_start(src: &[u8], offset: usize) -> Option<usize> {
    let rest = src.get(offset..)?;
    let (_, rest) = read_u32(skip_ws(rest))?;
    let (_, rest) = read_u32(skip_ws(rest))?;
    let rest = skip_ws(rest).strip_prefix(b"obj")?;
    Some(src.len() - rest.len())
}

/// Copies the object body from `pos` to its `endobj`, which it writes without the
/// newline after it.
fn copy_body(
    src: &[u8],
    mut pos: usize,
    out: &mut Vec<u8>,
    transform: &mut dyn FnMut(&[u8]) -> Vec<u8>,
) -> Result<(), String> {
    let body_out = out.len();
    loop {
        let Some(&byte) = src.get(pos) else {
            return Err("not closed by `endobj`".to_string());
        };
        match byte {
            b'%' => {
                let end = src[pos..].iter().position(|&b| b == b'\n' || b == b'\r').map_or(src.len(), |i| pos + i);
                out.extend_from_slice(&src[pos..end]);
                pos = end;
            }
            b'(' => {
                let (plain, next) = read_literal_string(src, pos).ok_or_else(|| "unterminated string".to_string())?;
                push_hex_string(out, &transform(&plain));
                pos = next;
            }
            b'<' if src.get(pos + 1) == Some(&b'<') => {
                out.extend_from_slice(b"<<");
                pos += 2;
            }
            b'<' => {
                let (plain, next) = read_hex_string(src, pos).ok_or_else(|| "unterminated hex string".to_string())?;
                push_hex_string(out, &transform(&plain));
                pos = next;
            }
            b'/' => {
                let end = regular_run_end(src, pos + 1);
                out.extend_from_slice(&src[pos..end]);
                pos = end;
            }
            _ if is_whitespace(byte) || is_delimiter(byte) => {
                out.push(byte);
                pos += 1;
            }
            _ => {
                let end = regular_run_end(src, pos);
                match &src[pos..end] {
                    b"stream" => pos = copy_stream(src, end, body_out, out, transform)?,
                    b"endobj" => {
                        out.extend_from_slice(b"endobj");
                        return Ok(());
                    }
                    token => {
                        out.extend_from_slice(token);
                        pos = end;
                    }
                }
            }
        }
    }
}

/// Writes `stream`, the data run through `transform` and nothing else: the dictionary
/// before it is already in `out` from `body_out` on, and the `endstream` after the data
/// is copied by the caller's loop. Returns the position after the source's data.
fn copy_stream(
    src: &[u8],
    after_keyword: usize,
    body_out: usize,
    out: &mut Vec<u8>,
    transform: &mut dyn FnMut(&[u8]) -> Vec<u8>,
) -> Result<usize, String> {
    let data_start = match (src.get(after_keyword), src.get(after_keyword + 1)) {
        (Some(b'\r'), Some(b'\n')) => after_keyword + 2,
        (Some(b'\n'), _) => after_keyword + 1,
        _ => return Err("`stream` is not followed by an end of line".to_string()),
    };

    let length_range = length_value_range(&out[body_out..])?;
    let length_text = &out[body_out + length_range.start..body_out + length_range.end];
    let length: usize = std::str::from_utf8(length_text)
        .ok()
        .and_then(|text| text.parse().ok())
        .ok_or_else(|| "/Length is not a number that fits".to_string())?;

    let data_end = data_start + length;
    let data = src.get(data_start..data_end).ok_or_else(|| "stream data runs past the end of the file".to_string())?;
    let sealed = transform(data);

    let new_length = sealed.len().to_string().into_bytes();
    out.splice(body_out + length_range.start..body_out + length_range.end, new_length);
    out.extend_from_slice(b"stream\n");
    out.extend_from_slice(&sealed);
    Ok(data_end)
}

/// Where the number after `/Length` sits in a stream dictionary. `/Length1` and the
/// like are other keys and are skipped; an indirect length (`N G R`) is refused.
fn length_value_range(dict: &[u8]) -> Result<Range<usize>, String> {
    const KEY: &[u8] = b"/Length";
    let mut from = 0;
    while let Some(found) = dict[from..].windows(KEY.len()).position(|window| window == KEY) {
        let after_key = from + found + KEY.len();
        let ends_name = dict.get(after_key).is_none_or(|&b| is_whitespace(b) || is_delimiter(b));
        if ends_name {
            let start = after_key + dict[after_key..].iter().take_while(|&&b| is_whitespace(b)).count();
            let digits = dict[start..].iter().take_while(|b| b.is_ascii_digit()).count();
            if digits == 0 {
                return Err("/Length is not a number".to_string());
            }
            let end = start + digits;
            if let Some((_, after_generation)) = read_u32(skip_ws(&dict[end..]))
                && skip_ws(after_generation).first() == Some(&b'R')
            {
                return Err("an indirect /Length is not supported".to_string());
            }
            return Ok(start..end);
        }
        from = after_key;
    }
    Err("stream dictionary has no /Length".to_string())
}

/// Reads the literal string whose `(` is at `open`: its bytes with escapes resolved, and
/// the position after the closing `)`.
fn read_literal_string(src: &[u8], open: usize) -> Option<(Vec<u8>, usize)> {
    let mut plain = Vec::new();
    let mut depth = 1usize;
    let mut pos = open + 1;
    while let Some(&byte) = src.get(pos) {
        pos += 1;
        match byte {
            b'\\' => {
                let escaped = *src.get(pos)?;
                pos += 1;
                match escaped {
                    b'n' => plain.push(b'\n'),
                    b'r' => plain.push(b'\r'),
                    b't' => plain.push(b'\t'),
                    b'b' => plain.push(0x08),
                    b'f' => plain.push(0x0C),
                    b'0'..=b'7' => {
                        let mut value = u32::from(escaped - b'0');
                        for _ in 0..2 {
                            match src.get(pos) {
                                Some(&digit @ b'0'..=b'7') => {
                                    value = value * 8 + u32::from(digit - b'0');
                                    pos += 1;
                                }
                                _ => break,
                            }
                        }
                        plain.push((value & 0xFF) as u8);
                    }
                    // A backslash before an end of line joins the lines.
                    b'\r' => {
                        if src.get(pos) == Some(&b'\n') {
                            pos += 1;
                        }
                    }
                    b'\n' => {}
                    other => plain.push(other),
                }
            }
            b'(' => {
                depth += 1;
                plain.push(byte);
            }
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some((plain, pos));
                }
                plain.push(byte);
            }
            // An unescaped end of line in a string counts as a line feed.
            b'\r' => {
                if src.get(pos) == Some(&b'\n') {
                    pos += 1;
                }
                plain.push(b'\n');
            }
            _ => plain.push(byte),
        }
    }
    None
}

/// Reads the hex string whose `<` is at `open`: its bytes (an odd last digit counts as
/// followed by 0), and the position after the closing `>`.
fn read_hex_string(src: &[u8], open: usize) -> Option<(Vec<u8>, usize)> {
    let mut nibbles = Vec::new();
    let mut pos = open + 1;
    loop {
        let byte = *src.get(pos)?;
        pos += 1;
        if byte == b'>' {
            break;
        }
        if byte.is_ascii_hexdigit() {
            nibbles.push(hex_nibble(byte));
        }
    }
    let plain = nibbles.chunks(2).map(|pair| (pair[0] << 4) | pair.get(1).copied().unwrap_or(0)).collect();
    Some((plain, pos))
}

fn push_hex_string(out: &mut Vec<u8>, bytes: &[u8]) {
    out.push(b'<');
    for &byte in bytes {
        out.push(HEX[usize::from(byte >> 4)]);
        out.push(HEX[usize::from(byte & 0xF)]);
    }
    out.push(b'>');
}

fn regular_run_end(src: &[u8], from: usize) -> usize {
    from + src[from..].iter().take_while(|&&b| !is_whitespace(b) && !is_delimiter(b)).count()
}

fn is_whitespace(byte: u8) -> bool {
    matches!(byte, 0 | b'\t' | b'\n' | 0x0C | b'\r' | b' ')
}

fn is_delimiter(byte: u8) -> bool {
    matches!(byte, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(obj_num: u32, offset: usize) -> ObjectEntry {
        ObjectEntry { obj_num, gen_num: 0, offset }
    }

    // Wraps the bytes in `[` and `]` so a test can see what went through the transform.
    fn bracket(plain: &[u8]) -> Vec<u8> {
        let mut out = b"[".to_vec();
        out.extend_from_slice(plain);
        out.push(b']');
        out
    }

    fn rewrite_one(file: &[u8]) -> Result<String, String> {
        let rewritten = rewrite_objects(file, &[entry(1, 0)], &mut |plain| bracket(plain))?;
        Ok(String::from_utf8_lossy(&rewritten.bytes).into_owned())
    }

    #[test]
    fn a_literal_string_becomes_a_hex_string_of_the_transformed_bytes() {
        let out = rewrite_one(b"1 0 obj\n<< /Title (Hi) >>\nendobj\n").unwrap();
        // "[Hi]" in hex
        assert_eq!(out, "1 0 obj\n<< /Title <5B48695D> >>\nendobj\n");
    }

    #[test]
    fn escapes_in_a_literal_string_are_resolved_before_the_transform() {
        let out = rewrite_one(b"1 0 obj\n(a\\(b\\)\\\\\\n\\101(nested))\nendobj\n").unwrap();
        // [a(b)\<LF>A(nested)]
        assert_eq!(out, "1 0 obj\n<5B612862295C0A41286E6573746564295D>\nendobj\n");
    }

    #[test]
    fn a_hex_string_with_an_odd_last_digit_reads_it_as_followed_by_zero() {
        let out = rewrite_one(b"1 0 obj\n<41 4>\nendobj\n").unwrap();
        // [A@] : 41 40
        assert_eq!(out, "1 0 obj\n<5B41405D>\nendobj\n");
    }

    #[test]
    fn names_numbers_and_dictionary_brackets_are_copied_as_they_are() {
        let out = rewrite_one(b"1 0 obj\n<< /Type /Page /Count 3 /Kids [2 0 R 3 0 R] >>\nendobj\n").unwrap();
        assert_eq!(out, "1 0 obj\n<< /Type /Page /Count 3 /Kids [2 0 R 3 0 R] >>\nendobj\n");
    }

    #[test]
    fn a_stream_is_transformed_and_its_length_follows() {
        let out = rewrite_one(b"1 0 obj\n<< /Length 5 >>\nstream\nHello\nendstream\nendobj\n").unwrap();
        assert_eq!(out, "1 0 obj\n<< /Length 7 >>\nstream\n[Hello]\nendstream\nendobj\n");
    }

    #[test]
    fn length1_is_not_taken_for_length() {
        let out = rewrite_one(b"1 0 obj\n<< /Length1 99 /Length 2 >>\nstream\nab\nendstream\nendobj\n").unwrap();
        assert_eq!(out, "1 0 obj\n<< /Length1 99 /Length 4 >>\nstream\n[ab]\nendstream\nendobj\n");
    }

    #[test]
    fn stream_data_that_looks_like_pdf_syntax_is_not_read() {
        let out = rewrite_one(b"1 0 obj\n<< /Length 18 >>\nstream\n(x) endobj <</Foo>>\nendstream\nendobj\n").unwrap();
        assert!(out.starts_with("1 0 obj\n<< /Length 20 >>\nstream\n[(x) endobj"), "{out}");
        assert!(out.ends_with("endstream\nendobj\n"), "{out}");
    }

    #[test]
    fn a_stream_with_a_crlf_after_the_keyword_is_written_with_a_line_feed() {
        let out = rewrite_one(b"1 0 obj\n<< /Length 1 >>\nstream\r\nx\nendstream\nendobj\n").unwrap();
        assert_eq!(out, "1 0 obj\n<< /Length 3 >>\nstream\n[x]\nendstream\nendobj\n");
    }

    #[test]
    fn an_indirect_length_is_refused() {
        let error = rewrite_one(b"1 0 obj\n<< /Length 9 0 R >>\nstream\nx\nendstream\nendobj\n").unwrap_err();
        assert!(error.contains("indirect /Length"), "{error}");
    }

    #[test]
    fn a_stream_dictionary_without_a_length_is_refused() {
        let error = rewrite_one(b"1 0 obj\n<< /Filter /FlateDecode >>\nstream\nx\nendstream\nendobj\n").unwrap_err();
        assert!(error.contains("no /Length"), "{error}");
    }

    #[test]
    fn a_length_past_the_end_of_the_file_is_refused() {
        let error = rewrite_one(b"1 0 obj\n<< /Length 999 >>\nstream\nx\nendstream\nendobj\n").unwrap_err();
        assert!(error.contains("runs past the end"), "{error}");
    }

    #[test]
    fn an_object_without_endobj_is_refused() {
        let error = rewrite_one(b"1 0 obj\n<< /A 1 >>\n").unwrap_err();
        assert!(error.contains("endobj"), "{error}");
    }

    #[test]
    fn objects_move_to_the_offsets_they_are_written_at() {
        let file = b"%PDF-1.7\n1 0 obj\n(a)\nendobj\n2 0 obj\n(b)\nendobj\nxref\n";
        let first = file.windows(7).position(|w| w == b"1 0 obj").unwrap();
        let second = file.windows(7).position(|w| w == b"2 0 obj").unwrap();
        let rewritten = rewrite_objects(file, &[entry(1, first), entry(2, second)], &mut |plain| bracket(plain)).unwrap();

        assert!(rewritten.bytes.starts_with(b"%PDF-1.7\n"));
        assert!(!rewritten.bytes.windows(4).any(|w| w == b"xref"), "the old xref is dropped");
        for moved in &rewritten.entries {
            let at = &rewritten.bytes[moved.offset..];
            assert!(at.starts_with(format!("{} 0 obj", moved.obj_num).as_bytes()), "object {} at {}", moved.obj_num, moved.offset);
        }
    }
}
