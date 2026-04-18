//! RC4-128 PDF encryption (ISO 32000-1 §7.6, algorithm V=2 R=3).
//!
//! Applied as a post-processing pass on the raw bytes returned by pdf-writer.
//! RC4 is length-preserving so all object offsets remain valid after encryption;
//! the pass only appends a new /Encrypt object and rewrites the xref/trailer.
//!
//! # Compliance note
//! This implementation uses RC4-128 (V=2 R=3), which is the weakest encryption
//! scheme defined in the PDF specification and is considered legacy. It is
//! intentionally chosen here for maximum reader compatibility (all PDF viewers
//! support it). For environments with stricter security requirements (e.g. PCI-DSS,
//! HIPAA, or ISO 27001 compliance), AES-256 encryption (V=5 R=6, PDF 1.7 Ext 3)
//! should be used instead.

use md5::{Digest, Md5};

// ISO 32000-1 §7.6.3.3 — standard password padding string
const PADDING: [u8; 32] = [
    0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41,
    0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA, 0x01, 0x08,
    0x2E, 0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80,
    0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53, 0x69, 0x7A,
];

const HEX: [u8; 16] = *b"0123456789ABCDEF";

// ── Public types ───────────────────────────────────────────────────────────────

pub struct Permissions {
    pub print:         bool,  // spec bit 3
    pub modify:        bool,  // spec bit 4
    pub copy:          bool,  // spec bit 5
    pub annotate:      bool,  // spec bit 6
    pub fill_forms:    bool,  // spec bit 9
    pub accessibility: bool,  // spec bit 10
    pub assemble:      bool,  // spec bit 11
    pub print_hq:      bool,  // spec bit 12
}

impl Default for Permissions {
    fn default() -> Self {
        Self {
            print: true, modify: true, copy: true, annotate: true,
            fill_forms: true, accessibility: true, assemble: true, print_hq: true,
        }
    }
}

pub struct EncryptConfig {
    pub user_password:  String,
    pub owner_password: String,
    pub permissions:    Permissions,
}

// ── Internal types ─────────────────────────────────────────────────────────────

struct ObjectEntry {
    obj_num: u32,
    gen_num: u32,
    offset:  usize,
}

struct XrefInfo {
    entries:     Vec<ObjectEntry>,
    xref_offset: usize,
    root_ref:    (u32, u32),
    info_ref:    Option<(u32, u32)>,
}

// ── Crypto primitives ──────────────────────────────────────────────────────────

fn md5_hash(data: &[u8]) -> [u8; 16] {
    let result = Md5::digest(data);
    let mut arr = [0u8; 16];
    arr.copy_from_slice(&result);
    arr
}

fn rc4(key: &[u8], data: &mut [u8]) {
    let mut s = [0u8; 256];
    for (i, b) in s.iter_mut().enumerate() { *b = i as u8; }
    let mut j = 0u8;
    for i in 0..256 {
        j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
        s.swap(i, j as usize);
    }
    let (mut i, mut j) = (0u8, 0u8);
    for b in data.iter_mut() {
        i = i.wrapping_add(1);
        j = j.wrapping_add(s[i as usize]);
        s.swap(i as usize, j as usize);
        *b ^= s[s[i as usize].wrapping_add(s[j as usize]) as usize];
    }
}

// Pad or truncate a password to exactly 32 bytes using the standard padding.
fn pad_password(pw: &str) -> [u8; 32] {
    let bytes = pw.as_bytes();
    let take = bytes.len().min(32);
    let mut out = [0u8; 32];
    out[..take].copy_from_slice(&bytes[..take]);
    out[take..].copy_from_slice(&PADDING[..32 - take]);
    out
}

// ISO 32000-1 §7.6.3.3 — compose the P integer.
// Start from 0xFFFFFFFC (reserved bits 1-2 clear, all others set)
// then clear the permission bits for any disallowed operation.
fn compute_p(perms: &Permissions) -> i32 {
    let mut p: u32 = 0xFFFF_FFFC;
    if !perms.print        { p &= !(1 << 2);  }
    if !perms.modify       { p &= !(1 << 3);  }
    if !perms.copy         { p &= !(1 << 4);  }
    if !perms.annotate     { p &= !(1 << 5);  }
    if !perms.fill_forms   { p &= !(1 << 8);  }
    if !perms.accessibility { p &= !(1 << 9); }
    if !perms.assemble     { p &= !(1 << 10); }
    if !perms.print_hq     { p &= !(1 << 11); }
    p as i32
}

