//! PDF encryption with permission controls (ISO 32000 §7.6): AES-256 (V=5 R=6) and
//! RC4-128 (V=2 R=3).
//!
//! Applied as a post-processing pass on the raw bytes returned by pdf-writer.
//!
//! Both ciphers rewrite the file: `rewrite.rs` copies the objects into a new buffer,
//! each string read with its escapes resolved and written as a hex string of its
//! encrypted bytes, and each stream's data encrypted, and this pass appends the
//! /Encrypt object and writes a fresh xref and trailer around them. AES-CBC adds an IV
//! and padding to every string and stream; RC4 keeps the length of each, with a key for
//! each object, but a literal string still becomes a hex string, so the offsets move
//! for both. AES-256's random values come from the host's entropy (`aes256.rs`), so one
//! entropy gives one file; RC4 needs none, so the same input gives the same file.
//!
//! # Compliance note
//! RC4-128 (V=2 R=3) is the weakest encryption scheme defined in the PDF specification
//! and is deprecated by PDF 2.0. It stays as an option for readers that need it. For
//! environments with stricter security requirements (e.g. PCI-DSS, HIPAA, or ISO 27001
//! compliance), use AES-256.

use md5::{Digest, Md5};

use super::{aes256, rewrite};

// ISO 32000-1 §7.6.3.3 — standard password padding string
const PADDING: [u8; 32] = [
    0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41,
    0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA, 0x01, 0x08,
    0x2E, 0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80,
    0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53, 0x69, 0x7A,
];

// ── Public types ───────────────────────────────────────────────────────────────

/// What a reader may do without the owner password, as the `/P` integer's flags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

/// The words of `allow` on `<output>`, in the schema's order.
const ALLOW_WORDS: [&str; 7] = ["print", "modify", "copy", "annotate", "fill-forms", "accessibility", "assemble"];

impl Permissions {
    /// Reads `allow`: the words of what a reader may do, separated by spaces. What it doesn't name is
    /// denied, so an empty list allows nothing. `print` allows printing at full quality, both print flags.
    pub fn from_allow(list: &str) -> Result<Self, String> {
        let mut allowed = Self {
            print: false, modify: false, copy: false, annotate: false,
            fill_forms: false, accessibility: false, assemble: false, print_hq: false,
        };
        for word in list.split_whitespace() {
            match word {
                "print" => {
                    allowed.print = true;
                    allowed.print_hq = true;
                }
                "modify"        => allowed.modify = true,
                "copy"          => allowed.copy = true,
                "annotate"      => allowed.annotate = true,
                "fill-forms"    => allowed.fill_forms = true,
                "accessibility" => allowed.accessibility = true,
                "assemble"      => allowed.assemble = true,
                other => return Err(format!(
                    "unknown permission \"{other}\" in allow; expected {}",
                    ALLOW_WORDS.join(", "),
                )),
            }
        }
        Ok(allowed)
    }
}

/// The cipher a file is encrypted with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Algorithm {
    /// AES-256-CBC (V=5 R=6). Needs entropy from the host.
    Aes256,
    /// RC4-128 (V=2 R=3). Deprecated by PDF 2.0.
    Rc4128,
}

impl Algorithm {
    /// Reads the cipher `encrypt` names on `<output>`: `aes-256` or `rc4-128`.
    pub fn parse(name: &str) -> Result<Self, String> {
        match name {
            "aes-256" => Ok(Self::Aes256),
            "rc4-128" => Ok(Self::Rc4128),
            other => Err(format!("unknown cipher \"{other}\" in encrypt; expected aes-256 or rc4-128")),
        }
    }
}

pub struct EncryptConfig {
    pub user_password:  String,
    pub owner_password: String,
    pub permissions:    Permissions,
    pub algorithm:      Algorithm,
    /// Random bytes from the host's secure generator: the AES-256 file key, salts, IVs
    /// and file ID are all cut from them. Unused by RC4.
    pub entropy:        Vec<u8>,
}

impl EncryptConfig {
    /// Fails when the algorithm needs more than the config holds, so a bad request is
    /// refused before the document is laid out and not after.
    pub fn validate(&self) -> Result<(), String> {
        match self.algorithm {
            Algorithm::Aes256 => aes256::EntropyStream::new(&self.entropy).map(|_| ()),
            Algorithm::Rc4128 => Ok(()),
        }
    }
}

