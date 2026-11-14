//! The entities an SVG declares, and the bound on what a file makes of them (`svg.rs`).
//!
//! `roxmltree` stops an entity that nests past 10 levels or names more than 255 others, but not what a file builds
//! from many references: 100 uses of an entity of 255 uses of one of 1,000 characters is 25 MB in an attribute, from a
//! file of two kilobytes. So the entities are read first, and the file is refused if the ones it refers to expand to
//! more than `MAX_ENTITY_BYTES` in all.
//!
//! A check that reads the DOCTYPE differently from the XML reader can be walked round: a declaration the check takes
//! for quoted text, or a comment it takes for a DOCTYPE, hides entities the reader then expands. So `read_prolog`
//! follows `roxmltree`'s tokenizer step for step, where a step is a decision of `parse` in its `tokenizer.rs`, and
//! reads nothing it doesn't: what it can't read is an error, not "no entities".

use std::collections::HashMap;

/// The most bytes the entities a file refers to may expand to in all, each reference counted.
pub(crate) const MAX_ENTITY_BYTES: u64 = 1_000_000;

/// How deep one entity may name another.
const MAX_ENTITY_DEPTH: usize = 10;

/// What XML counts as white space, and nothing else: `char::is_whitespace` also takes U+00A0 and others, which the XML
/// reader refuses where we would go on.
const XML_SPACE: [char; 4] = [' ', '\t', '\r', '\n'];

/// The entities of a DOCTYPE, by name with their values as written.
type Entities<'a> = HashMap<&'a str, &'a str>;

/// Refuse a file whose entities expand past `MAX_ENTITY_BYTES` once it refers to them, an entity that names a file,
/// and a parameter entity. `text` is the file without its byte order mark.
pub(crate) fn check_entities(text: &str) -> Result<(), String> {
    check_within(text, MAX_ENTITY_BYTES)
}

/// `check_entities` with the limit given, which a test lowers.
pub(crate) fn check_within(text: &str, limit: u64) -> Result<(), String> {
    let Some((entities, body)) = read_prolog(text)? else { return Ok(()) };
    let mut expander = Expander { entities: &entities, known: HashMap::new(), limit };
    let mut total = 0u64;
    for name in entity_names(body) {
        if entities.contains_key(name) {
            total = total.saturating_add(expander.bytes(name, 1)?);
            if total > limit {
                return Err(too_many_bytes(limit));
            }
        }
    }
    Ok(())
}

fn too_many_bytes(limit: u64) -> String {
    format!("the SVG's entities expand to more than {limit} bytes: the limit is {limit}")
}

/// A place in a text, which reads it as `roxmltree`'s `Stream` does.
struct Reader<'a> {
    text: &'a str,
    at: usize,
}

impl<'a> Reader<'a> {
    fn rest(&self) -> &'a str {
        &self.text[self.at..]
    }

    fn starts_with(&self, prefix: &str) -> bool {
        self.rest().starts_with(prefix)
    }

    fn next_char(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn skip(&mut self, bytes: usize) {
        self.at += bytes;
    }

    fn skip_spaces(&mut self) {
        self.at = self.text.len() - self.rest().trim_start_matches(XML_SPACE).len();
    }

    /// Past the white space that has to be there.
    fn consume_spaces(&mut self) -> Result<(), String> {
        let before = self.at;
        self.skip_spaces();
        if self.at == before { Err(self.unreadable()) } else { Ok(()) }
    }

    /// Just past the first `end` ahead.
    fn skip_past(&mut self, end: &str) -> Result<(), String> {
        let found = self.rest().find(end).ok_or_else(|| self.unreadable())?;
        self.skip(found + end.len());
        Ok(())
    }

    /// The text quoted from here, which is at a quote, without its quotes. A quoted text holds anything but its own
    /// quote.
    fn quoted(&mut self) -> Result<&'a str, String> {
        let quote = self.next_char().filter(|c| matches!(c, '"' | '\'')).ok_or_else(|| self.unreadable())?;
        let inside = &self.rest()[1..];
        let close = inside.find(quote).ok_or_else(|| self.unreadable())?;
        self.skip(close + 2);
        Ok(&inside[..close])
    }

    fn unreadable(&self) -> String {
        let line = self.text[..self.at].matches('\n').count() + 1;
        format!("the SVG could not be read: its prolog is not well formed, on line {line}")
    }
}

/// The entities the DOCTYPE of `text` declares, and the text after the DOCTYPE, which is where they are used. `None`
/// when the file has no DOCTYPE with an internal subset.
fn read_prolog(text: &str) -> Result<Option<(Entities<'_>, &str)>, String> {
    let mut reader = Reader { text: text.strip_prefix('\u{feff}').unwrap_or(text), at: 0 };
    skip_xml_declaration(&mut reader)?;
    skip_comments_and_instructions(&mut reader)?;
    if !reader.starts_with("<!DOCTYPE") || !skip_doctype_head(&mut reader)? {
        return Ok(None);
    }
    let entities = read_subset(&mut reader)?;
    Ok(Some((entities, reader.rest())))
}

