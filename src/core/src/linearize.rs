//! Linearization, "fast web view" (ISO 32000-1, Annex F): rewrites a finished file so that a viewer that fetches it by
//! byte ranges can show page 1 before the rest has arrived, and fetch any page on its own.
//!
//! The file becomes: the header; the linearization dictionary; the cross-reference section of the first page, with the
//! trailer; the catalog and what opening the document needs; the hint stream; page 1 with every object it uses; each
//! other page with the objects only it uses, in page order; the objects the other pages share; the rest (the page tree,
//! the outline, the document information, the structure of a tagged file); and the main cross-reference section.
//!
//! The objects are given new numbers, so that each page's are numbered one after the other from its page object, as
//! the hint tables expect, and those of the first page's section after those of the rest, so each section of the
//! cross-reference table covers one range. Which objects go where is worked out from the file, as qpdf 12 does when it
//! checks a linearized file (`calculateLinearizationData`): an object belongs to a page when the page reaches it, and
//! is shared when more than one does. qpdf's `--check-linearization` then reads the hints back against the file.
//!
//! qpdf before 12 put an object that one page other than page 1 reaches, and a key of the catalog too, with the rest
//! rather than with its page: in a tagged file, the link annotations after page 1, which the structure reaches as
//! well. Annex F calls an object private to a page when no other page reaches it, as qpdf 12 does, and a viewer gets
//! a page's links with the page.
//!
//! It runs last, after the encryption pass: any change to the file afterwards moves the offsets the hints give. Under
//! RC4 the key of a string or a stream is its object's, so an object that gets a new number is encrypted again under
//! the new one, and the hint stream is encrypted as every other stream is. The same file gives the same bytes.
//!
//! It reads what the PDF writer and the encryption pass write: a classic xref table, direct `/Length` values and no
//! object streams, as `rewrite.rs` does.

use std::collections::{BTreeMap, BTreeSet};

use super::encrypt::{FileCipher, parse_xref};
use super::hints::{Hints, OutlineHint, PageHint};
use super::objects::{Object, Part, Value, read_object, read_value};
use super::rewrite::{read_hex_string, read_literal_string};

/// The keys of the catalog whose objects a viewer needs to open the document, as qpdf takes them (Annex F, part 4).
const OPEN_DOCUMENT_KEYS: [&[u8]; 5] = [b"ViewerPreferences", b"PageMode", b"Threads", b"OpenAction", b"AcroForm"];

/// The width the first trailer's `/Prev` is kept to, so that its length is known before the offset it holds: the
/// widest number of 32 bits.
const NUMBER_WIDTH: usize = 10;

/// What a walk through the file starts from, as qpdf tells what uses an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum User<'a> {
    /// The catalog itself.
    Root,
    /// A key of the catalog.
    RootKey(&'a [u8]),
    /// A key of the trailer other than `/Root`.
    Trailer(&'a [u8]),
    /// The page at this place, from 0.
    Page(usize),
}

/// The section an object goes to, as qpdf 12 sorts them (`ObjCategory` in `QPDF_linearization.cc`). When more than
/// one walk reaches an object, the earlier section mostly wins: see [`Placement::update`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Category {
    /// The catalog.
    Root,
    /// What opening the document needs: the keys of `OPEN_DOCUMENT_KEYS`, and the encryption dictionary.
    OpenDocument,
    /// Reached by page 1 and by another page.
    FirstPageShared,
    /// Reached by page 1 and by no other page.
    FirstPagePrivate,
    /// Reached by more than one page other than page 1.
    OtherPageShared,
    /// Reached by one page other than page 1, and by no other page.
    OtherPagePrivate,
    /// Reached from the catalog's `/Pages`: the page tree.
    PageTree,
    /// The outline dictionary.
    OutlineDictionary,
    /// The rest of the outline.
    Outline,
    /// The rest.
    Other,
}

/// Where an object goes, from the walks that reached it so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Placement {
    category: Category,
    /// The page that reached it, or for one that more than one page did, the first other than page 1; `-1` when no page
    /// did. qpdf's `pageno_`.
    page:     i64,
}

