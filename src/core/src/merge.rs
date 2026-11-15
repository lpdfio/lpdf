//! Merging: existing PDFs drawn into one PDF made for printing (`LpdfEngine::merge`).
//!
//! The engine writes PDFs and reads none, but here and in `stamp`: `lopdf` reads each file, and the merge writes a new
//! one. It never merges page trees. Each source page is imported: drawn on a fresh output page of the size the page
//! has as a viewer shows it (its crop box, turned by its `/Rotate`) as a form XObject, a drawing inside the PDF that
//! holds the page's content and resources. Then each annotation that prints is drawn after it from its stored
//! appearance, and none is written: what prints is what the page looked like, and forms, links, bookmarks, named
//! destinations, the structure tree and the metadata are never read.
//!
//! What a page draws is copied under new numbers, once for each source file, so what its pages share stays shared. The
//! source's layers (optional content groups) cross over, each on or off by default as it prints in its source.
//!
//! Without a valid license key each merged page gets the attribution line a render and a stamp draw, in its top-right
//! corner, with no link: the output holds no annotation. The blank sides `double-sided` adds stay empty.

use std::collections::{HashMap, HashSet};

use base64::Engine as _;
use lopdf::xref::XrefType;
use lopdf::{Dictionary, Document, Object, ObjectId, Permissions, Stream};
use miniz_oxide::deflate::compress_to_vec_zlib;

use crate::stamp::{contents_of, num, rect, Attribution, Geometry};

/// How many files one merge takes, at most.
pub(crate) const MAX_SOURCES: usize = 1_000;

/// How many bytes the content of one page may have once its streams are decoded, at most.
pub(crate) const MAX_CONTENT_BYTES: usize = 256 * 1024 * 1024;

/// How deep the dictionaries and arrays of an object may nest when it is copied, at most.
const MAX_NESTING: usize = 128;

/// An annotation prints when its Print flag (bit 3) is set and its Hidden flag (bit 2) is not.
const FLAG_HIDDEN: i64 = 2;
const FLAG_PRINT: i64 = 4;

/// A push button, among the button fields: it has no value to print.
const FIELD_PUSH_BUTTON: i64 = 1 << 16;

/// The options of a merge, by the names the SDKs send.
const OPTIONS: &[&str] = &["allow-signed", "allow-restricted", "double-sided"];

/// What a merge does beyond its defaults. Each is off by default.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Options {
    /// Merge a signed file, which loses its signature, rather than refuse it.
    pub allow_signed:     bool,
    /// Merge a file whose owner restricts printing or copying, rather than refuse it.
    pub allow_restricted: bool,
    /// Add a blank side after a document with an odd number of pages, so each document starts on a front side.
    pub double_sided:     bool,
}

/// One file to merge: its name, which every error about it names, and its bytes.
#[derive(Debug, Clone)]
pub(crate) struct Source {
    pub name: String,
    pub pdf:  Vec<u8>,
}

/// Read the options of a `merge` call: an object of flags, or nothing.
pub(crate) fn parse_options(value: &serde_json::Value) -> Result<Options, String> {
    let options = match value {
        serde_json::Value::Null      => return Ok(Options::default()),
        serde_json::Value::Object(o) => o,
        other => return Err(format!("the merge options must be an object, not {other}")),
    };
    if let Some(name) = options.keys().find(|k| !OPTIONS.contains(&k.as_str())) {
        return Err(format!("the merge has no option '{name}'"));
    }
    let flag = |key: &str| match options.get(key) {
        None | Some(serde_json::Value::Null) => Ok(false),
        Some(serde_json::Value::Bool(on))    => Ok(*on),
        Some(other) => Err(format!("the merge option {key} must be true or false, not {other}")),
    };
    Ok(Options {
        allow_signed:     flag("allow-signed")?,
        allow_restricted: flag("allow-restricted")?,
        double_sided:     flag("double-sided")?,
    })
}

/// The files of a `merge` call that sends their bytes one after the other, with a JSON list of their names and
/// lengths: `[{ "name": "a.pdf", "length": 1234 }, …]`.
#[allow(dead_code)] // the WASI build takes its files as JSON (`sources_from_request`)
pub(crate) fn sources_from_parts(pdfs: &[u8], index: &str) -> Result<Vec<Source>, String> {
    let index: serde_json::Value = serde_json::from_str(index).map_err(|e| format!("the merge's files could not be read: {e}"))?;
    let list = index.as_array().ok_or("the merge's files must be a list")?;
    let mut sources = Vec::with_capacity(list.len().min(MAX_SOURCES));
    let mut at = 0usize;
    for (i, item) in list.iter().enumerate() {
        let name = source_name(item, i)?;
        let length = item.get("length").and_then(serde_json::Value::as_u64)
            .ok_or_else(|| format!("file {} ({name}) has no length", i + 1))? as usize;
        let end = at.checked_add(length).filter(|end| *end <= pdfs.len())
            .ok_or_else(|| format!("file {} ({name}) is longer than the bytes sent", i + 1))?;
        sources.push(Source { name, pdf: pdfs[at..end].to_vec() });
        at = end;
    }
    Ok(sources)
}

