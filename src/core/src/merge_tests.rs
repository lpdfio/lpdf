//! Merging existing PDFs, end to end: the pages in the order of the files at the size they have as shown, what
//! crosses over (content, resources, appearances, layers) and what is dropped, the errors that name the file, and the
//! options. The files merged are built here with `lopdf`, a page of a few words each so that a review sees which page
//! is which, or rendered by Lpdf with an owner password; the corpus of real files is `test/merge-corpus`.

use lopdf::xref::XrefType;
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};
use miniz_oxide::deflate::compress_to_vec_zlib;
use serde_json::json;

use crate::merge::{merge, Options, Source};
use crate::{LpdfEngine, RenderEncryption};

// ── Inputs ────────────────────────────────────────────────────────────────────

const LETTER: [f32; 4] = [0.0, 0.0, 612.0, 792.0];
const A4: [f32; 4] = [0.0, 0.0, 595.0, 842.0];
const SMALL: [f32; 4] = [0.0, 0.0, 300.0, 200.0];

/// A PDF being built: pages with a line of text each, which a test then dresses with annotations, layers and the like.
struct Builder {
    doc:      Document,
    pages_id: ObjectId,
    kids:     Vec<Object>,
    font:     ObjectId,
    catalog:  Dictionary,
}

impl Builder {
    fn new(version: &str) -> Self {
        let mut doc = Document::with_version(version);
        doc.reference_table.cross_reference_type = XrefType::CrossReferenceTable;
        let pages_id = doc.new_object_id();
        let font = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" });
        Builder { doc, pages_id, kids: Vec::new(), font, catalog: dictionary! { "Type" => "Catalog" } }
    }

    /// A page of the size given, with `label` written on it and its own resources.
    fn page(&mut self, media: [f32; 4], label: &str) -> ObjectId {
        let content = self.doc.add_object(Stream::new(dictionary! {}, format!("BT /F1 18 Tf 20 20 Td ({label}) Tj ET").into_bytes()));
        let id = self.doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => self.pages_id, "MediaBox" => rect(media), "Contents" => content,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => self.font } },
        });
        self.kids.push(id.into());
        id
    }

    fn page_dict(&mut self, id: ObjectId) -> &mut Dictionary {
        self.doc.get_dictionary_mut(id).expect("a page")
    }

    /// Add an annotation to a page.
    fn annotate(&mut self, page: ObjectId, annotation: Dictionary) -> ObjectId {
        let id = self.doc.add_object(annotation);
        let dict = self.page_dict(page);
        let mut annots = dict.get(b"Annots").and_then(Object::as_array).cloned().unwrap_or_default();
        annots.push(id.into());
        dict.set("Annots", annots);
        id
    }

    /// A form XObject for an appearance: `bbox` as its box, drawing the text given.
    fn appearance(&mut self, bbox: [f32; 4], drawing: &str) -> ObjectId {
        self.doc.add_object(Stream::new(
            dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => rect(bbox) },
            drawing.as_bytes().to_vec(),
        ))
    }

    fn save(mut self) -> Vec<u8> {
        let tree = dictionary! { "Type" => "Pages", "Count" => self.kids.len() as i64, "Kids" => self.kids.clone() };
        self.doc.objects.insert(self.pages_id, Object::Dictionary(tree));
        self.catalog.set("Pages", self.pages_id);
        let catalog = self.doc.add_object(self.catalog.clone());
        self.doc.trailer.set("Root", catalog);
        let mut out = Vec::new();
        self.doc.save_to(&mut out).expect("saved");
        out
    }
}

fn rect([a, b, c, d]: [f32; 4]) -> Object {
    vec![a.into(), b.into(), c.into(), d.into()].into()
}

/// A file of pages of the sizes given, each with its number and the file's name written on it.
fn file(name: &str, sizes: &[[f32; 4]]) -> Vec<u8> {
    let mut builder = Builder::new("1.7");
    for (i, size) in sizes.iter().enumerate() {
        builder.page(*size, &format!("{name} page {}", i + 1));
    }
    builder.save()
}

// ── Merging and reading back ──────────────────────────────────────────────────

fn sources(files: &[(&str, &[u8])]) -> Vec<Source> {
    files.iter().map(|(name, pdf)| Source { name: name.to_string(), pdf: pdf.to_vec() }).collect()
}

/// Merge as a licensed engine does, with no attribution line, which must succeed.
fn merged(files: &[(&str, &[u8])], options: Options) -> Vec<u8> {
    merge(&sources(files), &options, true).unwrap_or_else(|e| panic!("merge failed: {e}"))
}

fn merged_unlicensed(files: &[(&str, &[u8])], options: Options) -> Vec<u8> {
    merge(&sources(files), &options, false).unwrap_or_else(|e| panic!("merge failed: {e}"))
}

fn error(files: &[(&str, &[u8])], options: Options) -> String {
    merge(&sources(files), &options, true).err().expect("an error")
}

fn reopen(pdf: &[u8]) -> Document {
    Document::load_mem(pdf).unwrap_or_else(|e| panic!("the merged PDF doesn't load: {e}"))
}

fn text(stream: &Stream) -> String {
    String::from_utf8_lossy(&stream.decompressed_content().unwrap_or_else(|_| stream.content.clone())).into_owned()
}

fn page(doc: &Document, number: u32) -> &Dictionary {
    doc.get_dictionary(doc.get_pages()[&number]).expect("a page")
}

fn numbers(doc: &Document, value: &Object) -> Vec<f32> {
    value.as_array().expect("an array").iter().map(|n| doc.dereference(n).unwrap().1.as_float().expect("a number")).collect()
}

/// A page's size, from its media box.
fn size(doc: &Document, number: u32) -> (f32, f32) {
    let b = numbers(doc, page(doc, number).get(b"MediaBox").expect("a media box"));
    assert_eq!((b[0], b[1]), (0.0, 0.0), "the media box starts at the origin");
    (b[2], b[3])
}

/// The content stream of a page of the output, decoded.
fn content(doc: &Document, number: u32) -> String {
    let id = page(doc, number).get(b"Contents").and_then(Object::as_reference).expect("one content stream");
    text(doc.get_object(id).and_then(Object::as_stream).expect("a stream"))
}

/// The XObject a page of the output draws under `name`.
fn xobject<'a>(doc: &'a Document, number: u32, name: &str) -> &'a Stream {
    let resources = doc.get_dictionary(doc.get_pages()[&number]).and_then(|p| p.get(b"Resources")).and_then(Object::as_dict).expect("resources");
    let id = resources.get(b"XObject").and_then(Object::as_dict).and_then(|x| x.get(name.as_bytes())).and_then(Object::as_reference)
        .unwrap_or_else(|_| panic!("no XObject {name} on page {number}"));
    doc.get_object(id).and_then(Object::as_stream).expect("a form")
}

/// The form XObject that draws a source page.
fn form(doc: &Document, number: u32) -> &Stream {
    xobject(doc, number, "LpdfPage")
}

fn count_of(doc: &Document, kind: &[u8]) -> usize {
    doc.objects.values().filter(|o| o.as_dict().is_ok_and(|d| d.get(b"Type").and_then(Object::as_name).is_ok_and(|t| t == kind))).count()
}

// ── Pages ─────────────────────────────────────────────────────────────────────

#[test]
fn the_pages_come_in_the_order_of_the_files_each_at_its_own_size() {
    let (a, b) = (file("a", &[LETTER, A4]), file("b", &[SMALL]));
    let out = merged(&[("a.pdf", &a), ("b.pdf", &b)], Options::default());
    let doc = reopen(&out);
    assert_eq!(doc.get_pages().len(), 3);
    assert_eq!((size(&doc, 1), size(&doc, 2), size(&doc, 3)), ((612.0, 792.0), (595.0, 842.0), (300.0, 200.0)));
    for (number, label) in [(1, "a page 1"), (2, "a page 2"), (3, "b page 1")] {
        assert!(text(form(&doc, number)).contains(&format!("({label})")), "page {number} draws {label}");
        assert!(content(&doc, number).contains("/LpdfPage Do"));
    }
    assert!(out.starts_with(b"%PDF-1.7"));
}