impl Default for Placement {
    fn default() -> Self {
        Self { category: Category::Other, page: -1 }
    }
}

impl Placement {
    /// Where `user` alone would put an object it reaches, `top` when the object is where the walk starts.
    fn of(user: User<'_>, top: bool) -> Self {
        let category = match user {
            User::Root => Category::Root,
            User::Page(0) => return Self { category: Category::FirstPagePrivate, page: 0 },
            User::Page(index) => return Self { category: Category::OtherPagePrivate, page: index as i64 },
            User::RootKey(key) if OPEN_DOCUMENT_KEYS.contains(&key) => Category::OpenDocument,
            User::RootKey(b"Outlines") if top => Category::OutlineDictionary,
            User::RootKey(b"Outlines") => Category::Outline,
            User::RootKey(b"Pages") => Category::PageTree,
            User::Trailer(b"Encrypt") => Category::OpenDocument,
            User::RootKey(_) | User::Trailer(_) => Category::Other,
        };
        Self { category, page: -1 }
    }

    /// Folds in one more walk that reached the object, as qpdf 12 does (`ObjCategory::update`).
    fn update(&mut self, user: User<'_>, top: bool) {
        let other = Self::of(user, top);
        let low = self.category.min(other.category);
        let high = self.category.max(other.category);
        let page = self.page;
        self.page = if page <= 0 || other.page <= 0 { page.max(other.page) } else { page.min(other.page) };
        self.category = match low {
            Category::Root | Category::OpenDocument | Category::FirstPageShared => low,
            Category::FirstPagePrivate if matches!(high, Category::OtherPagePrivate | Category::OtherPageShared) => {
                Category::FirstPageShared
            }
            Category::OtherPagePrivate if high == Category::OtherPagePrivate && page != other.page => Category::OtherPageShared,
            _ => low,
        };
    }
}

/// The file the pass reads.
struct File<'a> {
    src:          &'a [u8],
    objects:      BTreeMap<u32, Object>,
    trailer:      Value,
    root:         u32,
    /// Where the first object is: the header ends there.
    first_object: usize,
}

/// Rewrites `src` as a linearized file. `cipher` is the one the encryption pass encrypted it with, if it did.
pub(crate) fn linearize(src: &[u8], cipher: Option<&mut FileCipher>) -> Result<Vec<u8>, String> {
    let file = File::read(src)?;
    let pages = file.pages()?;
    let reached = Reached::of(&file, &pages);
    let parts = Parts::of(&file, &pages, &reached)?;
    write(&file, &pages, &reached, &parts, cipher)
}

impl<'a> File<'a> {
    fn read(src: &'a [u8]) -> Result<Self, String> {
        let xref = parse_xref(src)?;
        let mut objects = BTreeMap::new();
        for entry in &xref.entries {
            let object = read_object(src, entry.offset).map_err(|e| format!("object {}: {e}", entry.obj_num))?;
            objects.insert(entry.obj_num, object);
        }
        let trailer = read_value(src, xref.trailer + b"trailer".len())?;
        let first_object = xref.entries.iter().map(|entry| entry.offset).min().ok_or("the file has no objects")?;
        Ok(Self { src, objects, trailer, root: xref.root_ref.0, first_object })
    }

    fn object(&self, number: u32) -> Result<&Object, String> {
        self.objects.get(&number).ok_or_else(|| format!("a reference to object {number}, which the file does not have"))
    }

    /// The page objects, in the order of the page tree.
    fn pages(&self) -> Result<Vec<u32>, String> {
        let root = self.object(self.root)?;
        let tree = root.value.reference(b"Pages").ok_or("the catalog has no /Pages")?;
        let mut pages = Vec::new();
        let mut pending = vec![tree];
        let mut seen = BTreeSet::new();
        while let Some(number) = pending.pop() {
            if !seen.insert(number) {
                return Err(format!("the page tree reaches object {number} twice"));
            }
            let node = &self.object(number)?.value;
            if node.is_type(b"Page") {
                pages.push(number);
            } else if let Some(Value::Array(kids)) = node.get(b"Kids") {
                // Pushed last first, so the first kid comes off first.
                for kid in kids.iter().rev() {
                    let Value::Ref(kid) = kid else { return Err(format!("a kid of page tree node {number} is not a reference")) };
                    pending.push(*kid);
                }
            }
        }
        if pages.is_empty() {
            return Err("the file has no pages".to_string());
        }
        Ok(pages)
    }