// Algorithm 3: Owner entry (32 bytes).
fn derive_o_entry(user_pw: &str, owner_pw: &str) -> [u8; 32] {
    let owner_input = if owner_pw.is_empty() { user_pw } else { owner_pw };
    let mut rc4_key = md5_hash(&pad_password(owner_input));
    for _ in 0..50 { rc4_key = md5_hash(&rc4_key); }
    let mut o = pad_password(user_pw);
    rc4(&rc4_key, &mut o);
    for i in 1u8..=19 {
        let k: [u8; 16] = rc4_key.map(|b| b ^ i);
        rc4(&k, &mut o);
    }
    o
}

// Algorithm 2: File encryption key (16 bytes, V=2 R=3).
fn derive_fek(user_pw: &str, o_entry: &[u8; 32], p: i32, file_id: &[u8; 16]) -> [u8; 16] {
    let mut input = [0u8; 84];
    input[0..32].copy_from_slice(&pad_password(user_pw));
    input[32..64].copy_from_slice(o_entry);
    input[64..68].copy_from_slice(&(p as u32).to_le_bytes());
    input[68..84].copy_from_slice(file_id);
    let mut key = md5_hash(&input);
    for _ in 0..50 { key = md5_hash(&key); }
    key
}

// Algorithm 5: User entry (32 bytes, R=3).
fn derive_u_entry(fek: &[u8; 16], file_id: &[u8; 16]) -> [u8; 32] {
    let mut hash_input = [0u8; 48];
    hash_input[..32].copy_from_slice(&PADDING);
    hash_input[32..].copy_from_slice(file_id);
    let hash = md5_hash(&hash_input);
    let mut u = hash;
    rc4(fek, &mut u);
    for i in 1u8..=19 {
        let k: [u8; 16] = fek.map(|b| b ^ i);
        rc4(&k, &mut u);
    }
    let mut out = [0u8; 32];
    out[..16].copy_from_slice(&u);
    out
}

// Algorithm 1: Per-object key (16 bytes, keylen=16 for V=2).
fn derive_obj_key(fek: &[u8; 16], obj_num: u32, gen_num: u32) -> [u8; 16] {
    let mut input = [0u8; 21];
    input[..16].copy_from_slice(fek);
    let obj_le = obj_num.to_le_bytes();
    input[16..19].copy_from_slice(&obj_le[..3]);
    let gen_le = gen_num.to_le_bytes();
    input[19..21].copy_from_slice(&gen_le[..2]);
    md5_hash(&input)
}

// ── Byte helpers ───────────────────────────────────────────────────────────────

fn find_bytes(data: &[u8], needle: &[u8]) -> Option<usize> {
    data.windows(needle.len()).position(|w| w == needle)
}

fn rfind_bytes(data: &[u8], needle: &[u8]) -> Option<usize> {
    let n = needle.len();
    if data.len() < n { return None; }
    for i in (0..=data.len() - n).rev() {
        if &data[i..i + n] == needle { return Some(i); }
    }
    None
}

fn skip_ws(data: &[u8]) -> &[u8] {
    let n = data.iter()
        .take_while(|&&b| b == b' ' || b == b'\t' || b == b'\r' || b == b'\n')
        .count();
    &data[n..]
}

fn read_u32(data: &[u8]) -> Option<(u32, &[u8])> {
    let n = data.iter().take_while(|&&b| b.is_ascii_digit()).count();
    if n == 0 { return None; }
    let v: u32 = std::str::from_utf8(&data[..n]).ok()?.parse().ok()?;
    Some((v, &data[n..]))
}

// Find "/Key N G R" in a trailer dict and return (N, G).
fn parse_ref_in(data: &[u8], key: &[u8]) -> Option<(u32, u32)> {
    let pos = data.windows(key.len()).position(|w| w == key)?;
    let after = skip_ws(&data[pos + key.len()..]);
    let (obj, rest) = read_u32(after)?;
    let rest = skip_ws(rest);
    let (gen_num, _) = read_u32(rest)?;
    Some((obj, gen_num))
}

fn hex_nibble(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
    }
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

// ── Xref parser ────────────────────────────────────────────────────────────────