/// The files of a `merge` request: a list of `{ "name": "a.pdf", "pdf": "<base64>" }`. The base64 is taken out of the
/// request as it is decoded, so a merge holds one copy of each file.
#[allow(dead_code)] // the WASM build takes its files as bytes (`sources_from_parts`), the WASI build as JSON
pub(crate) fn sources_from_request(value: &mut serde_json::Value) -> Result<Vec<Source>, String> {
    let list = value.as_array_mut().ok_or("request missing 'sources' list")?;
    let mut sources = Vec::with_capacity(list.len().min(MAX_SOURCES));
    for (i, item) in list.iter_mut().enumerate() {
        let name = source_name(item, i)?;
        let pdf = match item.get_mut("pdf").map(serde_json::Value::take) {
            Some(serde_json::Value::String(b64)) => base64::engine::general_purpose::STANDARD.decode(b64)
                .map_err(|e| format!("{name}: the PDF is not base64: {e}"))?,
            _ => return Err(format!("file {} ({name}) has no 'pdf': give it the file's bytes", i + 1)),
        };
        sources.push(Source { name, pdf });
    }
    Ok(sources)
}

fn source_name(item: &serde_json::Value, i: usize) -> Result<String, String> {
    match item.get("name").and_then(serde_json::Value::as_str) {
        Some(name) if !name.is_empty() => Ok(name.to_string()),
        _ => Err(format!("file {} has no name: give it the file's name, which every error about it names", i + 1)),
    }
}

/// Merge `sources` into one PDF made for printing: each page drawn as it looked, annotations flattened, layers
/// carried over. Unless `licensed`, each merged page gets the attribution line.
pub(crate) fn merge(sources: &[Source], options: &Options, licensed: bool) -> Result<Vec<u8>, String> {
    if sources.is_empty() {
        return Err("no PDFs were given: give merge at least one".to_string());
    }
    if sources.len() > MAX_SOURCES {
        return Err(format!("merge takes {MAX_SOURCES} PDFs at most, and {} were given", sources.len()));
    }
    let attribution = (!licensed).then(Attribution::new);
    let mut out = Output::new();
    for (i, source) in sources.iter().enumerate() {
        let imported = import(source, options, attribution.as_ref(), &mut out)
            .map_err(|e| format!("{}: {e}", source.name))?;
        // A blank side after a document that ends on a front side, so the next starts on one.
        if options.double_sided && i + 1 < sources.len() && imported.pages % 2 == 1 {
            out.blank_page(imported.last);
        }
        crate::memory::check()?;
    }
    out.finish()
}

// ── Reading a source ──────────────────────────────────────────────────────────

/// What a source gave the output.
struct Imported {
    pages: usize,
    /// The size of its last page as a viewer shows it.
    last:  (f32, f32),
}

/// Open a source, refusing what a merge must not carry over silently.
fn open(source: &Source, options: &Options) -> Result<Document, String> {
    if !source.pdf.windows(5).take(1024).any(|w| w == b"%PDF-") {
        return Err("the file is not a PDF: it doesn't start with %PDF-".to_string());
    }
    let doc = Document::load_mem(&source.pdf).map_err(|e| format!("the PDF could not be read: {e}"))?;
    // `lopdf` opens a file with an owner password only by itself; one that needs a password to open loads with its
    // `/Encrypt` still in place and no pages.
    if doc.is_encrypted() {
        return Err("the PDF needs a password to open, and a merge can't open it: remove the password first".to_string());
    }
    if !options.allow_restricted && restricts(&doc) {
        return Err("the owner restricts printing or copying: set the allow-restricted option to merge it anyway".to_string());
    }
    if !options.allow_signed && signed(&doc) {
        return Err("the file is signed, and a merge can't keep a signature, which covers the bytes of its own file: \
                    set the allow-signed option to merge it without the signature".to_string());
    }
    Ok(doc)
}

/// Whether the owner password of a file that opens without one restricts printing, printing in high quality or
/// copying.
fn restricts(doc: &Document) -> bool {
    let Some(state) = doc.encryption_state.as_ref() else { return false };
    let allowed = state.permissions();
    [Permissions::PRINTABLE, Permissions::PRINTABLE_IN_HIGH_QUALITY, Permissions::COPYABLE]
        .iter().any(|bit| !allowed.contains(*bit))
}

/// Whether the file holds a signature: a signature field with a value, or a certifying signature.
fn signed(doc: &Document) -> bool {
    let Ok(catalog) = doc.catalog() else { return false };
    let certified = catalog.get(b"Perms").ok().and_then(|p| doc.dereference(p).ok()?.1.as_dict().ok())
        .is_some_and(|perms| perms.has(b"DocMDP"));
    if certified {
        return true;
    }
    let Some(fields) = catalog.get(b"AcroForm").ok().and_then(|f| doc.dereference(f).ok()?.1.as_dict().ok())
        .and_then(|form| form.get(b"Fields").ok()) else { return false };
    // The fields, with the field type a node takes from its parents. A form that loops back on itself is read once.
    let mut stack: Vec<(&Object, bool)> = vec![(fields, false)];
    let mut seen: HashSet<ObjectId> = HashSet::new();
    while let Some((node, parent_is_signature)) = stack.pop() {
        match doc.dereference(node) {
            Ok((id, _)) if id.is_some_and(|id| !seen.insert(id)) => {}
            Ok((_, Object::Array(items))) => stack.extend(items.iter().map(|item| (item, parent_is_signature))),
            Ok((_, Object::Dictionary(field))) => {
                let signature = match field.get(b"FT").ok().and_then(|t| t.as_name().ok()) {
                    Some(kind) => kind == b"Sig",
                    None => parent_is_signature,
                };
                if signature && field.get(b"V").ok().and_then(|v| doc.dereference(v).ok()).is_some_and(|(_, v)| v.as_dict().is_ok()) {
                    return true;
                }
                if let Ok(kids) = field.get(b"Kids") {
                    stack.push((kids, signature));
                }
            }
            _ => {}
        }
    }
    false
}