    /// The objects reached from `start`, as qpdf follows references to tell what uses what (`updateObjectMaps`): through
    /// dictionaries, arrays and stream dictionaries, but never up a page's `/Parent`, nor into a page other than the
    /// one the walk starts at. With them, the object `start` refers to, when it is a reference.
    fn reach(&self, start: &Value) -> (BTreeSet<u32>, Option<u32>) {
        let mut reached = BTreeSet::new();
        let mut pending: Vec<(&Value, bool)> = vec![(start, true)];
        while let Some((value, top)) = pending.pop() {
            let value = match value {
                Value::Ref(number) => {
                    // A reference to an object the file does not have is null, and reaches nothing.
                    let Some(object) = self.objects.get(number) else { continue };
                    if object.value.is_type(b"Page") && !top {
                        continue;
                    }
                    if !reached.insert(*number) {
                        continue;
                    }
                    &object.value
                }
                direct => direct,
            };
            let is_page = value.is_type(b"Page");
            match value {
                Value::Dict(entries) => {
                    for (key, item) in entries {
                        if !(is_page && key == b"Parent") {
                            pending.push((item, false));
                        }
                    }
                }
                Value::Array(items) => pending.extend(items.iter().map(|item| (item, false))),
                _ => {}
            }
        }
        let top = match start {
            Value::Ref(number) if reached.contains(number) => Some(*number),
            _ => None,
        };
        (reached, top)
    }
}

/// Where each object goes, and what each page reaches.
struct Reached {
    placements: BTreeMap<u32, Placement>,
    by_page:    Vec<BTreeSet<u32>>,
}

impl Reached {
    /// Walks the file from each page, the trailer's keys, the catalog's keys and the catalog, in qpdf's order.
    fn of(file: &File<'_>, pages: &[u32]) -> Self {
        let mut placements: BTreeMap<u32, Placement> = BTreeMap::new();
        let mut place = |(reached, top): (BTreeSet<u32>, Option<u32>), user: User<'_>| {
            for number in &reached {
                placements.entry(*number).or_default().update(user, Some(*number) == top);
            }
            reached
        };

        let by_page: Vec<BTreeSet<u32>> =
            pages.iter().enumerate().map(|(index, &page)| place(file.reach(&Value::Ref(page)), User::Page(index))).collect();
        if let Value::Dict(entries) = &file.trailer {
            for (key, value) in entries.iter().filter(|(key, _)| key != b"Root") {
                place(file.reach(value), User::Trailer(key));
            }
        }
        if let Some(Value::Dict(entries)) = file.objects.get(&file.root).map(|root| &root.value) {
            for (key, value) in entries {
                place(file.reach(value), User::RootKey(key));
            }
        }
        place((BTreeSet::from([file.root]), Some(file.root)), User::Root);
        Self { placements, by_page }
    }

    fn category(&self, number: u32) -> Option<Category> {
        self.placements.get(&number).map(|placement| placement.category)
    }

    /// Whether the shared object table lists the object: page 1's section holds it, or the shared one.
    fn is_shared(&self, number: u32) -> bool {
        matches!(self.category(number), Some(Category::FirstPageShared | Category::OtherPageShared))
    }
}

/// Where each object goes, by the sections of Annex F, each in the order it is written.
struct Parts {
    /// The catalog, and what opening the document needs.
    open:          Vec<u32>,
    /// Page 1 and every object it reaches, and the outline when the document opens with it showing.
    first_page:    Vec<u32>,
    /// Each other page, with the objects only it reaches.
    other_pages:   Vec<Vec<u32>>,
    /// The objects more than one of the other pages reach.
    shared:        Vec<u32>,
    /// The rest: the page tree, the outline, and what no page reaches.
    rest:          Vec<u32>,
    /// The outline dictionary and the objects it reaches, when there is an outline.
    outline:       Option<Vec<u32>>,
}