#[test]
fn the_same_file_twice_gives_its_pages_twice_with_resources_of_their_own() {
    let a = file("a", &[LETTER]);
    let doc = reopen(&merged(&[("a.pdf", &a), ("a-again.pdf", &a)], Options::default()));
    assert_eq!(doc.get_pages().len(), 2);
    let font = |n: u32| {
        let resources = form(&doc, n).dict.get(b"Resources").and_then(Object::as_dict).expect("resources").clone();
        resources.get(b"Font").and_then(Object::as_dict).and_then(|f| f.get(b"F1")).and_then(Object::as_reference).expect("a font")
    };
    assert_ne!(font(1), font(2));
}

#[test]
fn the_pages_of_one_file_share_what_they_share() {
    let mut builder = Builder::new("1.7");
    let resources = builder.doc.add_object(dictionary! { "Font" => dictionary! { "F1" => builder.font } });
    for label in ["one", "two"] {
        let id = builder.page(LETTER, label);
        builder.page_dict(id).set("Resources", resources);
    }
    let doc = reopen(&merged(&[("shared.pdf", &builder.save())], Options::default()));
    let at = |n: u32| form(&doc, n).dict.get(b"Resources").and_then(Object::as_reference).expect("resources, shared");
    assert_eq!(at(1), at(2));
    assert_eq!(count_of(&doc, b"Font"), 1, "the font was copied once");
}

#[test]
fn a_turned_page_is_as_big_as_it_shows_and_a_crop_box_cuts_it() {
    let mut builder = Builder::new("1.7");
    let id = builder.page(LETTER, "turned");
    builder.page_dict(id).set("Rotate", 90);
    builder.page_dict(id).set("CropBox", rect([50.0, 100.0, 350.0, 500.0]));
    let plain = builder.page(LETTER, "plain");
    builder.page_dict(plain).set("CropBox", rect([0.0, 0.0, 200.0, 100.0]));
    let doc = reopen(&merged(&[("t.pdf", &builder.save())], Options::default()));
    // The crop box is 300 wide and 400 high; turned a quarter, it shows 400 wide and 300 high.
    assert_eq!(size(&doc, 1), (400.0, 300.0));
    assert_eq!(numbers(&doc, form(&doc, 1).dict.get(b"BBox").unwrap()), [50.0, 100.0, 350.0, 500.0]);
    assert_eq!(size(&doc, 2), (200.0, 100.0));
    // Turned clockwise: the crop box's bottom-left corner is the shown top-left, which lands at the output's top-left.
    assert!(content(&doc, 1).starts_with("q 0 -1 1 0 -100 350 cm /LpdfPage Do"), "{}", content(&doc, 1));
}

#[test]
fn a_page_takes_its_resources_and_size_from_its_parents() {
    let mut builder = Builder::new("1.7");
    let id = builder.page(LETTER, "inherits");
    let page = builder.page_dict(id);
    page.remove(b"Resources");
    page.remove(b"MediaBox");
    let font = builder.font;
    let tree = builder.pages_id;
    builder.doc.objects.insert(tree, Object::Dictionary(dictionary! {}));
    let mut bytes = builder.save();
    // The page tree node, which `save` writes, gets what its pages inherit.
    let mut doc = reopen(&bytes);
    let pages = doc.catalog().unwrap().get(b"Pages").and_then(Object::as_reference).unwrap();
    let node = doc.get_dictionary_mut(pages).unwrap();
    node.set("Resources", dictionary! { "Font" => dictionary! { "F1" => font } });
    node.set("MediaBox", rect(A4));
    bytes.clear();
    doc.save_to(&mut bytes).unwrap();
    let doc = reopen(&merged(&[("inherits.pdf", &bytes)], Options::default()));
    assert_eq!(size(&doc, 1), (595.0, 842.0));
    let resources = form(&doc, 1).dict.get(b"Resources").and_then(Object::as_dict).expect("resources");
    assert!(resources.get(b"Font").and_then(Object::as_dict).is_ok_and(|f| f.has(b"F1")), "{resources:?}");
}

#[test]
fn several_content_streams_are_joined_and_one_is_kept_as_it_is() {
    let mut builder = Builder::new("1.7");
    let id = builder.page(LETTER, "x");
    let (a, b) = (
        builder.doc.add_object(Stream::new(dictionary! {}, b"0 0 m".to_vec())),
        builder.doc.add_object(Stream::new(dictionary! {}, b"10 10 l S".to_vec())),
    );
    builder.page_dict(id).set("Contents", vec![Object::from(a), Object::from(b)]);
    let packed = compress_to_vec_zlib(b"BT (packed) Tj ET", 6);
    let single = builder.page(LETTER, "y");
    let stream = builder.doc.add_object(Stream::new(dictionary! { "Filter" => "FlateDecode" }, packed.clone()));
    builder.page_dict(single).set("Contents", stream);
    let doc = reopen(&merged(&[("c.pdf", &builder.save())], Options::default()));
    assert_eq!(text(form(&doc, 1)), "0 0 m\n10 10 l S\n");
    assert_eq!(form(&doc, 2).content, packed, "a page with one stream keeps it, filter and all");
    assert_eq!(form(&doc, 2).dict.get(b"Filter").and_then(Object::as_name).unwrap(), b"FlateDecode");
}

#[test]
fn a_page_with_no_content_is_an_empty_page_of_its_size() {
    let mut builder = Builder::new("1.7");
    let id = builder.page(SMALL, "gone");
    builder.page_dict(id).remove(b"Contents");
    let doc = reopen(&merged(&[("empty.pdf", &builder.save())], Options::default()));
    assert_eq!(size(&doc, 1), (300.0, 200.0));
    assert_eq!(text(form(&doc, 1)), "");
}

#[test]
fn a_user_unit_stays_so_the_page_prints_at_its_size() {
    let mut builder = Builder::new("1.7");
    let id = builder.page(LETTER, "big");
    builder.page_dict(id).set("UserUnit", 2);
    let doc = reopen(&merged(&[("u.pdf", &builder.save())], Options::default()));
    assert_eq!(page(&doc, 1).get(b"UserUnit").and_then(Object::as_float).unwrap(), 2.0);
}

#[test]
fn what_ties_a_drawing_to_the_pages_it_came_from_is_left_behind() {
    let mut builder = Builder::new("1.7");
    let id = builder.page(LETTER, "ties");
    let tree = builder.pages_id;
    // A form with a parent and a page of its own, and a font that names its page: nothing a page draws needs them.
    let xobject = builder.doc.add_object(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => rect(SMALL), "Parent" => tree, "P" => id, "StructParents" => 3 },
        b"0 0 m".to_vec(),
    ));
    let font = builder.font;
    builder.doc.get_dictionary_mut(font).unwrap().set("Page", id);
    builder.page_dict(id).set("Resources", dictionary! {
        "Font" => dictionary! { "F1" => font }, "XObject" => dictionary! { "Fm0" => xobject },
    });
    let doc = reopen(&merged(&[("ties.pdf", &builder.save())], Options::default()));
    assert_eq!((count_of(&doc, b"Pages"), count_of(&doc, b"Page")), (1, 1));
    let copy = doc.objects.values().filter_map(|o| o.as_stream().ok()).find(|s| s.dict.has(b"StructParents") || s.dict.has(b"Parent"));
    assert!(copy.is_none(), "the copy of the form has no parent and no structure parents");
}

#[test]
fn the_same_files_merge_to_the_same_bytes() {
    let files: Vec<(String, Vec<u8>)> = synthetic();
    let all: Vec<(&str, &[u8])> = files.iter().map(|(name, bytes)| (name.as_str(), bytes.as_slice())).collect();
    let options = Options { allow_signed: true, allow_restricted: true, double_sided: true };
    assert_eq!(merged(&all, options), merged(&all, options));
}