/// The PDF version of a file as `(major, minor)`: the larger of its header's and the catalog's `/Version`.
fn version_of(doc: &Document) -> Option<(u32, u32)> {
    let read = |text: &str| {
        let (major, minor) = text.trim().split_once('.')?;
        Some((major.parse().ok()?, minor.parse().ok()?))
    };
    let header = read(&doc.version);
    let catalog = doc.catalog().ok().and_then(|c| c.get(b"Version").ok())
        .and_then(|v| doc.dereference(v).ok()?.1.as_name().ok())
        .and_then(|name| read(&String::from_utf8_lossy(name)));
    header.max(catalog)
}

/// Whether each layer of a file prints, by the object that names it. A layer is on or off by the default
/// configuration (`/D`), and the usage `/Print` of the layers its print `/AS` entry names says whether it prints.
fn read_layers(doc: &Document) -> HashMap<ObjectId, bool> {
    let mut layers = HashMap::new();
    let Some(properties) = doc.catalog().ok().and_then(|c| c.get(b"OCProperties").ok())
        .and_then(|p| doc.dereference(p).ok()?.1.as_dict().ok()) else { return layers };
    let ids = |value: Option<&Object>| -> Vec<ObjectId> {
        value.and_then(|v| doc.dereference(v).ok()?.1.as_array().ok())
            .map(|items| items.iter().filter_map(|item| item.as_reference().ok()).collect())
            .unwrap_or_default()
    };
    let config = properties.get(b"D").ok().and_then(|d| doc.dereference(d).ok()?.1.as_dict().ok());
    let base_on = config.and_then(|d| d.get(b"BaseState").ok()?.as_name().ok()) != Some(b"OFF".as_slice());
    let on = ids(config.and_then(|d| d.get(b"ON").ok()));
    let off = ids(config.and_then(|d| d.get(b"OFF").ok()));
    for id in ids(properties.get(b"OCGs").ok()) {
        layers.insert(id, if base_on { !off.contains(&id) } else { on.contains(&id) });
    }
    // The print state a layer's usage gives it, where an automatic-state entry for printing names it.
    let states = config.and_then(|d| d.get(b"AS").ok()).and_then(|a| doc.dereference(a).ok()?.1.as_array().ok());
    for entry in states.into_iter().flatten() {
        let Ok((_, Object::Dictionary(entry))) = doc.dereference(entry) else { continue };
        let names = |key: &[u8]| -> Vec<Vec<u8>> {
            entry.get(key).ok().and_then(|v| doc.dereference(v).ok()?.1.as_array().ok())
                .map(|items| items.iter().filter_map(|i| i.as_name().ok().map(<[u8]>::to_vec)).collect())
                .unwrap_or_default()
        };
        if entry.get(b"Event").ok().and_then(|e| e.as_name().ok()) != Some(b"Print".as_slice())
            || !names(b"Category").iter().any(|c| c == b"Print") {
            continue;
        }
        for id in ids(entry.get(b"OCGs").ok()) {
            let state = doc.get_dictionary(id).ok().and_then(|g| g.get(b"Usage").ok())
                .and_then(|u| doc.dereference(u).ok()?.1.as_dict().ok())
                .and_then(|u| doc.dereference(u.get(b"Print").ok()?).ok()?.1.as_dict().ok())
                .and_then(|p| p.get(b"PrintState").ok()?.as_name().ok());
            if let (Some(state), Some(prints)) = (state, layers.get_mut(&id)) {
                *prints = state == b"ON";
            }
        }
    }
    layers
}

// ── Copying what a page draws ─────────────────────────────────────────────────

/// Copies objects of a source file into the output under new numbers, each once. A reference is copied as a
/// reference to the copy, which `drain` makes.
struct Copier<'a, 'd> {
    src:         &'a Document,
    dst:         &'d mut Document,
    /// The new number of each object copied or waiting to be.
    map:         HashMap<ObjectId, ObjectId>,
    /// The source objects whose copies are not made yet.
    queue:       Vec<ObjectId>,
    /// Whether each layer of the source prints.
    layers:      HashMap<ObjectId, bool>,
    /// The layers copied so far, by their new numbers, with whether each prints.
    used:        Vec<(ObjectId, bool)>,
    /// The appearance streams of the source, which take the form's default resources when they have none.
    appearances: HashSet<ObjectId>,
    /// The form's default resources (`/DR` of the `/AcroForm`), as the source has them.
    defaults:    Option<Object>,
}