impl Parts {
    /// Puts the objects in their sections as qpdf 12 does (`calculateLinearizationData`), which is what its check
    /// compares the hints with: each in the order of its number within its category.
    fn of(file: &File<'_>, pages: &[u32], reached: &Reached) -> Result<Self, String> {
        let of = |category: Category| -> Vec<u32> {
            reached.placements.iter().filter(|(_, p)| p.category == category).map(|(&number, _)| number).collect()
        };

        let root = file.object(file.root)?;
        let outline = root.value.reference(b"Outlines").map(|_| {
            let mut objects = of(Category::OutlineDictionary);
            objects.extend(of(Category::Outline));
            objects
        });
        let outline_first = matches!(root.value.get(b"PageMode"), Some(Value::Name(mode)) if mode == b"UseOutlines");

        let mut open = of(Category::Root);
        open.extend(of(Category::OpenDocument));

        let first = pages[0];
        if reached.category(first) != Some(Category::FirstPagePrivate) {
            return Err(format!("page 1, object {first}, is reached from elsewhere than itself"));
        }
        let mut first_page = vec![first];
        first_page.extend(of(Category::FirstPagePrivate).into_iter().filter(|&number| number != first));
        first_page.extend(of(Category::FirstPageShared));
        if outline_first && let Some(outline) = &outline {
            first_page.extend(outline);
        }

        let mut other_pages: Vec<Vec<u32>> = pages[1..].iter().map(|&page| vec![page]).collect();
        for (&number, placement) in &reached.placements {
            if placement.category == Category::OtherPagePrivate && !pages.contains(&number) {
                other_pages[placement.page as usize - 1].push(number);
            }
        }
        for (index, &page) in pages.iter().enumerate().skip(1) {
            let placement = reached.placements.get(&page);
            if placement != Some(&Placement { category: Category::OtherPagePrivate, page: index as i64 }) {
                return Err(format!("page {}, object {page}, is reached from elsewhere than itself", index + 1));
            }
        }

        let shared = of(Category::OtherPageShared);

        let mut rest = of(Category::PageTree);
        if !outline_first && let Some(outline) = &outline {
            rest.extend(outline);
        }
        rest.extend(of(Category::Other));
        // What nothing reaches stays in the file, at its end.
        rest.extend(file.objects.keys().copied().filter(|number| !reached.placements.contains_key(number)));

        Ok(Self { open, first_page, other_pages, shared, rest, outline })
    }

    /// The objects of the main section, in the order they are written: the other pages, the shared objects, the rest.
    fn main_section(&self) -> impl Iterator<Item = u32> + '_ {
        self.other_pages.iter().flatten().chain(&self.shared).chain(&self.rest).copied()
    }
}

/// An object as it is written in the new file.
struct Written {
    number:     u32,
    generation: u32,
    bytes:      Vec<u8>,
}