fn parse_xref(bytes: &[u8]) -> Result<XrefInfo, String> {
    // Locate `startxref` in the last 256 bytes of the file
    let tail_start = bytes.len().saturating_sub(256);
    let tail = &bytes[tail_start..];
    let rel = rfind_bytes(tail, b"startxref")
        .ok_or_else(|| "startxref not found".to_string())?;

    let (xref_offset, _) = read_u32(skip_ws(&tail[rel + 9..]))
        .ok_or_else(|| "invalid startxref value".to_string())?;
    let xref_offset = xref_offset as usize;

    if xref_offset >= bytes.len() {
        return Err(format!("xref offset {xref_offset} exceeds file size"));
    }
    let xd = &bytes[xref_offset..];
    if !xd.starts_with(b"xref") {
        return Err(format!("expected 'xref' at offset {xref_offset}"));
    }

    let mut pos = 4;
    while pos < xd.len() && (xd[pos] == b'\r' || xd[pos] == b'\n') { pos += 1; }

    let mut entries = Vec::new();

    loop {
        while pos < xd.len() && (xd[pos] == b' ' || xd[pos] == b'\t') { pos += 1; }
        if pos >= xd.len() {
            return Err("truncated xref section".to_string());
        }
        if xd[pos..].starts_with(b"trailer") { break; }

        // Subsection header: "first_obj count\n"
        let line_len = xd[pos..].iter().position(|&b| b == b'\n')
            .ok_or_else(|| "no newline in xref subsection header".to_string())?;
        let header = &xd[pos..pos + line_len];
        let header = if header.ends_with(b"\r") { &header[..header.len() - 1] } else { header };

        let sp = header.iter().position(|&b| b == b' ')
            .ok_or_else(|| "no space in xref subsection header".to_string())?;
        let first_obj: u32 = std::str::from_utf8(&header[..sp]).ok()
            .and_then(|s| s.trim().parse().ok())
            .ok_or_else(|| "bad first_obj in xref header".to_string())?;
        let count: u32 = std::str::from_utf8(&header[sp + 1..]).ok()
            .and_then(|s| s.trim().parse().ok())
            .ok_or_else(|| "bad count in xref header".to_string())?;
        pos += line_len + 1;

        // Each entry is exactly 20 bytes: "OOOOOOOOOO GGGGG n\r\n"
        // Flag is at byte offset 17 within the entry.
        for i in 0..count {
            if pos + 20 > xd.len() {
                return Err("truncated xref table".to_string());
            }
            let entry = &xd[pos..pos + 20];
            let obj_offset: usize = std::str::from_utf8(&entry[0..10]).ok()
                .and_then(|s| s.trim().parse().ok())
                .ok_or_else(|| "bad xref entry offset".to_string())?;
            let gen_num: u32 = std::str::from_utf8(&entry[11..16]).ok()
                .and_then(|s| s.trim().parse().ok())
                .ok_or_else(|| "bad xref entry gen".to_string())?;
            if entry[17] == b'n' {
                entries.push(ObjectEntry { obj_num: first_obj + i, gen_num, offset: obj_offset });
            }
            pos += 20;
        }
    }

    let trailer_slice = &xd[pos..];
    let root_ref = parse_ref_in(trailer_slice, b"/Root")
        .ok_or_else(|| "no /Root in trailer".to_string())?;
    let info_ref = parse_ref_in(trailer_slice, b"/Info");

    Ok(XrefInfo { entries, xref_offset, root_ref, info_ref })
}

// ── Object body scanner ────────────────────────────────────────────────────────

fn encrypt_literal_string(data: &mut [u8], open_pos: usize, end: usize, key: &[u8; 16]) -> usize {
    let content_start = open_pos + 1;
    let mut pos = content_start;
    let mut depth: usize = 1;
    while pos < end && depth > 0 {
        match data[pos] {
            b'\\' => pos += 2,
            b'(' => { depth += 1; pos += 1; }
            b')' => {
                depth -= 1;
                if depth == 0 {
                    rc4(key, &mut data[content_start..pos]);
                    return pos + 1;
                }
                pos += 1;
            }
            _ => pos += 1,
        }
    }
    pos
}

fn encrypt_hex_string(data: &mut [u8], open_pos: usize, end: usize, key: &[u8; 16]) -> usize {
    let hex_start = open_pos + 1;
    let mut pos = hex_start;
    while pos < end && data[pos] != b'>' { pos += 1; }
    if pos >= end { return pos; }

    let hex_bytes: Vec<u8> = data[hex_start..pos].to_vec();
    // pdf-writer always produces even-length hex strings; skip gracefully if not
    if hex_bytes.len() % 2 != 0 { return pos + 1; }

    let n = hex_bytes.len() / 2;
    let mut plain = vec![0u8; n];
    for i in 0..n {
        plain[i] = (hex_nibble(hex_bytes[i * 2]) << 4) | hex_nibble(hex_bytes[i * 2 + 1]);
    }
    rc4(key, &mut plain);
    for i in 0..n {
        data[hex_start + i * 2]     = HEX[plain[i] as usize >> 4];
        data[hex_start + i * 2 + 1] = HEX[plain[i] as usize & 0xF];
    }
    pos + 1
}