#[test]
fn resources_that_refer_back_to_themselves_are_copied_once() {
    let mut builder = Builder::new("1.7");
    let id = builder.page(LETTER, "loop");
    let resources = builder.doc.new_object_id();
    // A font that names the resources that name it, and an XObject that is its own resource.
    let font = builder.font;
    builder.doc.get_dictionary_mut(font).unwrap().set("Back", resources);
    let form = builder.doc.new_object_id();
    builder.doc.objects.insert(form, Object::Stream(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => rect(SMALL), "Resources" => resources },
        b"0 0 m".to_vec(),
    )));
    builder.doc.objects.insert(resources, Object::Dictionary(dictionary! {
        "Font" => dictionary! { "F1" => font }, "XObject" => dictionary! { "Fm0" => form },
    }));
    builder.page_dict(id).set("Resources", resources);
    let doc = reopen(&merged(&[("loop.pdf", &builder.save())], Options::default()));
    assert_eq!(doc.get_pages().len(), 1);
    assert_eq!(count_of(&doc, b"Font"), 1);
}

// ── Annotations ───────────────────────────────────────────────────────────────

/// A page with the annotations given, each with a square appearance of 10 by 10 that draws `label`.
fn annotated(annotations: &[(i64, &str, [f32; 4])]) -> Vec<u8> {
    let mut builder = Builder::new("1.7");
    let id = builder.page(LETTER, "annotated");
    for (flags, label, rect_) in annotations {
        let appearance = builder.appearance([0.0, 0.0, 10.0, 10.0], label);
        builder.annotate(id, dictionary! {
            "Type" => "Annot", "Subtype" => "Square", "F" => *flags, "Rect" => rect(*rect_), "AP" => dictionary! { "N" => appearance },
        });
    }
    builder.save()
}

#[test]
fn an_annotation_prints_when_its_print_flag_is_set_and_its_hidden_flag_is_not() {
    let pdf = annotated(&[
        (4, "printed", [100.0, 100.0, 150.0, 130.0]),
        (0, "no flag", [0.0, 0.0, 10.0, 10.0]),
        (6, "hidden", [0.0, 0.0, 10.0, 10.0]),
        (2, "hidden alone", [0.0, 0.0, 10.0, 10.0]),
    ]);
    let doc = reopen(&merged(&[("a.pdf", &pdf)], Options::default()));
    // The appearance, 10 by 10, fitted to its rectangle, 50 by 30 at (100, 100).
    assert!(content(&doc, 1).contains("q 5 0 0 3 100 100 cm /LpdfAnnot0 Do Q"), "{}", content(&doc, 1));
    assert!(!content(&doc, 1).contains("LpdfAnnot1"));
    assert_eq!(text(xobject(&doc, 1, "LpdfAnnot0")), "printed");
}

#[test]
fn the_output_holds_no_annotation_and_a_link_or_a_popup_draws_nothing() {
    let mut builder = Builder::new("1.7");
    let id = builder.page(LETTER, "links");
    builder.annotate(id, dictionary! {
        "Type" => "Annot", "Subtype" => "Link", "F" => 4, "Rect" => rect([0.0, 0.0, 50.0, 10.0]),
        "A" => dictionary! { "S" => "URI", "URI" => Object::string_literal("https://lpdf.io") },
    });
    let appearance = builder.appearance([0.0, 0.0, 10.0, 10.0], "popup");
    builder.annotate(id, dictionary! {
        "Type" => "Annot", "Subtype" => "Popup", "F" => 4, "Rect" => rect([0.0, 0.0, 50.0, 10.0]), "AP" => dictionary! { "N" => appearance },
    });
    let doc = reopen(&merged(&[("l.pdf", &builder.save())], Options::default()));
    assert!(!content(&doc, 1).contains("LpdfAnnot"));
    assert_eq!(count_of(&doc, b"Annot"), 0);
    assert!(doc.objects.values().all(|o| o.as_dict().map_or(true, |d| !d.has(b"Annots"))), "no page names an annotation");
    assert!(doc.objects.values().all(|o| o.as_dict().map_or(true, |d| !d.has(b"A") && !d.has(b"Dest"))), "and nothing is a link");
}

#[test]
fn an_appearance_with_a_matrix_and_a_box_off_the_origin_is_fitted_to_the_rectangle() {
    let mut builder = Builder::new("1.7");
    let id = builder.page(LETTER, "fit");
    let appearance = builder.doc.add_object(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => rect([10.0, 10.0, 20.0, 30.0]), "Matrix" => vec![1.into(), 0.into(), 0.into(), 1.into(), 0.into(), 0.into()] },
        b"0 0 m".to_vec(),
    ));
    builder.annotate(id, dictionary! {
        "Type" => "Annot", "Subtype" => "Stamp", "F" => 4, "Rect" => rect([0.0, 0.0, 100.0, 100.0]), "AP" => dictionary! { "N" => appearance },
    });
    // A box turned a quarter by the appearance's own matrix: 20 wide and 10 high, at (0, 0).
    let turned = builder.doc.add_object(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => rect([0.0, 0.0, 10.0, 20.0]),
            "Matrix" => vec![0.into(), 1.into(), (-1).into(), 0.into(), 0.into(), 0.into()] },
        b"0 0 m".to_vec(),
    ));
    builder.annotate(id, dictionary! {
        "Type" => "Annot", "Subtype" => "Stamp", "F" => 4, "Rect" => rect([200.0, 200.0, 240.0, 220.0]), "AP" => dictionary! { "N" => turned },
    });
    let doc = reopen(&merged(&[("f.pdf", &builder.save())], Options::default()));
    // Box (10, 10)–(20, 30) to (0, 0)–(100, 100): 10 across, 5 up, and moved back by what the box's corner scales to.
    assert!(content(&doc, 1).contains("q 10 0 0 5 -100 -50 cm /LpdfAnnot0 Do Q"), "{}", content(&doc, 1));
    // The turned box spans (-20, 0)–(0, 10): fitted to 40 by 20 at (200, 200), that is 2 across and 2 up, from (240, 200).
    assert!(content(&doc, 1).contains("q 2 0 0 2 240 200 cm /LpdfAnnot1 Do Q"), "{}", content(&doc, 1));
}

#[test]
fn a_check_box_prints_the_state_it_is_in() {
    let mut builder = Builder::new("1.7");
    let id = builder.page(LETTER, "boxes");
    let yes = builder.appearance([0.0, 0.0, 10.0, 10.0], "ticked");
    let no = builder.appearance([0.0, 0.0, 10.0, 10.0], "empty");
    for (state, rect_) in [("Yes", [0.0, 0.0, 10.0, 10.0]), ("Off", [20.0, 0.0, 30.0, 10.0])] {
        builder.annotate(id, dictionary! {
            "Type" => "Annot", "Subtype" => "Widget", "FT" => "Btn", "F" => 4, "Rect" => rect(rect_), "AS" => state,
            "AP" => dictionary! { "N" => dictionary! { "Yes" => yes, "Off" => no } },
        });
    }
    // A box that is on but has no appearance for its state draws nothing, and has no value to lose when it is off.
    builder.annotate(id, dictionary! {
        "Type" => "Annot", "Subtype" => "Widget", "FT" => "Btn", "F" => 4, "Rect" => rect([40.0, 0.0, 50.0, 10.0]), "AS" => "Off", "V" => "Off",
        "AP" => dictionary! { "N" => dictionary! { "Yes" => yes } },
    });
    let doc = reopen(&merged(&[("b.pdf", &builder.save())], Options::default()));
    assert_eq!(text(xobject(&doc, 1, "LpdfAnnot0")), "ticked");
    assert_eq!(text(xobject(&doc, 1, "LpdfAnnot1")), "empty");
    assert!(!content(&doc, 1).contains("LpdfAnnot2"));
}