fn write(
    file: &File<'_>,
    pages: &[u32],
    reached: &Reached,
    parts: &Parts,
    cipher: Option<&mut FileCipher>,
) -> Result<Vec<u8>, String> {
    // The new numbers: the main section from 1, then the linearization dictionary, the objects opening needs, the hint
    // stream and the first page's section.
    let mut renumber: BTreeMap<u32, u32> = BTreeMap::new();
    let mut next = 1;
    for old in parts.main_section() {
        renumber.insert(old, next);
        next += 1;
    }
    let first_section_start = next;
    let linearization_number = next;
    next += 1;
    for &old in &parts.open {
        renumber.insert(old, next);
        next += 1;
    }
    let hint_number = next;
    next += 1;
    for &old in &parts.first_page {
        renumber.insert(old, next);
        next += 1;
    }
    let size = next;

    let encryption_dictionary = file.trailer.reference(b"Encrypt");
    let new_number = |old: u32| renumber.get(&old).copied().ok_or_else(|| format!("object {old} has no place in the file"));
    let copy = |old: u32| -> Result<Written, String> {
        let object = file.object(old)?;
        let number = new_number(old)?;
        // The /Encrypt dictionary is not encrypted, so it has nothing to encrypt again.
        let reseal = cipher.as_deref().filter(|cipher| cipher.keys_by_object() && Some(old) != encryption_dictionary);
        let bytes = copy_object(file.src, object, (old, number), &new_number, reseal)?;
        Ok(Written { number, generation: object.generation, bytes })
    };
    let open: Vec<Written> = parts.open.iter().map(|&old| copy(old)).collect::<Result<_, _>>()?;
    let first_page: Vec<Written> = parts.first_page.iter().map(|&old| copy(old)).collect::<Result<_, _>>()?;
    let main: Vec<Written> = parts.main_section().map(copy).collect::<Result<_, _>>()?;
    let ids = Ids::of(file, &new_number)?;

    // What comes before the hint stream has a length known now, as the numbers of the linearization dictionary and of
    // the first trailer's /Prev are kept to a fixed width.
    let header = &file.src[..file.first_object];
    let dictionary_length = linearization_dictionary(linearization_number, &Numbers::widest()).len();
    let first_xref_offset = header.len() + dictionary_length + DICTIONARY_END.len();
    let first_section_objects = (size - first_section_start) as usize;
    let first_xref_length = first_xref(first_section_start, size, &[], &ids, None).len() + first_section_objects * 20;
    let open_start = first_xref_offset + first_xref_length;
    let hint_offset = open_start + open.iter().map(|object| object.bytes.len()).sum::<usize>();

    // Where each object would be without the hint stream, as the hints give it, by its new number.
    let mut unhinted: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
    let mut at = open_start;
    for object in open.iter().chain(&first_page).chain(&main) {
        unhinted.insert(object.number, (at, object.bytes.len()));
        at += object.bytes.len();
    }
    let place = |old: u32| -> Result<(u32, usize, usize), String> {
        let number = new_number(old)?;
        let (offset, length) = unhinted[&number];
        Ok((number, offset, length))
    };

    let stream = hints(parts, pages, reached, &place)?.encode();
    let data = match cipher {
        Some(cipher) => cipher.seal(hint_number, 0, &stream.data),
        None => stream.data,
    };
    let mut hint_object = format!("{hint_number} 0 obj\n<< /Length {} /S {}", data.len(), stream.shared).into_bytes();
    if let Some(outline) = stream.outline {
        hint_object.extend(format!(" /O {outline}").bytes());
    }
    hint_object.extend(b" >>\nstream\n");
    hint_object.extend(&data);
    hint_object.extend(b"\nendstream\nendobj\n");
    let hint_length = hint_object.len();

    // Where each object is, the hint stream counted.
    let offset_of = |number: u32| {
        let (offset, _) = unhinted[&number];
        if offset >= hint_offset { offset + hint_length } else { offset }
    };
    let end_of = |objects: &[Written]| objects.last().map(|object| offset_of(object.number) + object.bytes.len());
    let first_page_end = end_of(&first_page).ok_or("the first page's section is empty")?;
    let main_xref_offset = end_of(&main).unwrap_or(first_page_end);
    let main_xref = main_xref(first_section_start, &main, &offset_of, &ids, first_xref_offset);
    let file_length = main_xref_offset + main_xref.bytes.len();

    let numbers = Numbers {
        file_length,
        hint_offset,
        hint_length,
        first_page: new_number(pages[0])?,
        first_page_end,
        pages: pages.len(),
        main_xref_first_entry: main_xref_offset + main_xref.first_entry,
    };
    let mut out = Vec::with_capacity(file_length);
    out.extend_from_slice(header);
    let dictionary = linearization_dictionary(linearization_number, &numbers);
    out.extend(&dictionary);
    out.resize(out.len() + dictionary_length - dictionary.len(), b' ');
    out.extend(DICTIONARY_END);

    let mut entries: Vec<(usize, u32)> = vec![(header.len(), 0)];
    entries.extend(open.iter().map(|object| (offset_of(object.number), object.generation)));
    entries.push((hint_offset, 0));
    entries.extend(first_page.iter().map(|object| (offset_of(object.number), object.generation)));
    out.extend(first_xref(first_section_start, size, &entries, &ids, Some(main_xref_offset)));

    for object in &open {
        out.extend(&object.bytes);
    }
    out.extend(hint_object);
    for object in first_page.iter().chain(&main) {
        out.extend(&object.bytes);
    }
    out.extend(main_xref.bytes);
    if out.len() != file_length {
        return Err(format!("the linearized file is {} bytes where its dictionary says {file_length}", out.len()));
    }
    Ok(out)
}