impl<'a, 'd> Copier<'a, 'd> {
    fn new(src: &'a Document, dst: &'d mut Document) -> Self {
        let defaults = src.catalog().ok().and_then(|c| c.get(b"AcroForm").ok())
            .and_then(|f| src.dereference(f).ok()?.1.as_dict().ok())
            .and_then(|form| form.get(b"DR").ok()).cloned();
        Copier {
            src, dst,
            map: HashMap::new(),
            queue: Vec::new(),
            layers: read_layers(src),
            used: Vec::new(),
            appearances: HashSet::new(),
            defaults,
        }
    }

    /// The new number of the copy of `old`, which is made by the next `drain` when it isn't yet.
    fn id_for(&mut self, old: ObjectId) -> ObjectId {
        if let Some(&new) = self.map.get(&old) {
            return new;
        }
        let new = self.dst.new_object_id();
        self.map.insert(old, new);
        self.queue.push(old);
        new
    }

    /// `object` as the output has it: its references name copies. A reference to a page, a page tree or the catalog
    /// becomes null, so that what a page draws never pulls the document's structure along.
    fn object(&mut self, object: &Object, depth: usize) -> Result<Object, String> {
        if depth > MAX_NESTING {
            return Err(format!("an object nests more than {MAX_NESTING} levels deep"));
        }
        Ok(match object {
            Object::Reference(id) => match self.src.get_object(*id) {
                Ok(target) if is_structure(target) => Object::Null,
                Ok(_) => Object::Reference(self.id_for(*id)),
                Err(_) => Object::Null,
            },
            Object::Array(items) => Object::Array(
                items.iter().map(|item| self.object(item, depth + 1)).collect::<Result<_, _>>()?,
            ),
            Object::Dictionary(dict) => Object::Dictionary(self.dictionary(dict, depth)?),
            Object::Stream(stream) => {
                let mut dict = self.dictionary(&stream.dict, depth)?;
                dict.remove(b"Length");
                Object::Stream(Stream::new(dict, stream.content.clone()))
            }
            other => other.clone(),
        })
    }

    fn dictionary(&mut self, dict: &Dictionary, depth: usize) -> Result<Dictionary, String> {
        let mut copy = Dictionary::new();
        for (key, value) in dict.iter() {
            // What ties a drawing to the structure or the application that wrote it, not to what it looks like.
            if matches!(key.as_slice(), b"Parent" | b"P" | b"StructParent" | b"StructParents" | b"PieceInfo") {
                continue;
            }
            copy.set(key.clone(), self.object(value, depth + 1)?);
        }
        Ok(copy)
    }

    /// Make the copies that are waiting, and those their references bring.
    fn drain(&mut self) -> Result<(), String> {
        let mut made = 0usize;
        while let Some(old) = self.queue.pop() {
            let new = self.map[&old];
            let source = self.src.get_object(old).map_err(|e| e.to_string())?;
            let mut copy = self.object(source, 0)?;
            if let Some(&prints) = self.layers.get(&old) {
                // The usage of a layer is for the viewer that built the source's own view.
                if let Object::Dictionary(group) = &mut copy {
                    group.remove(b"Usage");
                }
                self.used.push((new, prints));
            }
            if self.appearances.contains(&old) {
                self.give_default_resources(&mut copy)?;
            }
            self.dst.objects.insert(new, copy);
            made += 1;
            if made % 4096 == 0 {
                crate::memory::check()?;
            }
        }
        Ok(())
    }

    /// An appearance stream with no resources of its own takes the form's default resources, as a viewer gives it.
    fn give_default_resources(&mut self, copy: &mut Object) -> Result<(), String> {
        let (Object::Stream(stream), Some(defaults)) = (copy, self.defaults.clone()) else { return Ok(()) };
        if !stream.dict.has(b"Resources") {
            stream.dict.set("Resources", self.object(&defaults, 0)?);
        }
        Ok(())
    }

    /// The layers copied, by their new numbers, with whether each prints.
    fn finish(self) -> Vec<(ObjectId, bool)> {
        self.used
    }
}

/// A page, a page tree node or the catalog: the structure of a document, never a part of what a page draws.
fn is_structure(object: &Object) -> bool {
    let dict = match object {
        Object::Dictionary(d) => d,
        _ => return false,
    };
    matches!(dict.get(b"Type").ok().and_then(|t| t.as_name().ok()), Some(b"Page" | b"Pages" | b"Catalog"))
}

/// The value of an attribute a page may inherit from its parents, as the file has it: a reference stays one.
fn inherited_raw<'a>(doc: &'a Document, page: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
    let mut node = page;
    for _ in 0..64 {
        if let Ok(value) = node.get(key) {
            return Some(value);
        }
        node = node.get(b"Parent").ok().and_then(|p| doc.dereference(p).ok()?.1.as_dict().ok())?;
    }
    None
}

// ── Importing a page ──────────────────────────────────────────────────────────

/// One annotation drawn on a page: the appearance's copy, the matrix that places it, and the layer it is in, if any.
struct Drawn {
    appearance: ObjectId,
    matrix:     [f32; 6],
    layer:      Option<Object>,
}