#[test]
fn a_field_with_a_value_and_no_stored_appearance_is_an_error_that_names_the_file_and_the_field() {
    let build = |value: Object, flags: i64| {
        let mut builder = Builder::new("1.7");
        let id = builder.page(LETTER, "form");
        let parent = builder.doc.add_object(dictionary! { "T" => Object::string_literal("buyer"), "FT" => "Tx" });
        builder.annotate(id, dictionary! {
            "Type" => "Annot", "Subtype" => "Widget", "F" => flags, "Rect" => rect([0.0, 0.0, 100.0, 20.0]),
            "Parent" => parent, "T" => Object::string_literal("name"), "V" => value,
        });
        builder.save()
    };
    let e = error(&[("contract.pdf", &build(Object::string_literal("Ada"), 4))], Options::default());
    assert_eq!(
        e,
        "contract.pdf: page 1: the form field 'buyer.name' has a value but no stored appearance, so it would print blank: \
         open the file in a PDF viewer and save it, which stores the appearance",
    );
    // No value to print, or a field that doesn't print, is no problem.
    merged(&[("a.pdf", &build(Object::string_literal(""), 4))], Options::default());
    merged(&[("b.pdf", &build(Object::string_literal("Ada"), 0))], Options::default());
}

#[test]
fn a_button_that_is_on_with_no_appearance_for_its_state_is_the_same_error() {
    let mut builder = Builder::new("1.7");
    let id = builder.page(LETTER, "form");
    let off = builder.appearance([0.0, 0.0, 10.0, 10.0], "off");
    builder.annotate(id, dictionary! {
        "Type" => "Annot", "Subtype" => "Widget", "FT" => "Btn", "F" => 4, "Rect" => rect([0.0, 0.0, 10.0, 10.0]),
        "T" => Object::string_literal("agree"), "V" => "Yes", "AS" => "Yes", "AP" => dictionary! { "N" => dictionary! { "Off" => off } },
    });
    let e = error(&[("c.pdf", &builder.save())], Options::default());
    assert!(e.starts_with("c.pdf: page 1: the form field 'agree' has a value but no stored appearance"), "{e}");
}

#[test]
fn a_radio_group_with_one_choice_made_is_no_error_when_the_buttons_not_chosen_store_no_off_appearance() {
    let mut builder = Builder::new("1.7");
    let id = builder.page(LETTER, "radio");
    let chosen = builder.appearance([0.0, 0.0, 12.0, 12.0], "0 g 3 3 6 6 re f");
    let other = builder.appearance([0.0, 0.0, 12.0, 12.0], "0 G 0 0 12 12 re S");
    // The group's value, the choice made, is on its parent, and the button not chosen has no appearance for `Off`.
    let group = builder.doc.add_object(dictionary! { "FT" => "Btn", "Ff" => 32768, "T" => Object::string_literal("choice"), "V" => "A" });
    for (state, at, drawn) in [("A", 0.0, chosen), ("B", 30.0, other)] {
        let on = if state == "A" { "A" } else { "Off" };
        builder.annotate(id, dictionary! {
            "Type" => "Annot", "Subtype" => "Widget", "Parent" => group, "F" => 4, "Rect" => rect([at, 0.0, at + 12.0, 12.0]),
            "AS" => on, "AP" => dictionary! { "N" => dictionary! { state => drawn } },
        });
    }
    let doc = reopen(&merged(&[("radio.pdf", &builder.save())], Options::default()));
    // Only the button chosen is drawn.
    assert!(content(&doc, 1).contains("/LpdfAnnot0 Do") && !content(&doc, 1).contains("LpdfAnnot1"));
    assert_eq!(text(xobject(&doc, 1, "LpdfAnnot0")), "0 g 3 3 6 6 re f");
}

#[test]
fn a_button_that_is_on_by_its_own_state_but_has_no_appearance_for_it_is_the_error_whatever_its_parent_says() {
    let mut builder = Builder::new("1.7");
    let id = builder.page(LETTER, "radio");
    let other = builder.appearance([0.0, 0.0, 12.0, 12.0], "0 G 0 0 12 12 re S");
    let group = builder.doc.add_object(dictionary! { "FT" => "Btn", "Ff" => 32768, "T" => Object::string_literal("choice") });
    builder.annotate(id, dictionary! {
        "Type" => "Annot", "Subtype" => "Widget", "Parent" => group, "F" => 4, "Rect" => rect([0.0, 0.0, 12.0, 12.0]),
        "AS" => "A", "AP" => dictionary! { "N" => dictionary! { "B" => other } },
    });
    let e = error(&[("radio.pdf", &builder.save())], Options::default());
    assert!(e.starts_with("radio.pdf: page 1: the form field 'choice' has a value but no stored appearance"), "{e}");
}

#[test]
fn an_appearance_with_no_resources_takes_the_forms_default_resources() {
    let mut builder = Builder::new("1.7");
    let id = builder.page(LETTER, "form");
    let font = builder.font;
    let appearance = builder.appearance([0.0, 0.0, 100.0, 20.0], "BT /Helv 12 Tf (Ada) Tj ET");
    builder.annotate(id, dictionary! {
        "Type" => "Annot", "Subtype" => "Widget", "FT" => "Tx", "F" => 4, "Rect" => rect([0.0, 0.0, 100.0, 20.0]),
        "V" => Object::string_literal("Ada"), "AP" => dictionary! { "N" => appearance },
    });
    builder.catalog.set("AcroForm", dictionary! { "Fields" => Vec::<Object>::new(), "DR" => dictionary! { "Font" => dictionary! { "Helv" => font } } });
    let doc = reopen(&merged(&[("f.pdf", &builder.save())], Options::default()));
    let resources = xobject(&doc, 1, "LpdfAnnot0").dict.get(b"Resources").and_then(Object::as_dict).expect("resources").clone();
    assert!(resources.get(b"Font").and_then(Object::as_dict).is_ok_and(|f| f.has(b"Helv")), "{resources:?}");
}

// ── Layers ────────────────────────────────────────────────────────────────────

/// A file with three layers: `on` is on, `off` is in the default configuration's `OFF`, and `quiet` is on but its
/// usage says it doesn't print, which the print entry of the configuration applies.
fn layered(base_state: &str) -> Vec<u8> {
    let mut builder = Builder::new("1.7");
    let layer = |builder: &mut Builder, name: &str, usage: Option<&str>| {
        let mut group = dictionary! { "Type" => "OCG", "Name" => Object::string_literal(name) };
        if let Some(state) = usage {
            group.set("Usage", dictionary! { "Print" => dictionary! { "PrintState" => state }, "View" => dictionary! { "ViewState" => "ON" } });
        }
        builder.doc.add_object(group)
    };
    let (on, off, quiet) = (layer(&mut builder, "on", None), layer(&mut builder, "off", None), layer(&mut builder, "quiet", Some("OFF")));
    let id = builder.page(LETTER, "layers");
    let font = builder.font;
    builder.page_dict(id).set("Resources", dictionary! {
        "Font" => dictionary! { "F1" => font },
        "Properties" => dictionary! { "MC0" => on, "MC1" => off, "MC2" => quiet },
    });
    let mut config = dictionary! {
        "BaseState" => base_state, "ON" => vec![Object::from(on)], "OFF" => vec![Object::from(off)],
        "AS" => vec![dictionary! { "Event" => "Print", "Category" => vec![Object::from("Print")], "OCGs" => vec![Object::from(quiet)] }.into()],
    };
    if base_state == "OFF" {
        config.set("OFF", Vec::<Object>::new());
    }
    builder.catalog.set("OCProperties", dictionary! { "OCGs" => vec![Object::from(on), Object::from(off), Object::from(quiet)], "D" => config });
    builder.save()
}

fn layers_of(doc: &Document) -> (Vec<ObjectId>, Vec<ObjectId>, Vec<ObjectId>) {
    let properties = doc.catalog().unwrap().get(b"OCProperties").and_then(Object::as_dict).expect("OCProperties").clone();
    let ids = |dict: &Dictionary, key: &[u8]| -> Vec<ObjectId> {
        dict.get(key).and_then(Object::as_array).expect("a list").iter().map(|o| o.as_reference().unwrap()).collect()
    };
    let config = properties.get(b"D").and_then(Object::as_dict).unwrap();
    (ids(&properties, b"OCGs"), ids(config, b"ON"), ids(config, b"OFF"))
}