/// The hint tables, from where each object would be without the hint stream: `place` gives an object's new number,
/// offset and length by its old number.
fn hints(
    parts: &Parts,
    pages: &[u32],
    reached: &Reached,
    place: &impl Fn(u32) -> Result<(u32, usize, usize), String>,
) -> Result<Hints, String> {
    let to_u32 = |value: usize| u32::try_from(value).map_err(|_| "the file is too large to linearize: 4 GiB or more".to_string());
    let length_of = |objects: &[u32]| -> Result<u32, String> {
        objects.iter().map(|&old| place(old).map(|(_, _, length)| length)).sum::<Result<usize, _>>().and_then(to_u32)
    };

    // The shared object table: each object of the first page's section, then each of the shared section.
    let shared_objects: Vec<u32> = parts.first_page.iter().chain(&parts.shared).copied().collect();
    let shared_index: BTreeMap<u32, u32> = shared_objects.iter().enumerate().map(|(index, &old)| (old, index as u32)).collect();
    let shared_lengths = shared_objects.iter().map(|&old| length_of(&[old])).collect::<Result<Vec<_>, _>>()?;

    let mut page_hints = vec![PageHint { objects: parts.first_page.len() as u32, length: length_of(&parts.first_page)?, shared: vec![] }];
    for (index, objects) in parts.other_pages.iter().enumerate() {
        // The shared objects a page reaches, by their place in the table.
        let shared = reached.by_page[index + 1].iter()
            .filter(|&&old| reached.is_shared(old))
            .filter_map(|old| shared_index.get(old).copied())
            .collect();
        page_hints.push(PageHint { objects: objects.len() as u32, length: length_of(objects)?, shared });
    }

    // The section of the shared objects, or where it would start when it is empty: the object after the pages.
    let first_shared = parts.shared.first().or(parts.rest.first()).copied().unwrap_or(pages[0]);
    let (first_shared_object, first_shared_offset, _) = place(first_shared)?;
    let (_, first_page_offset, _) = place(pages[0])?;

    let outline = match &parts.outline {
        Some(objects) => {
            let (first_object, offset, _) = place(objects[0])?;
            Some(OutlineHint { first_object, offset: to_u32(offset)?, objects: objects.len() as u32, length: length_of(objects)? })
        }
        None => None,
    };

    Ok(Hints {
        first_page_offset: to_u32(first_page_offset)?,
        pages: page_hints,
        shared_lengths,
        first_page_shared: parts.first_page.len() as u32,
        first_shared_object,
        first_shared_offset: to_u32(first_shared_offset)?,
        outline,
    })
}