fn encrypt_stream(data: &mut [u8], stream_kw: usize, end: usize, key: &[u8; 16]) -> usize {
    // Skip "stream" + CRLF or LF
    let mut stream_start = stream_kw + 6;
    if stream_start < end && data[stream_start] == b'\r' { stream_start += 1; }
    if stream_start < end && data[stream_start] == b'\n' { stream_start += 1; }

    let mut pos = stream_start;
    while pos < end {
        // Check \r\nendstream before \nendstream to handle both terminators correctly
        if pos + 11 <= end && &data[pos..pos + 11] == b"\r\nendstream" {
            rc4(key, &mut data[stream_start..pos]);
            return pos + 11;
        }
        if pos + 10 <= end && &data[pos..pos + 10] == b"\nendstream" {
            rc4(key, &mut data[stream_start..pos]);
            return pos + 10;
        }
        pos += 1;
    }
    pos
}

fn scan_body(data: &mut [u8], start: usize, end: usize, key: &[u8; 16]) {
    let mut pos = start;
    while pos < end {
        match data[pos] {
            b'%' => {
                pos += 1;
                while pos < end && data[pos] != b'\n' { pos += 1; }
                if pos < end { pos += 1; }
            }
            b'(' => {
                pos = encrypt_literal_string(data, pos, end, key);
            }
            b'<' => {
                if pos + 1 < end && data[pos + 1] == b'<' {
                    pos += 2; // dict open <<, not a hex string
                } else {
                    pos = encrypt_hex_string(data, pos, end, key);
                }
            }
            b's' => {
                if pos + 6 <= end && &data[pos..pos + 6] == b"stream" {
                    let next = pos + 6;
                    if next < end && (data[next] == b'\n' || data[next] == b'\r') {
                        pos = encrypt_stream(data, pos, end, key);
                    } else {
                        pos += 1;
                    }
                } else {
                    pos += 1;
                }
            }
            _ => pos += 1,
        }
    }
}

fn encrypt_object(out: &mut [u8], offset: usize, key: &[u8; 16]) {
    // Skip past "N G obj\n" (or "N G obj\r\n") to find body start
    let obj_slice = &out[offset..];
    let body_rel = obj_slice.windows(4)
        .position(|w| w == b" obj")
        .map(|i| {
            let mut j = i + 4;
            if j < obj_slice.len() && obj_slice[j] == b'\r' { j += 1; }
            if j < obj_slice.len() && obj_slice[j] == b'\n' { j += 1; }
            j
        })
        .unwrap_or(0);
    let body_start = offset + body_rel;

    // Use \nendobj as boundary — avoids false matches of "endobj" inside binary streams
    let search = &out[body_start..];
    let body_end = body_start + find_bytes(search, b"\nendobj").unwrap_or(search.len());

    if body_start < body_end {
        scan_body(out, body_start, body_end, key);
    }
}

// ── Output assembly ────────────────────────────────────────────────────────────

fn append_encrypt_dict(out: &mut Vec<u8>, obj_num: u32, o: &[u8; 32], u: &[u8; 32], p: i32) {
    let s = format!(
        "{obj_num} 0 obj\n<<\n/Filter /Standard\n/V 2\n/R 3\n/Length 128\n\
         /P {p}\n/O <{}>\n/U <{}>\n>>\nendobj\n",
        to_hex(o), to_hex(u)
    );
    out.extend_from_slice(s.as_bytes());
}