#[test]
fn a_layer_is_on_or_off_in_the_output_as_it_prints_in_its_source() {
    let doc = reopen(&merged(&[("l.pdf", &layered("ON"))], Options::default()));
    let (groups, on, off) = layers_of(&doc);
    assert_eq!((groups.len(), on.len(), off.len()), (3, 1, 2));
    let name = |id: ObjectId| doc.get_dictionary(id).and_then(|g| g.get(b"Name")).and_then(Object::as_str).map(<[u8]>::to_vec).unwrap();
    assert_eq!(name(on[0]), b"on");
    let mut hidden: Vec<Vec<u8>> = off.iter().map(|id| name(*id)).collect();
    hidden.sort();
    assert_eq!(hidden, [b"off".to_vec(), b"quiet".to_vec()], "one is off by default, one by its print usage");
    // The layers' usage was the source's own view, and the output's configuration says it all.
    assert!(groups.iter().all(|id| !doc.get_dictionary(*id).unwrap().has(b"Usage")));
    let config = doc.catalog().unwrap().get(b"OCProperties").and_then(Object::as_dict).and_then(|p| p.get(b"D")).and_then(Object::as_dict).unwrap();
    assert!(!config.has(b"AS"));
    // The page's content still names its layers, now the copies.
    let properties = form(&doc, 1).dict.get(b"Resources").and_then(Object::as_dict).and_then(|r| r.get(b"Properties")).and_then(Object::as_dict).unwrap();
    for key in ["MC0", "MC1", "MC2"] {
        assert!(groups.contains(&properties.get(key.as_bytes()).and_then(Object::as_reference).unwrap()), "{key}");
    }
}

#[test]
fn a_layer_in_a_configuration_that_starts_off_is_on_only_when_it_is_named() {
    let doc = reopen(&merged(&[("l.pdf", &layered("OFF"))], Options::default()));
    let (groups, on, off) = layers_of(&doc);
    assert_eq!((groups.len(), on.len(), off.len()), (3, 1, 2));
}

#[test]
fn the_layers_of_two_files_stay_apart() {
    let (a, b) = (layered("ON"), layered("OFF"));
    let doc = reopen(&merged(&[("a.pdf", &a), ("b.pdf", &b)], Options::default()));
    let (groups, on, off) = layers_of(&doc);
    assert_eq!((groups.len(), on.len(), off.len()), (6, 2, 4));
}

#[test]
fn an_annotation_in_a_layer_is_drawn_in_it() {
    let mut builder = Builder::new("1.7");
    let group = builder.doc.add_object(dictionary! { "Type" => "OCG", "Name" => Object::string_literal("notes") });
    let id = builder.page(LETTER, "notes");
    let appearance = builder.appearance([0.0, 0.0, 10.0, 10.0], "note");
    builder.annotate(id, dictionary! {
        "Type" => "Annot", "Subtype" => "Square", "F" => 4, "Rect" => rect([0.0, 0.0, 10.0, 10.0]), "AP" => dictionary! { "N" => appearance }, "OC" => group,
    });
    builder.catalog.set("OCProperties", dictionary! { "OCGs" => vec![Object::from(group)], "D" => dictionary! { "OFF" => vec![Object::from(group)] } });
    let doc = reopen(&merged(&[("n.pdf", &builder.save())], Options::default()));
    assert!(content(&doc, 1).contains("/OC /LpdfLayer0 BDC q 1 0 0 1 0 0 cm /LpdfAnnot0 Do Q EMC"), "{}", content(&doc, 1));
    let (groups, on, off) = layers_of(&doc);
    assert_eq!((groups.len(), on.len(), off.len()), (1, 0, 1));
    let properties = page(&doc, 1).get(b"Resources").and_then(Object::as_dict).and_then(|r| r.get(b"Properties")).and_then(Object::as_dict).unwrap();
    assert_eq!(properties.get(b"LpdfLayer0").and_then(Object::as_reference).unwrap(), groups[0]);
}

// ── The version ───────────────────────────────────────────────────────────────

fn versioned(version: &str) -> Vec<u8> {
    let mut builder = Builder::new(version);
    builder.page(LETTER, version);
    builder.save()
}

#[test]
fn the_version_is_the_highest_among_the_inputs_and_the_file_reads_in_it() {
    let (old, mid, new) = (versioned("1.3"), versioned("1.6"), versioned("2.0"));
    assert!(merged(&[("old.pdf", &old)], Options::default()).starts_with(b"%PDF-1.3"));
    assert!(merged(&[("old.pdf", &old), ("mid.pdf", &mid)], Options::default()).starts_with(b"%PDF-1.6"));
    assert!(merged(&[("new.pdf", &new), ("old.pdf", &old)], Options::default()).starts_with(b"%PDF-2.0"));
    // A 1.3 file has a cross-reference table, since a stream for it is for 1.5 and later.
    let out = merged(&[("old.pdf", &old)], Options::default());
    assert!(!String::from_utf8_lossy(&out).contains("/Type /XRef") && !String::from_utf8_lossy(&out).contains("/XRef"));
    assert_eq!(reopen(&out).get_pages().len(), 1);
}

#[test]
fn the_version_the_catalog_names_counts_as_the_files() {
    let mut builder = Builder::new("1.4");
    builder.page(LETTER, "x");
    builder.catalog.set("Version", "1.7");
    assert!(merged(&[("v.pdf", &builder.save())], Options::default()).starts_with(b"%PDF-1.7"));
}

// ── Errors ────────────────────────────────────────────────────────────────────