/// The encryption a document asks for in its `<output>`: the cipher of `encrypt` and the
/// permissions of `allow`. The passwords are not part of it: they come with the render call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutputEncryption {
    pub algorithm:   Algorithm,
    pub permissions: Permissions,
}

/// What a render call says about encryption: the passwords and the host's entropy, or that the
/// render skips the encryption the document asks for, as a preview that has no passwords does. It has no `Debug`,
/// so that the passwords never end up in a log.
#[derive(Clone, Default)]
pub struct RenderEncryption {
    pub user_password:   Option<String>,
    pub owner_password:  Option<String>,
    pub skip_encryption: bool,
    /// Random bytes from the host's secure generator, 32 or more for AES-256.
    pub entropy:         Vec<u8>,
}

impl RenderEncryption {
    /// A render that skips the encryption the document asks for, or asks for none when `skip` is false.
    // The WASI binary reads skip_encryption from the request, so this is for the engine, the command line and the tests.
    #[allow(dead_code)]
    pub fn skipping(skip: bool) -> Self {
        Self { skip_encryption: skip, ..Self::default() }
    }

    /// The config to encrypt the document with, or `None` when the file goes out unencrypted. The template is
    /// the only place that turns encryption on, and only the caller has the passwords, so each error says
    /// which of the two to change: a document that asks for encryption needs an owner password or
    /// `skip_encryption`, and passwords for a document that doesn't are refused, so that a file never goes out
    /// unencrypted while the caller believes it is locked.
    pub fn resolve(&self, document: Option<&OutputEncryption>) -> Result<Option<EncryptConfig>, String> {
        let has_passwords = self.user_password.is_some() || self.owner_password.is_some();
        if has_passwords && self.skip_encryption {
            return Err("the render got passwords and skip_encryption together: pass the passwords to encrypt the \
                file, or skip_encryption to render it unencrypted, not both".to_string());
        }
        let Some(document) = document else {
            if has_passwords {
                return Err("the render got passwords, but the document does not ask for encryption: add encrypt to \
                    its <output> (<output encrypt=\"aes-256\"/>), or render it without passwords".to_string());
            }
            return Ok(None);
        };
        if self.skip_encryption {
            return Ok(None);
        }
        let owner_password = match self.owner_password.as_deref() {
            Some(password) if !password.is_empty() => password,
            _ => return Err("the document asks for encryption in its <output>, and the render got no owner password: \
                pass the passwords with the render, or skip_encryption to render it unencrypted".to_string()),
        };
        let config = EncryptConfig {
            user_password:  self.user_password.clone().unwrap_or_default(),
            owner_password: owner_password.to_string(),
            permissions:    document.permissions,
            algorithm:      document.algorithm,
            entropy:        self.entropy.clone(),
        };
        config.validate()?;
        Ok(Some(config))
    }
}

// ── Internal types ─────────────────────────────────────────────────────────────

pub(crate) struct ObjectEntry {
    pub(crate) obj_num: u32,
    pub(crate) gen_num: u32,
    pub(crate) offset:  usize,
}