/// Import every page of a source into the output.
fn import(source: &Source, options: &Options, attribution: Option<&Attribution>, out: &mut Output) -> Result<Imported, String> {
    let src = open(source, options)?;
    out.raise_version(&src);
    let pages = src.get_pages();
    if pages.is_empty() {
        return Err("the PDF has no pages".to_string());
    }
    crate::memory::check()?;
    let mut copier = Copier::new(&src, &mut out.doc);
    let mut last = (0.0, 0.0);
    for (&number, &page_id) in &pages {
        let (id, size) = import_page(&mut copier, page_id, out.pages_id, attribution)
            .map_err(|e| format!("page {number}: {e}"))?;
        out.kids.push(Object::Reference(id));
        last = size;
        crate::memory::check()?;
    }
    for (group, prints) in copier.finish() {
        out.groups.push(group);
        if prints { out.on.push(group) } else { out.off.push(group) }
    }
    Ok(Imported { pages: pages.len(), last })
}

/// Import one page: a page of the output that draws it. Returns its object and its size as a viewer shows it.
fn import_page(
    copier: &mut Copier, page_id: ObjectId, pages_id: ObjectId, attribution: Option<&Attribution>,
) -> Result<(ObjectId, (f32, f32)), String> {
    let src = copier.src;
    let page = src.get_dictionary(page_id).map_err(|e| e.to_string())?;
    let geometry = Geometry::of(src, page);
    let (width, height) = geometry.size();

    let form = page_form(copier, page, &geometry)?;
    let drawn = flatten(copier, page)?;
    copier.drain()?;

    // The page's content: its drawing, turned and moved onto the output page, and its annotations in the same place.
    let [a, b, c, d, e, f] = onto_page(&geometry);
    let mut content = format!("q {} {} {} {} {} {} cm /LpdfPage Do\n", num(a), num(b), num(c), num(d), num(e), num(f));
    let mut xobjects = Dictionary::new();
    xobjects.set("LpdfPage", Object::Reference(form));
    let mut layers: Vec<(String, Object)> = Vec::new();
    for (n, annotation) in drawn.iter().enumerate() {
        let name = format!("LpdfAnnot{n}");
        xobjects.set(name.as_bytes(), Object::Reference(annotation.appearance));
        let layer = annotation.layer.as_ref().map(|layer| {
            let known = layers.iter().position(|(_, l)| l == layer).unwrap_or_else(|| {
                layers.push((format!("LpdfLayer{}", layers.len()), layer.clone()));
                layers.len() - 1
            });
            layers[known].0.clone()
        });
        let [a, b, c, d, e, f] = annotation.matrix;
        let begin = layer.as_ref().map(|l| format!("/OC /{l} BDC ")).unwrap_or_default();
        let end = if layer.is_some() { " EMC" } else { "" };
        content.push_str(&format!("{begin}q {} {} {} {} {} {} cm /{name} Do Q{end}\n", num(a), num(b), num(c), num(d), num(e), num(f)));
    }
    content.push_str("Q\n");
    if let Some(line) = attribution {
        let shown = Geometry { left: 0.0, bottom: 0.0, right: width, top: height, rotate: 0 };
        content.push_str(&line.draw(shown.line_matrix(line.place((width, height))), None));
    }

    let mut resources = Dictionary::new();
    resources.set("XObject", xobjects);
    if !layers.is_empty() {
        let mut properties = Dictionary::new();
        for (name, layer) in layers {
            properties.set(name.as_bytes(), layer);
        }
        resources.set("Properties", properties);
    }
    let mut dict = Dictionary::new();
    dict.set("Type", Object::Name(b"Page".to_vec()));
    dict.set("Parent", Object::Reference(pages_id));
    dict.set("MediaBox", media_box(width, height));
    dict.set("Resources", resources);
    if let Some(unit) = page.get(b"UserUnit").ok().and_then(|u| src.dereference(u).ok()?.1.as_float().ok()).filter(|u| *u > 0.0) {
        dict.set("UserUnit", Object::Real(unit));
    }
    let mut stream = Dictionary::new();
    stream.set("Filter", Object::Name(b"FlateDecode".to_vec()));
    let contents = copier.dst.add_object(Stream::new(stream, compress_to_vec_zlib(content.as_bytes(), 6)));
    dict.set("Contents", Object::Reference(contents));
    Ok((copier.dst.add_object(dict), (width, height)))
}

/// The page's drawing as a form XObject: its content and resources, clipped to its crop box.
fn page_form(copier: &mut Copier, page: &Dictionary, geometry: &Geometry) -> Result<ObjectId, String> {
    let src = copier.src;
    let (mut dict, content) = page_content(copier, page)?;
    dict.set("Type", Object::Name(b"XObject".to_vec()));
    dict.set("Subtype", Object::Name(b"Form".to_vec()));
    dict.set("BBox", Object::Array(
        [geometry.left, geometry.bottom, geometry.right, geometry.top].map(Object::Real).to_vec(),
    ));
    let resources = match inherited_raw(src, page, b"Resources") {
        Some(resources) => copier.object(resources, 0)?,
        None => Object::Dictionary(Dictionary::new()),
    };
    dict.set("Resources", resources);
    if let Ok(group) = page.get(b"Group") {
        dict.set("Group", copier.object(group, 0)?);
    }
    Ok(copier.dst.add_object(Stream::new(dict, content)))
}