/// A file Lpdf renders with `allow` and an owner password, and with an open password too when `open` is given.
fn locked(allow: &str, open: Option<&str>) -> Vec<u8> {
    let xml = format!(r#"<lpdf><document><output encrypt="aes-256" allow="{allow}"/><section><layout><column>
        <text>Locked.</text></column></layout></section></document></lpdf>"#);
    let encryption = RenderEncryption {
        user_password: open.map(str::to_string),
        owner_password: Some("owner".to_string()),
        skip_encryption: false,
        entropy: vec![7; 64],
    };
    LpdfEngine::render_xml_with_encryption(&xml, &encryption).expect("renders")
}

#[test]
fn a_file_that_is_not_a_pdf_or_cannot_be_read_is_an_error_that_names_it() {
    let good = file("good", &[LETTER]);
    assert_eq!(
        error(&[("good.pdf", &good), ("notes.txt", b"hello")], Options::default()),
        "notes.txt: the file is not a PDF: it doesn't start with %PDF-",
    );
    let e = error(&[("cut.pdf", &good[..good.len() / 2])], Options::default());
    assert!(e.starts_with("cut.pdf: the PDF could not be read"), "{e}");
}

#[test]
fn a_file_with_no_pages_and_a_call_with_no_files_are_errors() {
    let mut builder = Builder::new("1.7");
    let _ = &mut builder;
    let e = error(&[("none.pdf", &builder.save())], Options::default());
    assert_eq!(e, "none.pdf: the PDF has no pages");
    assert_eq!(merge(&[], &Options::default(), true).err().unwrap(), "no PDFs were given: give merge at least one");
}

#[test]
fn a_file_that_needs_a_password_to_open_is_an_error_that_names_it() {
    let good = file("good", &[LETTER]);
    let e = error(&[("good.pdf", &good), ("contract-b.pdf", &locked("all", Some("open")))], Options::default());
    assert_eq!(e, "contract-b.pdf: the PDF needs a password to open, and a merge can't open it: remove the password first");
}

#[test]
fn an_owner_restriction_is_refused_unless_allowed_and_a_file_that_allows_it_all_merges() {
    let restricted = locked("print", None);
    let e = error(&[("limited.pdf", &restricted)], Options::default());
    assert_eq!(e, "limited.pdf: the owner restricts printing or copying: set the allow-restricted option to merge it anyway");
    // The option merges it, and the output is open: no encryption, and the page is there.
    let out = merged(&[("limited.pdf", &restricted)], Options { allow_restricted: true, ..Options::default() });
    let doc = reopen(&out);
    assert!(!doc.is_encrypted() && !String::from_utf8_lossy(&out).contains("/Encrypt"));
    assert_eq!(doc.get_pages().len(), 1);
    assert!(text(form(&doc, 1)).contains("BT"), "the page's text is drawn: {}", text(form(&doc, 1)));
    // Printing and copying allowed: the owner password restricts nothing that matters here.
    merged(&[("open.pdf", &locked("print copy", None))], Options::default());
    assert_eq!(reopen(&merged(&[("open.pdf", &locked("all", None))], Options::default())).get_pages().len(), 1);
}

#[test]
fn a_signature_is_refused_unless_allowed_and_an_empty_signature_field_is_not_one() {
    let build = |signed: bool, certified: bool| {
        let mut builder = Builder::new("1.7");
        builder.page(LETTER, "signed");
        let mut field = dictionary! { "FT" => "Sig", "T" => Object::string_literal("sig"), "Subtype" => "Widget", "Rect" => rect([0.0, 0.0, 0.0, 0.0]) };
        if signed {
            field.set("V", dictionary! {
                "Type" => "Sig", "Filter" => "Adobe.PPKLite", "ByteRange" => vec![0.into(), 1.into(), 2.into(), 3.into()],
                "Contents" => Object::String(vec![0; 8], lopdf::StringFormat::Hexadecimal),
            });
        }
        let field = builder.doc.add_object(field);
        builder.catalog.set("AcroForm", dictionary! { "Fields" => vec![Object::from(field)], "SigFlags" => 3 });
        if certified {
            builder.catalog.set("Perms", dictionary! { "DocMDP" => dictionary! { "Type" => "Sig" } });
        }
        builder.save()
    };
    let signed = build(true, false);
    assert_eq!(
        error(&[("signed.pdf", &signed)], Options::default()),
        "signed.pdf: the file is signed, and a merge can't keep a signature, which covers the bytes of its own file: \
         set the allow-signed option to merge it without the signature",
    );
    assert!(error(&[("certified.pdf", &build(false, true))], Options::default()).starts_with("certified.pdf: the file is signed"));
    let out = merged(&[("signed.pdf", &signed)], Options { allow_signed: true, ..Options::default() });
    let doc = reopen(&out);
    assert_eq!(doc.get_pages().len(), 1);
    let text = String::from_utf8_lossy(&out);
    assert!(!text.contains("ByteRange") && !text.contains("AcroForm"), "the signature and the form are gone");
    // A signature field with no signature in it is no signature.
    merged(&[("empty.pdf", &build(false, false))], Options::default());
}

#[test]
fn an_error_in_a_late_page_names_the_file_and_the_page() {
    let mut builder = Builder::new("1.7");
    builder.page(LETTER, "fine");
    let id = builder.page(LETTER, "form");
    builder.annotate(id, dictionary! {
        "Type" => "Annot", "Subtype" => "Widget", "FT" => "Tx", "F" => 4, "Rect" => rect([0.0, 0.0, 10.0, 10.0]), "V" => Object::string_literal("x"),
    });
    let e = error(&[("book.pdf", &builder.save())], Options::default());
    assert!(e.starts_with("book.pdf: page 2: a form field has a value but no stored appearance"), "{e}");
}

#[test]
fn a_call_takes_at_most_a_fixed_number_of_files() {
    let one = file("one", &[LETTER]);
    let many: Vec<(&str, &[u8])> = (0..crate::merge::MAX_SOURCES + 1).map(|_| ("one.pdf", one.as_slice())).collect();
    assert_eq!(error(&many, Options::default()), format!("merge takes {} PDFs at most, and {} were given", crate::merge::MAX_SOURCES, crate::merge::MAX_SOURCES + 1));
}

// ── Printing on both sides of the sheet ───────────────────────────────────────

#[test]
fn double_sided_adds_a_blank_side_after_a_document_with_an_odd_number_of_pages_and_none_after_the_last() {
    let (a, b, c) = (file("a", &[LETTER, LETTER, A4]), file("b", &[SMALL, SMALL]), file("c", &[SMALL]));
    let files: [(&str, &[u8]); 3] = [("a.pdf", &a), ("b.pdf", &b), ("c.pdf", &c)];
    let plain = reopen(&merged(&files, Options::default()));
    assert_eq!(plain.get_pages().len(), 6);
    let doc = reopen(&merged_unlicensed(&files, Options { double_sided: true, ..Options::default() }));
    assert_eq!(doc.get_pages().len(), 7);
    // After a's three pages: one the size of the page before it, truly blank, with no attribution line.
    assert_eq!(size(&doc, 4), (595.0, 842.0));
    assert!(page(&doc, 4).get(b"Contents").is_err());
    assert_eq!(size(&doc, 5), (300.0, 200.0));
    assert_eq!(size(&doc, 7), (300.0, 200.0), "c's page is the last: no blank side after it");
    assert!(page(&doc, 7).get(b"Contents").is_ok());
}

#[test]
fn a_blank_side_has_the_user_unit_of_the_page_before_it() {
    let mut builder = Builder::new("1.7");
    let id = builder.page(LETTER, "big units");
    builder.page_dict(id).set("UserUnit", 2);
    let other = file("b", &[SMALL]);
    let doc = reopen(&merged(&[("u.pdf", &builder.save()), ("b.pdf", &other)], Options { double_sided: true, ..Options::default() }));
    assert_eq!(doc.get_pages().len(), 3);
    assert_eq!(page(&doc, 2).get(b"UserUnit").and_then(Object::as_float).unwrap(), 2.0);
    assert!(page(&doc, 3).get(b"UserUnit").is_err(), "a page with no unit has none");
}

#[test]
fn double_sided_adds_nothing_when_every_document_has_an_even_number_of_pages() {
    let (a, b) = (file("a", &[LETTER, LETTER]), file("b", &[SMALL, SMALL, SMALL, SMALL]));
    let doc = reopen(&merged(&[("a.pdf", &a), ("b.pdf", &b)], Options { double_sided: true, ..Options::default() }));
    assert_eq!(doc.get_pages().len(), 6);
}

// ── The attribution line ──────────────────────────────────────────────────────

#[test]
fn without_a_key_each_merged_page_gets_the_line_in_its_top_right_corner_and_no_link() {
    let (a, b) = (file("a", &[LETTER, A4]), file("b", &[SMALL]));
    let files: [(&str, &[u8]); 2] = [("a.pdf", &a), ("b.pdf", &b)];
    let (with, without) = (reopen(&merged(&files, Options::default())), reopen(&merged_unlicensed(&files, Options::default())));
    for number in 1..=3 {
        let (plain, line) = (content(&with, number), content(&without, number));
        assert!(line.starts_with(&plain) && line.len() > plain.len(), "page {number}: the line is drawn after the page");
        assert!(line[plain.len()..].contains("rg"), "page {number}: in a color");
    }
    // The line is placed at the page's top right, for the page's own size.
    let [_, _, _, _, e, f] = {
        let extra = content(&without, 1)[content(&with, 1).len()..].to_string();
        let numbers: Vec<f32> = extra.split(" cm ").next().unwrap().trim_start_matches("q ").split(' ').filter_map(|n| n.parse().ok()).collect();
        [numbers[0], numbers[1], numbers[2], numbers[3], numbers[4], numbers[5]]
    };
    assert!(e > 612.0 - 12.0 - 120.0 && e < 612.0 - 12.0, "the line's left edge is inside the right margin: {e}");
    assert!(f > 792.0 - 12.0 - 20.0 && f < 792.0, "and near the top: {f}");
    assert_eq!(count_of(&without, b"Annot"), 0, "no link");
    assert!(without.objects.values().all(|o| o.as_dict().map_or(true, |d| !d.has(b"Annots"))));
}

// ── Through the engine ────────────────────────────────────────────────────────

#[test]
fn the_engine_cuts_the_files_from_their_bytes_and_reads_its_options() {
    let (a, b) = (file("a", &[LETTER]), file("b", &[SMALL]));
    let pdfs = [a.as_slice(), b.as_slice()].concat();
    let index = json!([{ "name": "a.pdf", "length": a.len() }, { "name": "b.pdf", "length": b.len() }]).to_string();
    let engine = LpdfEngine::new("");
    let out = engine.merge_pdf(&pdfs, &index, r#"{"double-sided":true}"#).expect("merges");
    assert_eq!(reopen(&out).get_pages().len(), 3, "a's one page is odd, so a blank side follows it");
    assert_eq!(engine.merge_pdf(&pdfs, &index, "").map(|o| reopen(&o).get_pages().len()), Ok(2));
    assert_eq!(engine.merge_pdf(&pdfs, &index, r#"{"nope":1}"#).err().unwrap(), "the merge has no option 'nope'");
    assert!(engine.merge_pdf(&pdfs, "not json", "").err().unwrap().starts_with("the merge's files could not be read"));
    assert_eq!(
        engine.merge_pdf(&pdfs, &json!([{ "name": "a.pdf", "length": a.len() + b.len() + 1 }]).to_string(), "").err().unwrap(),
        "file 1 (a.pdf) is longer than the bytes sent",
    );
}

// ── The samples the external check reads ──────────────────────────────────────

/// The real files of the corpus, `test/merge-corpus/*.pdf`, by name.
fn corpus() -> Vec<(String, Vec<u8>)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/merge-corpus");
    let mut files: Vec<(String, Vec<u8>)> = std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|e| e == "pdf"))
        .map(|entry| (entry.file_name().to_string_lossy().into_owned(), std::fs::read(entry.path()).expect("a corpus file")))
        .collect();
    files.sort();
    files
}

#[test]
fn lopdf_reads_every_file_of_the_corpus_and_the_merge_takes_each() {
    let files = corpus();
    assert!(files.len() >= 11, "the corpus has its files: {}", files.len());
    for (name, bytes) in &files {
        let doc = Document::load_mem(bytes).unwrap_or_else(|e| panic!("{name}: lopdf can't read it: {e}"));
        assert!(!doc.is_encrypted() && !doc.get_pages().is_empty(), "{name}: opens, and has pages");
        let options = Options { allow_signed: true, ..Options::default() };
        let out = merge(&sources(&[(name.as_str(), bytes.as_slice())]), &options, true).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(reopen(&out).get_pages().len(), doc.get_pages().len(), "{name}: every page is merged");
    }
}

#[test]
fn a_signed_file_of_the_corpus_is_refused_until_it_is_allowed() {
    // The Government Publishing Office signs what govinfo serves.
    let (name, bytes) = corpus().into_iter().find(|(name, _)| name.starts_with("govinfo-")).expect("a signed file");
    let e = error(&[(name.as_str(), bytes.as_slice())], Options::default());
    assert!(e.starts_with(&format!("{name}: the file is signed")), "{e}");
}

/// A page's content replaced by `drawing`, which may name the layers `MC0`, `MC1` and `MC2` and the font `F1`.
fn draw(builder: &mut Builder, id: ObjectId, drawing: &str) {
    let content = builder.doc.add_object(Stream::new(dictionary! {}, drawing.as_bytes().to_vec()));
    builder.page_dict(id).set("Contents", content);
}

/// A square annotation of the flags given, which draws a filled square of `color`.
fn square(builder: &mut Builder, page: ObjectId, flags: i64, color: &str, at: [f32; 2]) {
    let appearance = builder.appearance([0.0, 0.0, 20.0, 20.0], &format!("{color} rg 0 0 20 20 re f 0 g 0 0 20 20 re S"));
    builder.annotate(page, dictionary! {
        "Type" => "Annot", "Subtype" => "Square", "F" => flags, "Rect" => rect([at[0], at[1], at[0] + 40.0, at[1] + 40.0]),
        "AP" => dictionary! { "N" => appearance },
    });
}

/// A page of annotations of each flag combination: red prints, blue has no flag, green is hidden but prints, gray is
/// hidden alone, and a link with a border. Only the red ones are in the merged page.
fn annotations_sample() -> Vec<u8> {
    let mut builder = Builder::new("1.7");
    let id = builder.page(LETTER, "annotations: two red squares print, nothing else does");
    square(&mut builder, id, 4, "1 0 0", [72.0, 600.0]);
    square(&mut builder, id, 4, "0.8 0 0", [172.0, 600.0]);
    square(&mut builder, id, 0, "0 0 1", [72.0, 500.0]);
    square(&mut builder, id, 6, "0 0.6 0", [172.0, 500.0]);
    square(&mut builder, id, 2, "0.5 0.5 0.5", [272.0, 500.0]);
    builder.annotate(id, dictionary! {
        "Type" => "Annot", "Subtype" => "Link", "F" => 4, "Rect" => rect([72.0, 400.0, 250.0, 430.0]), "Border" => vec![1.into(), 1.into(), 3.into()],
        "A" => dictionary! { "S" => "URI", "URI" => Object::string_literal("https://lpdf.io") },
    });
    // An appearance whose box is far from the origin and turned by its own matrix, placed by its rectangle.
    let turned = builder.doc.add_object(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => rect([100.0, 100.0, 140.0, 120.0]),
            "Matrix" => vec![0.into(), 1.into(), (-1).into(), 0.into(), 0.into(), 0.into()] },
        b"1 0.5 0 rg 100 100 40 20 re f 0 g 100 100 40 20 re S".to_vec(),
    ));
    builder.annotate(id, dictionary! {
        "Type" => "Annot", "Subtype" => "Stamp", "F" => 4, "Rect" => rect([72.0, 250.0, 132.0, 330.0]), "AP" => dictionary! { "N" => turned },
    });
    builder.save()
}