/// Copies `object`, given `numbers`, its old number and its new one, with every reference given its new number by
/// `new_number`. With `reseal`, each string and stream is encrypted again under the key of its new number.
fn copy_object(
    src: &[u8],
    object: &Object,
    numbers: (u32, u32),
    new_number: &impl Fn(u32) -> Result<u32, String>,
    reseal: Option<&FileCipher>,
) -> Result<Vec<u8>, String> {
    let (old, number) = numbers;
    let generation = object.generation;
    let mut out = format!("{number} {generation} obj").into_bytes();
    let mut at = object.body.start;
    for (range, part) in &object.parts {
        out.extend_from_slice(&src[at..range.start]);
        let text = &src[range.clone()];
        match (part, reseal) {
            (Part::Reference(target), _) => out.extend(new_number(*target)?.to_string().bytes()),
            (Part::String, Some(cipher)) => {
                let (sealed, _) = match text.first() {
                    Some(b'(') => read_literal_string(src, range.start),
                    _ => read_hex_string(src, range.start),
                }
                .ok_or_else(|| format!("object {old}: a string that does not read"))?;
                let resealed = cipher.reseal((old, generation), (number, generation), &sealed);
                out.push(b'<');
                out.extend(resealed.iter().flat_map(|byte| format!("{byte:02X}").into_bytes()));
                out.push(b'>');
            }
            (Part::StreamData, Some(cipher)) => out.extend(cipher.reseal((old, generation), (number, generation), text)),
            (Part::String | Part::StreamData, None) => out.extend_from_slice(text),
        }
        at = range.end;
    }
    out.extend_from_slice(&src[at..object.body.end]);
    out.push(b'\n');
    Ok(out)
}

/// The entries of the trailer that the new file keeps, with their new numbers.
struct Ids {
    root:    u32,
    info:    Option<u32>,
    encrypt: Option<u32>,
    /// `/ID` as it is written, `[<…> <…>]`, byte for byte: a literal string of it can hold any byte.
    id:      Option<Vec<u8>>,
}

impl Ids {
    fn of(file: &File<'_>, new_number: &impl Fn(u32) -> Result<u32, String>) -> Result<Self, String> {
        let id = match file.trailer.get(b"ID") {
            Some(Value::Array(items)) => {
                let mut id = b"[".to_vec();
                for (index, item) in items.iter().enumerate() {
                    let Value::String(span) = item else { return Err("an /ID of the trailer that is not two strings".to_string()) };
                    if index > 0 {
                        id.push(b' ');
                    }
                    id.extend_from_slice(&file.src[span.clone()]);
                }
                id.push(b']');
                Some(id)
            }
            _ => None,
        };
        Ok(Self {
            root:    new_number(file.root)?,
            info:    file.trailer.reference(b"Info").map(new_number).transpose()?,
            encrypt: file.trailer.reference(b"Encrypt").map(new_number).transpose()?,
            id,
        })
    }
}

/// The values of the linearization dictionary.
struct Numbers {
    file_length:           usize,
    hint_offset:           usize,
    hint_length:           usize,
    first_page:            u32,
    first_page_end:        usize,
    pages:                 usize,
    main_xref_first_entry: usize,
}

impl Numbers {
    /// Each value at the widest it can be written, for the length the dictionary is kept to.
    fn widest() -> Self {
        Self {
            file_length:           u32::MAX as usize,
            hint_offset:           u32::MAX as usize,
            hint_length:           u32::MAX as usize,
            first_page:            u32::MAX,
            first_page_end:        u32::MAX as usize,
            pages:                 u32::MAX as usize,
            main_xref_first_entry: u32::MAX as usize,
        }
    }
}

/// What ends the linearization dictionary, after the spaces that keep it to its length.
const DICTIONARY_END: &[u8] = b" >>\nendobj\n";

/// The linearization dictionary, up to the spaces before its closing `>>`.
fn linearization_dictionary(number: u32, values: &Numbers) -> Vec<u8> {
    format!(
        "{number} 0 obj\n<< /Linearized 1 /L {} /H [{} {}] /O {} /E {} /N {} /T {}",
        values.file_length, values.hint_offset, values.hint_length, values.first_page, values.first_page_end,
        values.pages, values.main_xref_first_entry,
    )
    .into_bytes()
}