/// The content of a page as the data of one stream, with the dictionary entries that say how it is encoded. A page
/// with one stream keeps it as it is; a page with several has them decoded and joined, since a form takes one.
fn page_content(copier: &mut Copier, page: &Dictionary) -> Result<(Dictionary, Vec<u8>), String> {
    let src = copier.src;
    let streams: Vec<&Stream> = contents_of(src, page).iter()
        .filter_map(|r| r.as_reference().ok())
        .filter_map(|id| src.get_object(id).ok()?.as_stream().ok())
        .collect();
    match streams[..] {
        [] => Ok((Dictionary::new(), Vec::new())),
        [only] => {
            let mut dict = Dictionary::new();
            for key in [&b"Filter"[..], b"DecodeParms"] {
                if let Ok(value) = only.dict.get(key) {
                    dict.set(key.to_vec(), copier.object(value, 0)?);
                }
            }
            Ok((dict, only.content.clone()))
        }
        _ => {
            let mut joined = Vec::new();
            for stream in streams {
                let part = stream.decompressed_content_with_limit(MAX_CONTENT_BYTES)
                    .map_err(|e| format!("the page's content could not be read: {e}"))?;
                if joined.len() + part.len() > MAX_CONTENT_BYTES {
                    return Err(format!("the page's content is more than {MAX_CONTENT_BYTES} bytes"));
                }
                // The streams of a page are split between tokens, so whitespace joins them without changing any.
                joined.extend_from_slice(&part);
                joined.push(b'\n');
            }
            let mut dict = Dictionary::new();
            dict.set("Filter", Object::Name(b"FlateDecode".to_vec()));
            Ok((dict, compress_to_vec_zlib(&joined, 6)))
        }
    }
}

/// The matrix that takes a source page's space, its crop box turned by its `/Rotate`, onto the output page that has
/// the page's size as a viewer shows it, from `(0, 0)` up.
pub(crate) fn onto_page(geometry: &Geometry) -> [f32; 6] {
    let Geometry { left, bottom, right, top, rotate } = *geometry;
    match rotate {
        90  => [0.0, -1.0, 1.0, 0.0, -bottom, right],
        180 => [-1.0, 0.0, 0.0, -1.0, right, top],
        270 => [0.0, 1.0, -1.0, 0.0, top, -left],
        _   => [1.0, 0.0, 0.0, 1.0, -left, -bottom],
    }
}

fn media_box(width: f32, height: f32) -> Object {
    Object::Array(vec![Object::Integer(0), Object::Integer(0), Object::Real(width), Object::Real(height)])
}

// ── Flattening annotations ────────────────────────────────────────────────────

/// Each annotation of a page that prints, with its stored appearance placed by its rectangle. Links and pop-ups draw
/// nothing. A form field that has a value and no stored appearance is an error: it would print blank.
fn flatten(copier: &mut Copier, page: &Dictionary) -> Result<Vec<Drawn>, String> {
    let src = copier.src;
    let Some(list) = page.get(b"Annots").ok().and_then(|a| src.dereference(a).ok()?.1.as_array().ok()) else {
        return Ok(Vec::new());
    };
    let mut drawn = Vec::new();
    for item in list {
        let Ok((_, Object::Dictionary(annotation))) = src.dereference(item) else { continue };
        let subtype = annotation.get(b"Subtype").ok().and_then(|s| s.as_name().ok()).unwrap_or_default();
        if matches!(subtype, b"Link" | b"Popup") {
            continue;
        }
        let flags = annotation.get(b"F").ok().and_then(|f| src.dereference(f).ok()?.1.as_i64().ok()).unwrap_or(0);
        if flags & FLAG_PRINT == 0 || flags & FLAG_HIDDEN != 0 {
            continue;
        }
        let Some(appearance) = normal_appearance(src, annotation) else {
            if subtype == b"Widget" {
                if let Some(field) = field_with_value(src, annotation) {
                    return Err(format!(
                        "{field} has a value but no stored appearance, so it would print blank: open the file in a PDF \
                         viewer and save it, which stores the appearance",
                    ));
                }
            }
            continue;
        };
        let Some(matrix) = place_appearance(src, appearance.1, annotation) else { continue };
        copier.appearances.insert(appearance.0);
        let layer = match annotation.get(b"OC") {
            Ok(oc) => Some(copier.object(oc, 0)?).filter(|o| !matches!(o, Object::Null)),
            Err(_) => None,
        };
        drawn.push(Drawn { appearance: copier.id_for(appearance.0), matrix, layer });
    }
    Ok(drawn)
}

/// The normal appearance of an annotation: its stream, which a check box or a radio button picks from its states by
/// `/AS`.
fn normal_appearance<'a>(src: &'a Document, annotation: &'a Dictionary) -> Option<(ObjectId, &'a Stream)> {
    let appearance = src.dereference(annotation.get(b"AP").ok()?).ok()?.1.as_dict().ok()?;
    let (id, normal) = src.dereference(appearance.get(b"N").ok()?).ok()?;
    match normal {
        Object::Stream(stream) => Some((id?, stream)),
        Object::Dictionary(states) => {
            let state = annotation.get(b"AS").ok()?.as_name().ok()?;
            let (id, stream) = src.dereference(states.get(state).ok()?).ok()?;
            Some((id?, stream.as_stream().ok()?))
        }
        _ => None,
    }
}