struct XrefInfo {
    entries:  Vec<ObjectEntry>,
    root_ref: (u32, u32),
    info_ref: Option<(u32, u32)>,
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
pub(crate) fn compute_p(perms: &Permissions) -> i32 {
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

fn rfind_bytes(data: &[u8], needle: &[u8]) -> Option<usize> {
    let n = needle.len();
    if data.len() < n { return None; }
    for i in (0..=data.len() - n).rev() {
        if &data[i..i + n] == needle { return Some(i); }
    }
    None
}

pub(crate) fn skip_ws(data: &[u8]) -> &[u8] {
    let n = data.iter()
        .take_while(|&&b| b == b' ' || b == b'\t' || b == b'\r' || b == b'\n')
        .count();
    &data[n..]
}

pub(crate) fn read_u32(data: &[u8]) -> Option<(u32, &[u8])> {
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

pub(crate) fn hex_nibble(b: u8) -> u8 {
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

    Ok(XrefInfo { entries, root_ref, info_ref })
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
    entries: &[ObjectEntry],
    encrypt_obj_num: u32,
    encrypt_obj_offset: usize,
    xref_info: &XrefInfo,
    file_id: &[u8; 16],
) {
    let new_xref_offset = out.len();

    let mut all: Vec<(u32, u32, usize)> = entries.iter()
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

fn append_aes256_encrypt_dict(out: &mut Vec<u8>, obj_num: u32, entries: &aes256::KeyEntries, p: i32) {
    let s = format!(
        "{obj_num} 0 obj\n<<\n/Filter /Standard\n/V 5\n/R 6\n/Length 256\n/P {p}\n\
         /CF << /StdCF << /AuthEvent /DocOpen /CFM /AESV3 /Length 32 >> >>\n\
         /StmF /StdCF\n/StrF /StdCF\n\
         /O <{}>\n/U <{}>\n/OE <{}>\n/UE <{}>\n/Perms <{}>\n>>\nendobj\n",
        to_hex(&entries.o), to_hex(&entries.u), to_hex(&entries.oe), to_hex(&entries.ue), to_hex(&entries.perms)
    );
    out.extend_from_slice(s.as_bytes());
}

// ── Public entry point ─────────────────────────────────────────────────────────

/// Post-process a pdf-writer–generated PDF to add encryption with the config's
/// algorithm: AES-256 (V=5 R=6) or RC4-128 (V=2 R=3).
///
/// Returns `Err` if the config is not usable (AES-256 without enough entropy) or the
/// PDF structure is malformed (e.g. missing or corrupt xref).
pub fn encrypt_pdf(bytes: &[u8], config: &EncryptConfig) -> Result<Vec<u8>, String> {
    match config.algorithm {
        Algorithm::Aes256 => encrypt_pdf_aes256(bytes, config),
        Algorithm::Rc4128 => encrypt_pdf_rc4(bytes, config),
    }
}

fn encrypt_pdf_aes256(bytes: &[u8], config: &EncryptConfig) -> Result<Vec<u8>, String> {
    let xref = parse_xref(bytes)?;
    let p = compute_p(&config.permissions);
    let aes256::Aes256Setup { entries, file_id, mut cipher } =
        aes256::setup(&config.user_password, &config.owner_password, p, &config.entropy)?;

    let rewrite::Rewritten { bytes: mut out, entries: moved } =
        rewrite::rewrite_objects(bytes, &xref.entries, &mut |_, _, plain| cipher.seal(plain))?;

    // The /Encrypt dictionary is stored in plaintext, so it is appended after the rewrite.
    let encrypt_obj_num    = moved.iter().map(|e| e.obj_num).max().unwrap_or(0) + 1;
    let encrypt_obj_offset = out.len();
    append_aes256_encrypt_dict(&mut out, encrypt_obj_num, &entries, p);
    write_xref_and_trailer(&mut out, &moved, encrypt_obj_num, encrypt_obj_offset, &xref, &file_id);
    Ok(out)
}

fn encrypt_pdf_rc4(bytes: &[u8], config: &EncryptConfig) -> Result<Vec<u8>, String> {
    let xref = parse_xref(bytes)?;

    // File ID: MD5 of the unencrypted bytes — deterministic, no random salt needed
    let file_id = md5_hash(bytes);
    let p       = compute_p(&config.permissions);
    let o_entry = derive_o_entry(&config.user_password, &config.owner_password);
    let fek     = derive_fek(&config.user_password, &o_entry, p, &file_id);
    let u_entry = derive_u_entry(&fek, &file_id);

    // Each string and stream is encrypted with the key of the object it is in.
    let rewrite::Rewritten { bytes: mut out, entries: moved } =
        rewrite::rewrite_objects(bytes, &xref.entries, &mut |obj_num, gen_num, plain| {
            let mut sealed = plain.to_vec();
            rc4(&derive_obj_key(&fek, obj_num, gen_num), &mut sealed);
            sealed
        })?;

    // The /Encrypt dictionary is stored in plaintext, so it is appended after the rewrite.
    let encrypt_obj_num    = moved.iter().map(|e| e.obj_num).max().unwrap_or(0) + 1;
    let encrypt_obj_offset = out.len();
    append_encrypt_dict(&mut out, encrypt_obj_num, &o_entry, &u_entry, p);
    write_xref_and_trailer(&mut out, &moved, encrypt_obj_num, encrypt_obj_offset, &xref, &file_id);
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
        r#"<lpdf version="1"><document size="a4"><section><layout margin="28pt"><text>Hello</text></layout></section></document></lpdf>"#
    }

    fn config(algorithm: Algorithm, user_pw: &str, owner_pw: &str) -> EncryptConfig {
        EncryptConfig {
            user_password:  user_pw.to_string(),
            owner_password: owner_pw.to_string(),
            permissions:    Permissions::default(),
            algorithm,
            entropy:        (0u8..48).collect(),
        }
    }

    fn render_minimal() -> Vec<u8> {
        crate::LpdfEngine::render_xml_to_pdf_bytes(minimal_xml()).expect("render failed")
    }

    fn encrypt_minimal(user_pw: &str, owner_pw: &str) -> Vec<u8> {
        encrypt_pdf(&render_minimal(), &config(Algorithm::Rc4128, user_pw, owner_pw)).expect("encrypt_pdf failed")
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
    fn rc4_encryption_is_deterministic() {
        let enc1 = encrypt_minimal("password", "owner");
        let enc2 = encrypt_minimal("password", "owner");
        assert_eq!(enc1, enc2);
    }

    #[test]
    fn encryption_changes_content() {
        let plain = render_minimal();
        let enc = encrypt_minimal("password", "owner");
        assert_ne!(plain, enc);
    }

    #[test]
    fn permissions_only_no_user_password() {
        let plain = render_minimal();
        let cfg = EncryptConfig {
            permissions: Permissions { print: false, copy: false, modify: false, ..Default::default() },
            ..config(Algorithm::Rc4128, "", "s3cr3t")
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
            permissions: Permissions { copy: false, modify: false, ..Default::default() },
            ..config(Algorithm::Rc4128, "password", "password")
        };
        let enc = encrypt_pdf(&plain, &cfg).expect("encrypt_pdf failed");
        assert_eq!(&enc[..5], b"%PDF-");
        assert!(enc.windows(8).any(|w| w == b"/Encrypt"));
        // Determinism: encrypting the same input twice gives the same output
        let enc2 = encrypt_pdf(&plain, &cfg).expect("encrypt_pdf failed");
        assert_eq!(enc, enc2);
    }

    /// The bytes of the hex string after `key` in the object `number` of `pdf`, found through its xref.
    fn hex_string_in_object(pdf: &[u8], number: u32, key: &str) -> Vec<u8> {
        let entry = parse_xref(pdf).unwrap().entries.into_iter().find(|e| e.obj_num == number).expect("the object");
        let object = String::from_utf8_lossy(&pdf[entry.offset..]).split("endobj").next().unwrap().to_string();
        let hex = object.split(&format!("{key} <")).nth(1).unwrap_or_else(|| panic!("no hex {key} in {object}"));
        let hex = hex.split('>').next().unwrap().as_bytes();
        hex.chunks(2).map(|pair| (hex_nibble(pair[0]) << 4) | hex_nibble(pair[1])).collect()
    }

    #[test]
    fn rc4_writes_each_string_as_hex_of_its_value_encrypted_with_the_key_of_its_object() {
        let xml = r#"<lpdf version="1"><document size="a4"><meta title="C:\reports\Q3 (draft"/><section><layout margin="28pt"><text>Hello</text></layout></section></document></lpdf>"#;
        let plain = crate::LpdfEngine::render_xml_to_pdf_bytes(xml).expect("render failed");
        assert!(String::from_utf8_lossy(&plain).contains(r"/Title (C:\\reports\\Q3 \(draft)"), "pdf-writer escapes the title");
        let cfg = config(Algorithm::Rc4128, "user", "owner");
        let enc = encrypt_pdf(&plain, &cfg).expect("encrypt_pdf failed");

        let (info, generation) = parse_xref(&enc).unwrap().info_ref.expect("an /Info");
        let mut title = hex_string_in_object(&enc, info, "/Title");
        let file_id = md5_hash(&plain);
        let p = compute_p(&cfg.permissions);
        let fek = derive_fek("user", &derive_o_entry("user", "owner"), p, &file_id);
        rc4(&derive_obj_key(&fek, info, generation), &mut title);
        assert_eq!(title, br"C:\reports\Q3 (draft");
    }

    /// The values of `/Length` in `pdf`, in the order they are written: not `/Length1` and the like.
    fn lengths(pdf: &[u8]) -> Vec<String> {
        let text = String::from_utf8_lossy(pdf);
        text.split("/Length ").skip(1).map(|rest| rest.chars().take_while(char::is_ascii_digit).collect()).collect()
    }

    #[test]
    fn rc4_keeps_the_length_of_each_stream() {
        let plain = render_minimal();
        let mut expected = lengths(&plain);
        assert!(!expected.is_empty(), "the render has streams");
        expected.push("128".to_string()); // the key length in the /Encrypt dictionary, written last
        assert_eq!(lengths(&encrypt_minimal("user", "owner")), expected);
    }

    #[test]
    fn the_xref_points_at_every_object_for_each_cipher() {
        for algorithm in [Algorithm::Aes256, Algorithm::Rc4128] {
            let enc = encrypt_pdf(&render_minimal(), &config(algorithm, "", "owner")).expect("encrypt_pdf failed");
            let xref = parse_xref(&enc).expect("the new xref parses");
            assert!(!xref.entries.is_empty());
            for entry in &xref.entries {
                let header = format!("{} {} obj", entry.obj_num, entry.gen_num);
                assert!(enc[entry.offset..].starts_with(header.as_bytes()), "{algorithm:?}: object {} is not at {}", entry.obj_num, entry.offset);
            }
            assert!(enc.windows(8).any(|w| w == b"/Encrypt"));
            assert!(enc.windows(3).any(|w| w == b"/ID"));
        }
    }

    // ── AES-256 ────────────────────────────────────────────────────────────────

    fn encrypt_minimal_aes(user_pw: &str, owner_pw: &str) -> Vec<u8> {
        encrypt_pdf(&render_minimal(), &config(Algorithm::Aes256, user_pw, owner_pw)).expect("encrypt_pdf failed")
    }

    #[test]
    fn aes256_encrypts_with_the_v5_r6_dictionary() {
        let enc = String::from_utf8_lossy(&encrypt_minimal_aes("", "owner")).into_owned();
        for expected in ["/Filter /Standard", "/V 5", "/R 6", "/Length 256", "/CFM /AESV3", "/StmF /StdCF", "/StrF /StdCF"] {
            assert!(enc.contains(expected), "missing {expected}");
        }
        for key in ["/O <", "/U <", "/OE <", "/UE <", "/Perms <"] {
            assert!(enc.contains(key), "missing {key}");
        }
    }

    #[test]
    fn aes256_with_the_same_entropy_is_deterministic() {
        assert_eq!(encrypt_minimal_aes("password", "owner"), encrypt_minimal_aes("password", "owner"));
    }

    #[test]
    fn aes256_with_other_entropy_gives_another_file() {
        let plain = render_minimal();
        let other = EncryptConfig { entropy: (100u8..148).collect(), ..config(Algorithm::Aes256, "password", "owner") };
        let first  = encrypt_pdf(&plain, &config(Algorithm::Aes256, "password", "owner")).unwrap();
        let second = encrypt_pdf(&plain, &other).unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn aes256_without_enough_entropy_is_refused() {
        let cfg = EncryptConfig { entropy: vec![1; 16], ..config(Algorithm::Aes256, "", "owner") };
        let error = encrypt_pdf(&render_minimal(), &cfg).unwrap_err();
        assert!(error.contains("at least 32 bytes"), "{error}");
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn rc4_needs_no_entropy() {
        let cfg = EncryptConfig { entropy: Vec::new(), ..config(Algorithm::Rc4128, "", "owner") };
        assert!(cfg.validate().is_ok());
        assert!(encrypt_pdf(&render_minimal(), &cfg).is_ok());
    }

    #[test]
    fn algorithm_names_are_the_ones_the_schema_has() {
        assert_eq!(Algorithm::parse("aes-256"), Ok(Algorithm::Aes256));
        assert_eq!(Algorithm::parse("rc4-128"), Ok(Algorithm::Rc4128));
        assert!(Algorithm::parse("aes-128").unwrap_err().contains("aes-128"));
    }

    // ── allow ──────────────────────────────────────────────────────────────────

    #[test]
    fn allow_allows_what_it_names_and_denies_the_rest() {
        let allowed = Permissions::from_allow("copy  fill-forms\taccessibility").unwrap();
        assert_eq!(allowed, Permissions {
            print: false, modify: false, copy: true, annotate: false,
            fill_forms: true, accessibility: true, assemble: false, print_hq: false,
        });
    }

    #[test]
    fn print_allows_printing_at_full_quality() {
        let p = compute_p(&Permissions::from_allow("print").unwrap()) as u32;
        assert_ne!(p & (1 << 2), 0, "print");
        assert_ne!(p & (1 << 11), 0, "print at full quality");
    }

    #[test]
    fn an_empty_allow_allows_nothing() {
        let p = compute_p(&Permissions::from_allow("").unwrap()) as u32;
        assert_eq!(p, 0xFFFF_FFFC & !(1 << 2) & !(1 << 3) & !(1 << 4) & !(1 << 5) & !(1 << 8) & !(1 << 9) & !(1 << 10) & !(1 << 11));
    }

    #[test]
    fn every_word_of_allow_but_print_is_one_flag() {
        for (word, bit) in [("modify", 3), ("copy", 4), ("annotate", 5), ("fill-forms", 8), ("accessibility", 9), ("assemble", 10)] {
            let p = compute_p(&Permissions::from_allow(word).unwrap()) as u32;
            let nothing = compute_p(&Permissions::from_allow("").unwrap()) as u32;
            assert_eq!(p, nothing | (1 << bit), "{word}");
        }
    }

    #[test]
    fn an_unknown_word_in_allow_is_named_with_the_words_there_are() {
        let error = Permissions::from_allow("print printing").unwrap_err();
        assert!(error.contains("\"printing\""), "{error}");
        assert!(error.contains("fill-forms"), "{error}");
    }

    // ── The document's encryption and the render call's ────────────────────────

    fn asks_for(algorithm: Algorithm) -> OutputEncryption {
        OutputEncryption { algorithm, permissions: Permissions::from_allow("print").unwrap() }
    }

    /// The error of a render the rules refuse. `EncryptConfig` holds the passwords and has no `Debug`, so the
    /// result can't be unwrapped.
    fn refused(result: Result<Option<EncryptConfig>, String>) -> String {
        match result {
            Err(error) => error,
            Ok(_) => panic!("the render was not refused"),
        }
    }

    fn call(user: Option<&str>, owner: Option<&str>, skip: bool) -> RenderEncryption {
        RenderEncryption {
            user_password:   user.map(str::to_string),
            owner_password:  owner.map(str::to_string),
            skip_encryption: skip,
            entropy:         (0u8..48).collect(),
        }
    }

    #[test]
    fn the_config_takes_the_cipher_and_permissions_of_the_document_and_the_passwords_of_the_render() {
        let config = call(Some("open"), Some("owner"), false).resolve(Some(&asks_for(Algorithm::Rc4128))).unwrap().unwrap();
        assert_eq!(config.algorithm, Algorithm::Rc4128);
        assert_eq!(config.permissions, Permissions::from_allow("print").unwrap());
        assert_eq!((config.user_password.as_str(), config.owner_password.as_str()), ("open", "owner"));
    }

    #[test]
    fn without_an_open_password_anyone_can_open_the_file() {
        let config = call(None, Some("owner"), false).resolve(Some(&asks_for(Algorithm::Aes256))).unwrap().unwrap();
        assert_eq!(config.user_password, "");
    }

    #[test]
    fn a_document_that_asks_for_encryption_needs_an_owner_password() {
        for (user, owner) in [(None, None), (Some("open"), None), (None, Some(""))] {
            let error = refused(call(user, owner, false).resolve(Some(&asks_for(Algorithm::Aes256))));
            assert!(error.contains("no owner password") && error.contains("skip_encryption"), "{error}");
        }
    }

    #[test]
    fn passwords_for_a_document_that_does_not_ask_for_encryption_are_refused() {
        for (user, owner) in [(None, Some("owner")), (Some(""), None)] {
            let error = refused(call(user, owner, false).resolve(None));
            assert!(error.contains("does not ask for encryption") && error.contains("<output"), "{error}");
        }
    }

    #[test]
    fn passwords_with_skip_encryption_are_refused() {
        for document in [None, Some(asks_for(Algorithm::Aes256))] {
            let error = refused(call(None, Some("owner"), true).resolve(document.as_ref()));
            assert!(error.contains("not both"), "{error}");
        }
    }

    #[test]
    fn skip_encryption_leaves_the_file_unencrypted() {
        assert!(RenderEncryption::skipping(true).resolve(Some(&asks_for(Algorithm::Aes256))).unwrap().is_none());
        assert!(RenderEncryption::skipping(true).resolve(None).unwrap().is_none());
    }

    #[test]
    fn a_document_without_encrypt_needs_no_passwords() {
        assert!(RenderEncryption::default().resolve(None).unwrap().is_none());
    }

    #[test]
    fn aes256_without_enough_entropy_is_refused_before_the_render() {
        let short = RenderEncryption { entropy: vec![1; 16], ..call(None, Some("owner"), false) };
        let error = refused(short.resolve(Some(&asks_for(Algorithm::Aes256))));
        assert!(error.contains("at least 32 bytes"), "{error}");
        assert!(short.resolve(Some(&asks_for(Algorithm::Rc4128))).is_ok(), "RC4-128 needs no entropy");
    }
}