fn write_xref_and_trailer(
    out: &mut Vec<u8>,
    original_entries: &[ObjectEntry],
    encrypt_obj_num: u32,
    encrypt_obj_offset: usize,
    xref_info: &XrefInfo,
    file_id: &[u8; 16],
) {
    let new_xref_offset = out.len();

    let mut all: Vec<(u32, u32, usize)> = original_entries.iter()
        .map(|e| (e.obj_num, e.gen_num, e.offset))
        .chain(std::iter::once((encrypt_obj_num, 0u32, encrypt_obj_offset)))
        .collect();
    all.sort_by_key(|&(n, _, _)| n);

    let max_obj = all.last().map(|&(n, _, _)| n).unwrap_or(0);
    let size = max_obj + 1;

    // Build per-object offset lookup (index = obj_num)
    let mut table = vec![(0usize, 0u32, false); size as usize];
    for &(obj_num, gen_num, offset) in &all {
        if (obj_num as usize) < table.len() {
            table[obj_num as usize] = (offset, gen_num, true);
        }
    }

    // xref table — single subsection 0..size
    out.extend_from_slice(b"xref\n");
    out.extend_from_slice(format!("0 {size}\n").as_bytes());
    // Object 0 is always the free list head: 20 bytes exactly
    out.extend_from_slice(b"0000000000 65535 f\r\n");
    for i in 1..size {
        let (offset, gen_num, in_use) = table[i as usize];
        if in_use {
            out.extend_from_slice(format!("{offset:010} {gen_num:05} n\r\n").as_bytes());
        } else {
            out.extend_from_slice(b"0000000000 65535 f\r\n");
        }
    }

    // trailer dict
    let (root_obj, root_gen) = xref_info.root_ref;
    let id_hex = to_hex(file_id);
    out.extend_from_slice(b"trailer\n<<\n");
    out.extend_from_slice(format!("/Size {size}\n").as_bytes());
    out.extend_from_slice(format!("/Root {root_obj} {root_gen} R\n").as_bytes());
    if let Some((info_obj, info_gen)) = xref_info.info_ref {
        out.extend_from_slice(format!("/Info {info_obj} {info_gen} R\n").as_bytes());
    }
    out.extend_from_slice(format!("/Encrypt {encrypt_obj_num} 0 R\n").as_bytes());
    out.extend_from_slice(format!("/ID [<{id_hex}><{id_hex}>]\n").as_bytes());
    out.extend_from_slice(b">>\n");
    out.extend_from_slice(format!("startxref\n{new_xref_offset}\n%%EOF\n").as_bytes());
}

// ── Public entry point ─────────────────────────────────────────────────────────