/// The XML declaration, which only a file's very first bytes can be. Its values are quoted, and may hold a `?>`.
fn skip_xml_declaration(reader: &mut Reader) -> Result<(), String> {
    if !reader.starts_with("<?xml ") {
        return Ok(());
    }
    reader.skip("<?xml".len());
    loop {
        match reader.next_char() {
            Some('"' | '\'') => {
                reader.quoted()?;
            }
            Some('?') if reader.starts_with("?>") => {
                reader.skip(2);
                return Ok(());
            }
            Some(other) => reader.skip(other.len_utf8()),
            None => return Err(reader.unreadable()),
        }
    }
}

/// White space, comments and processing instructions, up to what is none of them.
fn skip_comments_and_instructions(reader: &mut Reader) -> Result<(), String> {
    loop {
        reader.skip_spaces();
        if reader.starts_with("<!--") {
            reader.skip("<!--".len());
            reader.skip_past("-->")?;
        } else if reader.starts_with("<?") {
            reader.skip("<?".len());
            reader.skip_past("?>")?;
        } else {
            return Ok(());
        }
    }
}

/// From `<!DOCTYPE` to the `[` that opens its internal subset, with the name and the external id between, whose
/// quoted text may hold a `[` or a `>`. `false` when the `>` comes first: there is no subset.
fn skip_doctype_head(reader: &mut Reader) -> Result<bool, String> {
    reader.skip("<!DOCTYPE".len());
    loop {
        match reader.next_char() {
            Some('"' | '\'') => {
                reader.quoted()?;
            }
            Some('[') => {
                reader.skip(1);
                return Ok(true);
            }
            Some('>') => return Ok(false),
            Some(other) => reader.skip(other.len_utf8()),
            None => return Err(reader.unreadable()),
        }
    }
}

/// The declarations of the internal subset, up to its closing `]>`. Of what the tokenizer allows there, only an
/// `<!ENTITY` is read; a comment and an instruction are skipped, and so is a declaration of an element, an attribute
/// list or a notation, which ends at its first `>` whatever quotes it holds. Anything else is an error, as it is for
/// the tokenizer.
fn read_subset<'a>(reader: &mut Reader<'a>) -> Result<Entities<'a>, String> {
    let mut entities = Entities::new();
    loop {
        reader.skip_spaces();
        if reader.starts_with("<!ENTITY") {
            let (name, value) = read_entity(reader)?;
            // The first declaration of a name is the one in force.
            entities.entry(name).or_insert(value);
        } else if reader.starts_with("<!--") {
            reader.skip("<!--".len());
            reader.skip_past("-->")?;
        } else if reader.starts_with("<?") {
            reader.skip("<?".len());
            reader.skip_past("?>")?;
        } else if reader.starts_with("]") {
            reader.skip(1);
            reader.skip_spaces();
            if !reader.starts_with(">") {
                return Err(reader.unreadable());
            }
            reader.skip(1);
            return Ok(entities);
        } else if ["<!ELEMENT", "<!ATTLIST", "<!NOTATION"].iter().any(|declaration| reader.starts_with(declaration)) {
            reader.skip_past(">")?;
        } else if reader.starts_with("%") {
            return Err("the SVG refers to a parameter entity, which can't be read".to_string());
        } else {
            return Err(reader.unreadable());
        }
    }
}

/// The name and the value of the entity declared at `<!ENTITY`.
fn read_entity<'a>(reader: &mut Reader<'a>) -> Result<(&'a str, &'a str), String> {
    reader.skip("<!ENTITY".len());
    reader.consume_spaces()?;
    if reader.starts_with("%") {
        return Err("the SVG declares a parameter entity, which can't be read".to_string());
    }
    // A name has no white space, and the white space after it is the one thing the tokenizer wants there.
    let rest = reader.rest();
    let name = &rest[..rest.find(XML_SPACE).unwrap_or(rest.len())];
    reader.skip(name.len());
    reader.consume_spaces()?;
    if reader.starts_with("SYSTEM") || reader.starts_with("PUBLIC") {
        return Err(format!("the SVG's entity {name} refers to another file, which an SVG may not"));
    }
    let value = reader.quoted()?;
    reader.skip_spaces();
    if !reader.starts_with(">") {
        return Err(reader.unreadable());
    }
    reader.skip(1);
    Ok((name, value))
}

/// The names between `&` and `;` in `text`.
fn entity_names(text: &str) -> impl Iterator<Item = &str> {
    text.split('&').skip(1).filter_map(|part| part.split_once(';').map(|(name, _)| name))
}

/// What the entities are worth once the ones they name are expanded, which each is counted once.
struct Expander<'a, 'e> {
    entities: &'e Entities<'a>,
    known: HashMap<&'a str, u64>,
    limit: u64,
}

impl<'a> Expander<'a, '_> {
    /// The bytes the entity `name` is once the entities it names are expanded.
    fn bytes(&mut self, name: &'a str, depth: usize) -> Result<u64, String> {
        if let Some(bytes) = self.known.get(name) {
            return Ok(*bytes);
        }
        if depth > MAX_ENTITY_DEPTH {
            return Err(format!("the SVG's entities name one another more than {MAX_ENTITY_DEPTH} levels deep"));
        }
        let value = self.entities[name];
        let mut bytes = value.len() as u64;
        for inner in entity_names(value) {
            if self.entities.contains_key(inner) {
                bytes = bytes.saturating_add(self.bytes(inner, depth + 1)?);
                if bytes > self.limit {
                    return Err(too_many_bytes(self.limit));
                }
            }
        }
        self.known.insert(name, bytes);
        Ok(bytes)
    }
}