/// The matrix that draws an appearance stream in its annotation's rectangle (PDF 32000-1, 12.5.5): the appearance's
/// box, with its own matrix applied, fitted to the rectangle.
fn place_appearance(src: &Document, appearance: &Stream, annotation: &Dictionary) -> Option<[f32; 6]> {
    let [rx1, ry1, rx2, ry2] = rect(src, annotation.get(b"Rect").ok()?)?;
    let [bx1, by1, bx2, by2] = rect(src, appearance.dict.get(b"BBox").ok()?)?;
    let own = appearance.dict.get(b"Matrix").ok().and_then(|m| src.dereference(m).ok()?.1.as_array().ok())
        .map(|items| items.iter().filter_map(|n| src.dereference(n).ok()?.1.as_float().ok()).collect::<Vec<f32>>())
        .filter(|numbers| numbers.len() == 6)
        .map_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0], |n| [n[0], n[1], n[2], n[3], n[4], n[5]]);
    // The box's corners where the appearance's own matrix puts them, and the box around those.
    let corners = [(bx1, by1), (bx2, by1), (bx1, by2), (bx2, by2)]
        .map(|(x, y)| (own[0] * x + own[2] * y + own[4], own[1] * x + own[3] * y + own[5]));
    let (tx1, tx2) = (corners.iter().map(|c| c.0).fold(f32::MAX, f32::min), corners.iter().map(|c| c.0).fold(f32::MIN, f32::max));
    let (ty1, ty2) = (corners.iter().map(|c| c.1).fold(f32::MAX, f32::min), corners.iter().map(|c| c.1).fold(f32::MIN, f32::max));
    if !(tx2 - tx1 > 0.0 && ty2 - ty1 > 0.0) {
        return None;
    }
    let (sx, sy) = ((rx2 - rx1) / (tx2 - tx1), (ry2 - ry1) / (ty2 - ty1));
    Some([sx, 0.0, 0.0, sy, rx1 - tx1 * sx, ry1 - ty1 * sy])
}

/// What a form field says about itself, walking up from its widget through its parents: the name it is known by, the
/// field type and flags and the value, each from the nearest node that has it.
struct Field<'a> {
    name:  String,
    kind:  Option<&'a [u8]>,
    flags: i64,
    value: Option<&'a Object>,
}

fn field_of<'a>(src: &'a Document, widget: &'a Dictionary) -> Field<'a> {
    let mut field = Field { name: String::new(), kind: None, flags: 0, value: None };
    let mut parts: Vec<String> = Vec::new();
    let mut node = widget;
    for _ in 0..32 {
        if let Some(title) = node.get(b"T").ok().and_then(|t| src.dereference(t).ok()).and_then(|(_, t)| lopdf::decode_text_string(t).ok()) {
            parts.push(title);
        }
        if field.kind.is_none() {
            field.kind = node.get(b"FT").ok().and_then(|t| src.dereference(t).ok()?.1.as_name().ok());
        }
        if field.value.is_none() {
            field.value = node.get(b"V").ok().and_then(|v| src.dereference(v).ok()).map(|(_, v)| v);
        }
        if field.flags == 0 {
            field.flags = node.get(b"Ff").ok().and_then(|f| src.dereference(f).ok()?.1.as_i64().ok()).unwrap_or(0);
        }
        match node.get(b"Parent").ok().and_then(|p| src.dereference(p).ok()?.1.as_dict().ok()) {
            Some(parent) => node = parent,
            None => break,
        }
    }
    parts.reverse();
    field.name = parts.join(".");
    field
}

/// The description of a widget's field, `the form field 'name'`, when the field has a value that would print: text or
/// a choice with text in it, a check box or radio button that is on.
fn field_with_value(src: &Document, widget: &Dictionary) -> Option<String> {
    let field = field_of(src, widget);
    let value = field.value?;
    let has_value = match field.kind? {
        b"Tx" | b"Ch" => match value {
            Object::String(text, _) => !text.is_empty(),
            Object::Array(items)    => !items.is_empty(),
            Object::Stream(_)       => true,
            _ => false,
        },
        b"Btn" if field.flags & FIELD_PUSH_BUTTON == 0 => matches!(value, Object::Name(state) if state != b"Off"),
        _ => false,
    };
    has_value.then(|| if field.name.is_empty() { "a form field".to_string() } else { format!("the form field '{}'", field.name) })
}

// ── The output ────────────────────────────────────────────────────────────────

/// The PDF being written.
struct Output {
    doc:      Document,
    pages_id: ObjectId,
    kids:     Vec<Object>,
    version:  Option<(u32, u32)>,
    /// The layers the pages draw, and which of them print.
    groups:   Vec<ObjectId>,
    on:       Vec<ObjectId>,
    off:      Vec<ObjectId>,
}

impl Output {
    fn new() -> Self {
        let mut doc = Document::new();
        // A table in any version: a stream for it is for 1.5 and later.
        doc.reference_table.cross_reference_type = XrefType::CrossReferenceTable;
        let pages_id = doc.new_object_id();
        Output { doc, pages_id, kids: Vec::new(), version: None, groups: Vec::new(), on: Vec::new(), off: Vec::new() }
    }