/// A form with a filled text field, an empty one, a ticked check box, an empty one and a field that doesn't print.
fn form_sample() -> Vec<u8> {
    let mut builder = Builder::new("1.7");
    let id = builder.page(LETTER, "form: Ada Lovelace, ticked");
    let font = builder.font;
    let field = |builder: &mut Builder, name: &str, value: Option<&str>, at: [f32; 4], flags: i64| {
        let shown = value.map(|v| format!("BT /Helv 12 Tf 4 6 Td ({v}) Tj ET")).unwrap_or_default();
        let appearance = builder.doc.add_object(Stream::new(
            dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => rect([0.0, 0.0, at[2] - at[0], at[3] - at[1]]),
                "Resources" => dictionary! { "Font" => dictionary! { "Helv" => font } } },
            format!("0.9 g 0 0 {} {} re f 0 G 0 0 {} {} re S {shown}", at[2] - at[0], at[3] - at[1], at[2] - at[0], at[3] - at[1]).into_bytes(),
        ));
        let mut dict = dictionary! {
            "Type" => "Annot", "Subtype" => "Widget", "FT" => "Tx", "T" => Object::string_literal(name), "F" => flags, "Rect" => rect(at),
            "AP" => dictionary! { "N" => appearance },
        };
        if let Some(v) = value {
            dict.set("V", Object::string_literal(v));
        }
        builder.annotate(id, dict);
    };
    field(&mut builder, "name", Some("Ada Lovelace"), [72.0, 600.0, 272.0, 624.0], 4);
    field(&mut builder, "city", None, [72.0, 560.0, 272.0, 584.0], 4);
    field(&mut builder, "secret", Some("not printed"), [72.0, 520.0, 272.0, 544.0], 0);
    let ticked = builder.appearance([0.0, 0.0, 16.0, 16.0], "0 G 0 0 16 16 re S 3 8 m 7 3 l 13 13 l S");
    let empty = builder.appearance([0.0, 0.0, 16.0, 16.0], "0 G 0 0 16 16 re S");
    for (state, x) in [("Yes", 72.0), ("Off", 112.0)] {
        builder.annotate(id, dictionary! {
            "Type" => "Annot", "Subtype" => "Widget", "FT" => "Btn", "T" => Object::string_literal(format!("box-{state}")), "F" => 4,
            "Rect" => rect([x, 470.0, x + 16.0, 486.0]), "AS" => state, "V" => state,
            "AP" => dictionary! { "N" => dictionary! { "Yes" => ticked, "Off" => empty } },
        });
    }
    builder.catalog.set("AcroForm", dictionary! { "Fields" => Vec::<Object>::new(), "DR" => dictionary! { "Font" => dictionary! { "Helv" => font } } });
    builder.save()
}