/// The cross-reference section of the first page, `entries` the offset and generation of each of its objects, and its
/// trailer. `/Prev`, where the main section is, is kept to a fixed width, so that without it the length is known.
fn first_xref(start: u32, size: u32, entries: &[(usize, u32)], ids: &Ids, prev: Option<usize>) -> Vec<u8> {
    let mut out = format!("xref\n{start} {}\n", size - start).into_bytes();
    for (offset, generation) in entries {
        out.extend(format!("{offset:010} {generation:05} n\r\n").bytes());
    }
    out.extend(format!("trailer\n<< /Size {size} /Root {} 0 R", ids.root).bytes());
    if let Some(info) = ids.info {
        out.extend(format!(" /Info {info} 0 R").bytes());
    }
    if let Some(encrypt) = ids.encrypt {
        out.extend(format!(" /Encrypt {encrypt} 0 R").bytes());
    }
    if let Some(id) = &ids.id {
        out.extend(b" /ID ");
        out.extend(id);
    }
    let prev = prev.map_or_else(String::new, |offset| offset.to_string());
    out.extend(format!(" /Prev {prev:<NUMBER_WIDTH$} >>\nstartxref\n0\n%%EOF\n").bytes());
    out
}

/// The main cross-reference section and its trailer.
struct MainXref {
    bytes:       Vec<u8>,
    /// Where in `bytes` the white space before the entry of object 0 is: `/T` of the linearization dictionary.
    first_entry: usize,
}

/// The main section, whose objects are `objects` from 1, and its trailer. The file's last `startxref` gives
/// `first_xref`, where the first page's section is, whose trailer leads back here with `/Prev`.
fn main_xref(size: u32, objects: &[Written], offset_of: &impl Fn(u32) -> usize, ids: &Ids, first_xref: usize) -> MainXref {
    let mut bytes = format!("xref\n0 {size}").into_bytes();
    let first_entry = bytes.len();
    bytes.extend(b"\n0000000000 65535 f\r\n");
    for object in objects {
        bytes.extend(format!("{:010} {:05} n\r\n", offset_of(object.number), object.generation).bytes());
    }
    bytes.extend(format!("trailer\n<< /Size {size}").bytes());
    if let Some(id) = &ids.id {
        bytes.extend(b" /ID ");
        bytes.extend(id);
    }
    bytes.extend(format!(" >>\nstartxref\n{first_xref}\n%%EOF\n").bytes());
    MainXref { bytes, first_entry }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use pdf_writer::{Pdf, Rect, Ref};

    use super::*;

    /// A file of `pages` empty pages, written by pdf-writer as the engine writes one, with a file ID.
    fn file(pages: usize) -> Vec<u8> {
        let mut pdf = Pdf::new();
        let catalog = Ref::new(1);
        let tree = Ref::new(2);
        let page_ids: Vec<Ref> = (0..pages).map(|index| Ref::new(3 + index as i32)).collect();
        pdf.catalog(catalog).pages(tree);
        pdf.pages(tree).kids(page_ids.iter().copied()).count(pages as i32);
        for page in &page_ids {
            pdf.page(*page).parent(tree).media_box(Rect::new(0.0, 0.0, 100.0, 100.0));
        }
        pdf.set_file_id((vec![0x80, 0x00, 0xFF, 0x9C], vec![0x01, 0x02]));
        pdf.finish()
    }

    #[test]
    fn both_trailers_keep_the_file_id_as_it_is_written() {
        let src = file(2);
        let original = &src[src.windows(4).position(|w| w == b"/ID ").unwrap()..];
        let id = &original[..original.iter().position(|&b| b == b']').unwrap() + 1];
        let linearized = linearize(&src, None).unwrap();
        assert_eq!(linearized.windows(id.len()).filter(|w| *w == id).count(), 2, "{}", String::from_utf8_lossy(id));
    }

    #[test]
    fn the_dictionary_says_the_files_length_and_its_pages() {
        let src = file(3);
        let linearized = linearize(&src, None).unwrap();
        let text = String::from_utf8_lossy(&linearized);
        assert!(text.contains(&format!("/L {} ", linearized.len())), "{text}");
        assert!(text.contains(" /N 3 "), "{text}");
    }

    #[test]
    fn a_file_without_pages_is_refused() {
        let mut pdf = Pdf::new();
        pdf.catalog(Ref::new(1)).pages(Ref::new(2));
        pdf.pages(Ref::new(2)).count(0);
        let error = linearize(&pdf.finish(), None).unwrap_err();
        assert!(error.contains("no pages"), "{error}");
    }
}