    /// The output's version is the highest among the inputs.
    fn raise_version(&mut self, source: &Document) {
        self.version = self.version.max(version_of(source));
    }

    /// A page with nothing on it, of the size given.
    fn blank_page(&mut self, (width, height): (f32, f32)) {
        let mut dict = Dictionary::new();
        dict.set("Type", Object::Name(b"Page".to_vec()));
        dict.set("Parent", Object::Reference(self.pages_id));
        dict.set("MediaBox", media_box(width, height));
        dict.set("Resources", Dictionary::new());
        let id = self.doc.add_object(dict);
        self.kids.push(Object::Reference(id));
    }

    /// The file: the page tree, the catalog with the layers, and the bytes.
    fn finish(mut self) -> Result<Vec<u8>, String> {
        let mut pages = Dictionary::new();
        pages.set("Type", Object::Name(b"Pages".to_vec()));
        pages.set("Count", self.kids.len() as i64);
        pages.set("Kids", Object::Array(std::mem::take(&mut self.kids)));
        self.doc.objects.insert(self.pages_id, Object::Dictionary(pages));

        let mut catalog = Dictionary::new();
        catalog.set("Type", Object::Name(b"Catalog".to_vec()));
        catalog.set("Pages", Object::Reference(self.pages_id));
        if !self.groups.is_empty() {
            let references = |ids: &[ObjectId]| Object::Array(ids.iter().map(|id| Object::Reference(*id)).collect());
            let mut config = Dictionary::new();
            config.set("BaseState", Object::Name(b"ON".to_vec()));
            config.set("ON", references(&self.on));
            config.set("OFF", references(&self.off));
            let mut properties = Dictionary::new();
            properties.set("OCGs", references(&self.groups));
            properties.set("D", config);
            catalog.set("OCProperties", properties);
        }
        let root = self.doc.add_object(catalog);
        self.doc.trailer.set("Root", Object::Reference(root));
        let (major, minor) = self.version.unwrap_or((1, 4));
        self.doc.version = format!("{major}.{minor}");

        crate::memory::check()?;
        let mut bytes = Vec::new();
        self.doc.save_to(&mut bytes).map_err(|e| format!("the merged PDF could not be written: {e}"))?;
        crate::memory::check()?;
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_options_are_off_unless_given() {
        assert_eq!(parse_options(&serde_json::Value::Null).unwrap(), Options::default());
        let all = parse_options(&json!({ "allow-signed": true, "allow-restricted": true, "double-sided": true })).unwrap();
        assert_eq!(all, Options { allow_signed: true, allow_restricted: true, double_sided: true });
        assert_eq!(parse_options(&json!({ "double-sided": true })).unwrap(), Options { double_sided: true, ..Options::default() });
    }

    #[test]
    fn a_wrong_option_is_an_error_that_names_it() {
        assert_eq!(parse_options(&json!({ "allowSigned": true })).err().unwrap(), "the merge has no option 'allowSigned'");
        assert_eq!(
            parse_options(&json!({ "double-sided": "yes" })).err().unwrap(),
            "the merge option double-sided must be true or false, not \"yes\"",
        );
        assert_eq!(parse_options(&json!(3)).err().unwrap(), "the merge options must be an object, not 3");
    }

    #[test]
    fn the_files_of_a_call_are_cut_from_the_bytes_by_their_lengths() {
        let sources = sources_from_parts(b"aabbb", r#"[{"name":"a.pdf","length":2},{"name":"b.pdf","length":3}]"#).unwrap();
        assert_eq!(sources.iter().map(|s| (s.name.as_str(), s.pdf.as_slice())).collect::<Vec<_>>(), [("a.pdf", &b"aa"[..]), ("b.pdf", b"bbb")]);
        assert_eq!(
            sources_from_parts(b"aa", r#"[{"name":"a.pdf","length":3}]"#).err().unwrap(),
            "file 1 (a.pdf) is longer than the bytes sent",
        );
        assert_eq!(
            sources_from_parts(b"", r#"[{"length":0}]"#).err().unwrap(),
            "file 1 has no name: give it the file's name, which every error about it names",
        );
    }

    #[test]
    fn the_matrix_of_each_turn_takes_the_page_as_shown_onto_the_output_page() {
        // A crop box of 400 by 600 at (100, 50): the point `(u, v)` of the page as shown, from its top-left corner with
        // y down, lands at `(u, height - v)` of the output page, whatever the turn.
        for rotate in [0, 90, 180, 270] {
            let geometry = Geometry { left: 100.0, bottom: 50.0, right: 500.0, top: 650.0, rotate };
            let (_, height) = geometry.size();
            let [a, b, c, d, e, f] = onto_page(&geometry);
            for (u, v) in [(0.0, 0.0), (10.0, 0.0), (0.0, 20.0), (123.0, 77.0)] {
                let (x, y) = geometry.point(u, v);
                let landed = (a * x + c * y + e, b * x + d * y + f);
                assert!((landed.0 - u).abs() < 0.001 && (landed.1 - (height - v)).abs() < 0.001, "{rotate}: {landed:?}");
            }
        }
    }
}