/// Post-process a pdf-writer–generated PDF to add RC4-128 encryption (V=2 R=3).
///
/// Returns `Err` if the PDF structure is malformed (e.g. missing or corrupt xref).
pub fn encrypt_pdf(bytes: &[u8], config: &EncryptConfig) -> Result<Vec<u8>, String> {
    let xref = parse_xref(bytes)?;

    // File ID: MD5 of the unencrypted bytes — deterministic, no random salt needed
    let file_id = md5_hash(bytes);
    let p       = compute_p(&config.permissions);
    let o_entry = derive_o_entry(&config.user_password, &config.owner_password);
    let fek     = derive_fek(&config.user_password, &o_entry, p, &file_id);
    let u_entry = derive_u_entry(&fek, &file_id);

    // Drop the original xref + trailer; keep the object body bytes
    let mut out = bytes[..xref.xref_offset].to_vec();

    // Encrypt all in-use objects in place (RC4 is length-preserving)
    for entry in &xref.entries {
        let obj_key = derive_obj_key(&fek, entry.obj_num, entry.gen_num);
        encrypt_object(&mut out, entry.offset, &obj_key);
    }

    // Append /Encrypt dict as a new indirect object
    let encrypt_obj_num    = xref.entries.iter().map(|e| e.obj_num).max().unwrap_or(0) + 1;
    let encrypt_obj_offset = out.len();
    append_encrypt_dict(&mut out, encrypt_obj_num, &o_entry, &u_entry, p);

    // Write fresh xref and trailer referencing the /Encrypt object and /ID
    write_xref_and_trailer(&mut out, &xref.entries, encrypt_obj_num, encrypt_obj_offset, &xref, &file_id);

    Ok(out)
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // RC4 known-answer test (https://en.wikipedia.org/wiki/RC4#Test_vectors)
    #[test]
    fn rc4_known_vector() {
        let mut data = *b"Plaintext";
        rc4(b"Key", &mut data);
        assert_eq!(data, [0xBB, 0xF3, 0x16, 0xE8, 0xD9, 0x40, 0xAF, 0x0A, 0xD3]);
    }

    #[test]
    fn rc4_is_symmetric() {
        let key  = b"secret";
        let orig = b"Hello, PDF!";
        let mut buf = *orig;
        rc4(key, &mut buf);
        assert_ne!(&buf, orig);
        rc4(key, &mut buf); // second application should restore plaintext
        assert_eq!(&buf, orig);
    }

    // P value tests
    #[test]
    fn p_all_allowed() {
        assert_eq!(compute_p(&Permissions::default()) as u32, 0xFFFF_FFFC);
    }

    #[test]
    fn p_no_print() {
        let p = compute_p(&Permissions { print: false, ..Default::default() }) as u32;
        assert_eq!(p, 0xFFFF_FFFC & !(1u32 << 2));
    }

    #[test]
    fn p_no_copy_no_modify() {
        let p = compute_p(&Permissions { copy: false, modify: false, ..Default::default() }) as u32;
        assert_eq!(p, 0xFFFF_FFFC & !(1u32 << 3) & !(1u32 << 4));
    }

    // ── Full-pipeline tests (require render_xml_to_pdf_bytes) ─────────────────

    fn minimal_xml() -> &'static str {
        r#"<lpdf version="1"><document size="a4" margin="28pt"><pages><page><text size="m">Hello</text></page></pages></document></lpdf>"#
    }

    fn encrypt_minimal(user_pw: &str, owner_pw: &str) -> Vec<u8> {
        let plain = crate::LpdfEngine::render_xml_to_pdf_bytes(minimal_xml())
            .expect("render failed");
        let cfg = EncryptConfig {
            user_password:  user_pw.to_string(),
            owner_password: owner_pw.to_string(),
            permissions:    Permissions::default(),
        };
        encrypt_pdf(&plain, &cfg).expect("encrypt_pdf failed")
    }

    #[test]
    fn encrypted_pdf_has_pdf_header() {
        assert_eq!(&encrypt_minimal("", "owner")[..5], b"%PDF-");
    }

    #[test]
    fn encrypted_pdf_contains_encrypt_dict() {
        let enc = encrypt_minimal("", "owner");
        assert!(enc.windows(8).any(|w| w == b"/Encrypt"));
    }

    #[test]
    fn encrypted_pdf_contains_file_id() {
        let enc = encrypt_minimal("", "owner");
        assert!(enc.windows(3).any(|w| w == b"/ID"));
    }

    #[test]
    fn encrypted_pdf_contains_standard_filter() {
        let enc = encrypt_minimal("", "owner");
        assert!(enc.windows(9).any(|w| w == b"/Standard"));
    }

    #[test]
    fn encryption_is_deterministic() {
        let enc1 = encrypt_minimal("password", "owner");
        let enc2 = encrypt_minimal("password", "owner");
        assert_eq!(enc1, enc2);
    }

    #[test]
    fn encryption_changes_content() {
        let plain = crate::LpdfEngine::render_xml_to_pdf_bytes(minimal_xml())
            .expect("render failed");
        let enc = encrypt_minimal("password", "owner");
        assert_ne!(plain, enc);
    }

    #[test]
    fn permissions_only_no_user_password() {
        let plain = crate::LpdfEngine::render_xml_to_pdf_bytes(minimal_xml())
            .expect("render failed");
        let cfg = EncryptConfig {
            user_password:  "".to_string(),
            owner_password: "s3cr3t".to_string(),
            permissions: Permissions {
                print: false, copy: false, modify: false, ..Default::default()
            },
        };
        let enc = encrypt_pdf(&plain, &cfg).expect("encrypt_pdf failed");
        assert_eq!(&enc[..5], b"%PDF-");
        assert!(enc.windows(8).any(|w| w == b"/Encrypt"));
        // P integer must be present in the /Encrypt dict
        assert!(enc.windows(2).any(|w| w == b"/P"));
    }

    #[test]
    fn showcase_encryption_with_password() {
        let xml = std::fs::read_to_string(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../test/fixtures/showcase-encryption.xml")
        ).expect("fixture not found");
        let plain = crate::LpdfEngine::render_xml_to_pdf_bytes(&xml)
            .expect("render failed");
        let cfg = EncryptConfig {
            user_password:  "password".to_string(),
            owner_password: "password".to_string(),
            permissions: Permissions {
                copy: false, modify: false, ..Default::default()
            },
        };
        let enc = encrypt_pdf(&plain, &cfg).expect("encrypt_pdf failed");
        assert_eq!(&enc[..5], b"%PDF-");
        assert!(enc.windows(8).any(|w| w == b"/Encrypt"));
        // Determinism: encrypting the same input twice gives the same output
        let enc2 = encrypt_pdf(&plain, &cfg).expect("encrypt_pdf failed");
        assert_eq!(enc, enc2);
    }
}