/// A page with a red square in a group that prints and not on screen, a blue one in a group that shows on screen and
/// doesn't print, a green one in neither group, and the two groups' usage and the print event set as Acrobat writes them.
fn layers_sample() -> Vec<u8> {
    let mut builder = Builder::new("1.7");
    let group = |builder: &mut Builder, name: &str, print: &str, view: &str| builder.doc.add_object(dictionary! {
        "Type" => "OCG", "Name" => Object::string_literal(name),
        "Usage" => dictionary! { "Print" => dictionary! { "PrintState" => print }, "View" => dictionary! { "ViewState" => view } },
    });
    let (red, blue) = (group(&mut builder, "print only", "ON", "OFF"), group(&mut builder, "screen only", "OFF", "ON"));
    let id = builder.page(LETTER, "layers: the red square prints, the blue one is on screen only");
    let font = builder.font;
    builder.page_dict(id).set("Resources", dictionary! {
        "Font" => dictionary! { "F1" => font }, "Properties" => dictionary! { "MC0" => red, "MC1" => blue },
    });
    draw(&mut builder, id, "BT /F1 14 Tf 72 700 Td (layers: red prints, blue is on screen only, green is in neither layer) Tj ET \
        /OC /MC0 BDC 1 0 0 rg 72 500 120 120 re f EMC /OC /MC1 BDC 0 0 1 rg 232 500 120 120 re f EMC 0 0.6 0 rg 392 500 120 120 re f");
    let both = || -> Vec<Object> { vec![red.into(), blue.into()] };
    builder.catalog.set("OCProperties", dictionary! {
        "OCGs" => both(),
        "D" => dictionary! {
            "BaseState" => "ON", "ON" => both(), "OFF" => Vec::<Object>::new(),
            "AS" => vec![
                dictionary! { "Event" => "View", "Category" => vec![Object::from("View")], "OCGs" => both() }.into(),
                dictionary! { "Event" => "Print", "Category" => vec![Object::from("Print")], "OCGs" => both() }.into(),
            ],
        },
    });
    builder.save()
}

/// A file with its `startxref` pointing nowhere, which a reader has to rebuild its table for.
fn bad_startxref() -> Vec<u8> {
    let mut pdf = file("defect", &[LETTER, A4]);
    let at = pdf.windows(9).rposition(|w| w == b"startxref").expect("startxref") + 10;
    let end = pdf[at..].iter().position(|b| *b == b'\n').expect("a line") + at;
    pdf.splice(at..end, b"999999".iter().copied());
    pdf
}

/// The files `merge` is checked on besides the corpus, each with a structure the corpus lacks: pages turned and cut by
/// a crop box and of mixed sizes, resources a page inherits or shares, content in several streams, annotations of each
/// flag combination, filled and empty form fields, layers, an owner password and a damaged table.
fn synthetic() -> Vec<(String, Vec<u8>)> {
    use crate::stamp_tests::{build, Contents, Resources, LETTER as PAGE};
    let a4 = crate::stamp_tests::Page { media: A4, ..PAGE };
    let offset = crate::stamp_tests::Page { media: [100.0, 50.0, 712.0, 842.0], ..PAGE };
    let turned = [
        PAGE, PAGE.rotate(90), PAGE.rotate(180), PAGE.rotate(270), PAGE.crop([36.0, 72.0, 400.0, 500.0]), PAGE.crop([36.0, 72.0, 400.0, 500.0]).rotate(90),
        PAGE.crop([36.0, 72.0, 400.0, 500.0]).rotate(270), a4, a4.rotate(90), offset, offset.rotate(180), offset.crop([150.0, 100.0, 500.0, 600.0]).rotate(270),
    ];
    let contents = [PAGE.contents(Contents::Array), PAGE.contents(Contents::ArrayRef), PAGE.contents(Contents::Blank), PAGE];
    let rc4 = {
        let xml = r#"<lpdf><document><output encrypt="rc4-128" allow="print"/><section><layout><column><text>Printable, not copyable.</text></column></layout></section></document></lpdf>"#;
        let encryption = RenderEncryption { user_password: None, owner_password: Some("owner".to_string()), skip_encryption: false, entropy: vec![7; 64] };
        LpdfEngine::render_xml_with_encryption(xml, &encryption).expect("renders")
    };
    vec![
        ("synthetic-turned-cropped-and-mixed.pdf".to_string(), build(&turned, Resources::OnPage, false, None)),
        ("synthetic-inherited-resources-in-object-streams.pdf".to_string(), build(&turned, Resources::Inherited, true, None)),
        ("synthetic-shared-resources-and-content-streams.pdf".to_string(), build(&contents, Resources::Shared, false, None)),
        ("synthetic-annotations.pdf".to_string(), annotations_sample()),
        ("synthetic-form-states.pdf".to_string(), form_sample()),
        ("synthetic-layers.pdf".to_string(), layers_sample()),
        ("synthetic-bad-startxref.pdf".to_string(), bad_startxref()),
        ("lpdf-owner-restricted-aes-256.pdf".to_string(), locked("print", None)),
        ("lpdf-owner-restricted-rc4-128.pdf".to_string(), rc4),
    ]
}

#[test]
fn the_synthetic_files_merge_and_each_keeps_its_page_count() {
    for (name, bytes) in synthetic() {
        let options = Options { allow_restricted: true, ..Options::default() };
        let doc = Document::load_mem(&bytes).unwrap_or_else(|e| panic!("{name}: lopdf can't read it: {e}"));
        let out = merge(&sources(&[(name.as_str(), bytes.as_slice())]), &options, true).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(reopen(&out).get_pages().len(), doc.get_pages().len(), "{name}");
    }
}

/// Writes what `scripts/check-merge.mjs` reads: each file of the corpus and each synthetic one, `sources/<name>`, and
/// the merge of that file alone, `merged/<name>`; then every file merged in one PDF with a blank side after each
/// document of odd length, `all.pdf`, with `all.json` naming the source page each page of it is, and the same without a
/// key, `all-unlicensed.pdf`. Without `LPDF_MERGE_OUT` it writes nothing.
#[test]
fn writes_merge_samples_for_the_external_check() {
    let Some(dir) = std::env::var_os("LPDF_MERGE_OUT") else { return };
    let dir = std::path::Path::new(&dir);
    for sub in ["sources", "merged"] {
        std::fs::create_dir_all(dir.join(sub)).expect("the samples' folder");
    }
    let mut files = corpus();
    files.extend(synthetic());
    let options = Options { allow_signed: true, allow_restricted: true, double_sided: false };
    let mut pages = Vec::new();
    for (i, (name, bytes)) in files.iter().enumerate() {
        std::fs::write(dir.join("sources").join(name), bytes).expect("a source");
        let alone = merge(&sources(&[(name.as_str(), bytes.as_slice())]), &options, true).unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(dir.join("merged").join(name), alone).expect("a merged file");
        let count = Document::load_mem(bytes).expect("a PDF").get_pages().len();
        pages.extend((1..=count).map(|page| json!({ "source": name, "page": page })));
        if count % 2 == 1 && i + 1 < files.len() {
            pages.push(json!({ "blank": true }));
        }
    }
    let all: Vec<(&str, &[u8])> = files.iter().map(|(name, bytes)| (name.as_str(), bytes.as_slice())).collect();
    let together = Options { double_sided: true, ..options };
    std::fs::write(dir.join("all.pdf"), merged(&all, together)).expect("the whole");
    std::fs::write(dir.join("all.json"), json!({ "pages": pages }).to_string()).expect("its index");
    std::fs::write(dir.join("all-unlicensed.pdf"), merged_unlicensed(&all, together)).expect("the whole, unlicensed");
}
