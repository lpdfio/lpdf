//! Stamps on whole PDFs: the update after the input's bytes, pages in each form `lopdf` reads, the encryption an
//! owner password keeps, the errors, the snapshot fixtures, and the stamped PDF/A files `make check-pdfa` hands to
//! veraPDF. The PDFs stamped are built here with `lopdf`, a ruled page with its corners named upright, so that a
//! review of the fixtures sees where each stamp landed and which way up; or rendered by Lpdf, or taken from veraPDF's
//! corpus (`test/stamp/verapdf-corpus`).

use std::path::{Path, PathBuf};

use lopdf::{dictionary, Document, Object, SaveOptions, Stream};
use serde_json::json;

use crate::pdf::ImageRegistry;
use crate::png::tests::{phys, png};
use crate::snapshot_tests::check_snapshot;
use crate::{LpdfEngine, RenderEncryption};

// ── Inputs ────────────────────────────────────────────────────────────────────

/// How a page holds its content: one stream, an array of two, a reference to such an array, or none.
#[derive(Clone, Copy, PartialEq)]
enum Contents { Stream, Array, ArrayRef, Blank }

/// Where a page's resources are: its own, one dictionary every page refers to, or its parent's.
#[derive(Clone, Copy, PartialEq)]
enum Resources { OnPage, Shared, Inherited }

#[derive(Clone, Copy)]
struct Page {
    media:    [f32; 4],
    crop:     Option<[f32; 4]>,
    rotate:   i64,
    contents: Contents,
}

const LETTER: Page = Page { media: [0.0, 0.0, 612.0, 792.0], crop: None, rotate: 0, contents: Contents::Stream };

impl Page {
    fn crop(self, crop: [f32; 4]) -> Self { Page { crop: Some(crop), ..self } }
    fn rotate(self, rotate: i64) -> Self { Page { rotate, ..self } }
    fn contents(self, contents: Contents) -> Self { Page { contents, ..self } }
}

/// A PDF of `pages`, each ruled every inch from its top-left corner as a viewer shows it, framed 10 pt in, and with
/// its number and corners named, upright to the reader whatever its `/Rotate`.
fn build(pages: &[Page], resources: Resources, object_streams: bool, metadata: Option<&str>) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" });
    let own = dictionary! { "Font" => dictionary! { "F1" => font } };
    let shared = doc.add_object(own.clone());
    let mut kids = Vec::new();
    for (i, page) in pages.iter().enumerate() {
        let (first, second) = drawing(page, i + 1);
        let mut dict = dictionary! { "Type" => "Page", "Parent" => pages_id, "MediaBox" => rect(page.media) };
        if let Some(crop) = page.crop {
            dict.set("CropBox", rect(crop));
        }
        if page.rotate != 0 {
            dict.set("Rotate", page.rotate);
        }
        match resources {
            Resources::OnPage    => dict.set("Resources", own.clone()),
            Resources::Shared    => dict.set("Resources", shared),
            Resources::Inherited => {}
        }
        let mut stream = |text: String| doc.add_object(Stream::new(dictionary! {}, text.into_bytes()));
        match page.contents {
            Contents::Stream => dict.set("Contents", stream(format!("{first}{second}"))),
            Contents::Array => {
                let (a, b) = (stream(first), stream(second));
                dict.set("Contents", vec![a.into(), b.into()]);
            }
            Contents::ArrayRef => {
                let (a, b) = (stream(first), stream(second));
                let array = doc.add_object(vec![Object::from(a), Object::from(b)]);
                dict.set("Contents", array);
            }
            Contents::Blank => {}
        }
        kids.push(Object::from(doc.add_object(dict)));
    }
    let mut tree = dictionary! { "Type" => "Pages", "Count" => kids.len() as i64, "Kids" => kids };
    if resources == Resources::Inherited {
        tree.set("Resources", own);
    }
    doc.objects.insert(pages_id, Object::Dictionary(tree));
    let mut catalog = dictionary! { "Type" => "Catalog", "Pages" => pages_id };
    if let Some(xmp) = metadata {
        let packet = Stream::new(dictionary! { "Type" => "Metadata", "Subtype" => "XML" }, xmp.as_bytes().to_vec());
        catalog.set("Metadata", doc.add_object(packet));
    }
    let catalog = doc.add_object(catalog);
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    if object_streams {
        let options = SaveOptions::builder().use_object_streams(true).use_xref_streams(true).build();
        doc.save_with_options(&mut out, options).expect("saved");
    } else {
        doc.save_to(&mut out).expect("saved");
    }
    out
}

fn rect([a, b, c, d]: [f32; 4]) -> Object {
    vec![a.into(), b.into(), c.into(), d.into()].into()
}

/// The content of a page, in two parts, as `Contents::Array` splits it.
fn drawing(page: &Page, number: usize) -> (String, String) {
    let [l, b, r, t] = page.crop.unwrap_or(page.media);
    let rotate = page.rotate.rem_euclid(360);
    let (w, h) = if rotate % 180 == 90 { (t - b, r - l) } else { (r - l, t - b) };
    // From the page as shown, y up from its bottom-left corner, to the PDF's space.
    let m = match rotate {
        90  => [0.0, 1.0, -1.0, 0.0, r, b],
        180 => [-1.0, 0.0, 0.0, -1.0, r, t],
        270 => [0.0, -1.0, 1.0, 0.0, l, t],
        _   => [1.0, 0.0, 0.0, 1.0, l, b],
    };
    let mut rules = String::new();
    for x in (1..).map(|k| k as f32 * 72.0).take_while(|x| *x < w) {
        rules.push_str(&format!("{x} 0 m {x} {h} l "));
    }
    for y in (1..).map(|k| h - k as f32 * 72.0).take_while(|y| *y > 0.0) {
        rules.push_str(&format!("0 {y} m {w} {y} l "));
    }
    let [a, b2, c, d, e, f] = m;
    let first = format!(
        "q {a} {b2} {c} {d} {e} {f} cm 0.85 G 0.5 w {rules}S 0.4 G 1 w 10 10 {} {} re S\n", w - 20.0, h - 20.0,
    );
    let second = format!(
        "BT /F1 12 Tf 0.3 g 16 {} Td (page {number}: top left) Tj ET BT /F1 12 Tf {} 18 Td (bottom right) Tj ET Q\n",
        h - 28.0, w - 92.0,
    );
    (first, second)
}

// ── Images ────────────────────────────────────────────────────────────────────

/// The color of each quarter of a test image, so a review sees which way up it is: red top left, green top right,
/// blue bottom left, black bottom right.
fn quarter(x: u32, y: u32, w: u32, h: u32) -> [u8; 3] {
    match (x < w / 2, y < h / 2) {
        (true, true)   => [220, 30, 30],
        (false, true)  => [30, 160, 60],
        (true, false)  => [40, 70, 220],
        (false, false) => [20, 20, 20],
    }
}

/// An RGB PNG of `w` by `h` pixels in quarters, with the chunks of `extra` (such as its resolution).
fn quarters(w: u32, h: u32, extra: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let rows: Vec<Vec<u8>> = (0..h).map(|y| (0..w).flat_map(|x| quarter(x, y, w, h)).collect()).collect();
    png(w, h, 8, 2, &rows, extra)
}

/// A round seal: an RGBA PNG in quarters, opaque inside the circle it fits, half clear in a ring at its edge, and
/// clear outside.
fn seal(size: u32) -> Vec<u8> {
    let half = size as f32 / 2.0;
    let rows: Vec<Vec<u8>> = (0..size).map(|y| (0..size).flat_map(|x| {
        let d = ((x as f32 + 0.5 - half).powi(2) + (y as f32 + 0.5 - half).powi(2)).sqrt() / half;
        let alpha = if d < 0.8 { 255 } else if d < 1.0 { 128 } else { 0 };
        let [r, g, b] = quarter(x, y, size, size);
        [r, g, b, alpha]
    }).collect()).collect();
    png(size, size, 8, 6, &rows, &[])
}

/// An RGB PNG in quarters at 16 bits a channel.
fn deep(w: u32, h: u32) -> Vec<u8> {
    let rows: Vec<Vec<u8>> = (0..h).map(|y| (0..w).flat_map(|x| quarter(x, y, w, h).map(|c| [c, 0x80]).concat()).collect()).collect();
    png(w, h, 16, 2, &rows, &[])
}

/// A 1-bit gray scan: a checkerboard of 4-pixel squares, at `dpi`.
fn scan(w: u32, h: u32, dpi: f32) -> Vec<u8> {
    let rows: Vec<Vec<u8>> = (0..h).map(|y| {
        let mut row = vec![0u8; (w as usize).div_ceil(8)];
        for x in 0..w {
            if (x / 4 + y / 4) % 2 == 0 {
                row[x as usize / 8] |= 0x80 >> (x % 8);
            }
        }
        row
    }).collect();
    png(w, h, 1, 0, &rows, &[phys(dpi)])
}

/// A frame of `w` by `h` pixels, a gray line round a clear inside: drawn in a box of the same size, it shows the box.
fn frame(w: u32, h: u32) -> Vec<u8> {
    let rows: Vec<Vec<u8>> = (0..h).map(|y| (0..w).flat_map(|x| {
        let edge = x == 0 || y == 0 || x == w - 1 || y == h - 1;
        [150, 150, 150, if edge { 255 } else { 0 }]
    }).collect()).collect();
    png(w, h, 8, 6, &rows, &[])
}

// ── Stamping and reading back ─────────────────────────────────────────────────

/// Stamp `pdf` as a licensed engine does, with no attribution line.
fn stamp(pdf: &[u8], images: &[(&str, Vec<u8>)], stamps: serde_json::Value) -> Result<Vec<u8>, String> {
    let mut registry = ImageRegistry::new();
    for (name, bytes) in images {
        registry.load(name, bytes.clone());
    }
    crate::stamp::stamp(pdf, &crate::stamp::parse_stamps(&stamps)?, &registry, true)
}

/// Stamp `pdf` through an engine with no license key, which gives each stamp the attribution line.
fn stamp_unlicensed(pdf: &[u8], images: &[(&str, Vec<u8>)], stamps: serde_json::Value) -> Result<Vec<u8>, String> {
    let mut engine = LpdfEngine::new("");
    for (name, bytes) in images {
        engine.load_image(name, bytes);
    }
    engine.stamp_pdf(pdf, &stamps.to_string())
}

/// The output of a stamp, which must have succeeded and left the input's bytes as they were, at its start.
fn kept(pdf: &[u8], out: Result<Vec<u8>, String>) -> Vec<u8> {
    let out = out.unwrap_or_else(|e| panic!("stamp failed: {e}"));
    assert!(out.len() > pdf.len() && out.starts_with(pdf), "the output is not the input with an update after it");
    out
}

/// `stamp`, licensed, which must succeed.
fn stamped(pdf: &[u8], images: &[(&str, Vec<u8>)], stamps: serde_json::Value) -> Vec<u8> {
    kept(pdf, stamp(pdf, images, stamps))
}

/// `stamp_unlicensed`, which must succeed.
fn stamped_unlicensed(pdf: &[u8], images: &[(&str, Vec<u8>)], stamps: serde_json::Value) -> Vec<u8> {
    kept(pdf, stamp_unlicensed(pdf, images, stamps))
}

fn reopen(pdf: &[u8]) -> Document {
    Document::load_mem(pdf).unwrap_or_else(|e| panic!("the stamped PDF doesn't load: {e}"))
}

fn page(doc: &Document, number: u32) -> &lopdf::Dictionary {
    doc.get_dictionary(doc.get_pages()[&number]).expect("a page")
}

/// The references of a page's `/Contents`, which a stamped page has as an array.
fn contents(doc: &Document, number: u32) -> Vec<Object> {
    page(doc, number).get(b"Contents").and_then(Object::as_array).expect("an array of contents").clone()
}

/// The text of a page's last content stream, the one the stamp draws.
fn drawn(doc: &Document, number: u32) -> String {
    let last = contents(doc, number).last().and_then(|o| o.as_reference().ok()).expect("a reference");
    let stream = doc.get_object(last).and_then(Object::as_stream).expect("a stream");
    String::from_utf8_lossy(&stream.decompressed_content().unwrap_or_else(|_| stream.content.clone())).into_owned()
}

/// The image XObjects of a file: the objects a stamp adds, with their soft masks.
fn images_in(doc: &Document) -> usize {
    doc.objects.values().filter(|o| {
        o.as_stream().is_ok_and(|s| s.dict.get(b"Subtype").and_then(Object::as_name).is_ok_and(|n| n == b"Image"))
    }).count()
}

fn test_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test")
}

// ── The update ────────────────────────────────────────────────────────────────

#[test]
fn a_stamped_page_draws_its_own_content_in_q_and_q_then_the_image_as_an_artifact() {
    let pdf = build(&[LETTER], Resources::OnPage, false, None);
    let out = stamped(&pdf, &[("i", quarters(60, 30, &[]))], json!([{ "image": "i", "page": "1", "x": "72pt", "y": "72pt", "width": "60pt", "height": "30pt" }]));
    let doc = reopen(&out);
    let parts = contents(&doc, 1);
    assert_eq!(parts.len(), 3, "the q, the page's own stream and the stamp's");
    let first = doc.get_object(parts[0].as_reference().unwrap()).and_then(Object::as_stream).unwrap();
    assert_eq!(first.content, b"q\n");
    // 72 pt from the left, and 72 + 30 pt from the top of a page 792 pt high.
    assert_eq!(drawn(&doc, 1), "Q\n/Artifact BMC\nq 60 0 0 30 72 690 cm /LpdfStamp0 Do Q\nEMC\n");
    let xobjects = page(&doc, 1).get(b"Resources").and_then(Object::as_dict).and_then(|r| r.get(b"XObject")).and_then(Object::as_dict).unwrap();
    assert!(xobjects.has(b"LpdfStamp0"));
}

#[test]
fn a_contents_array_and_a_reference_to_one_become_one_flat_array() {
    let pages = [LETTER.contents(Contents::Array), LETTER.contents(Contents::ArrayRef), LETTER.contents(Contents::Blank)];
    let pdf = build(&pages, Resources::OnPage, false, None);
    let out = stamped(&pdf, &[("i", quarters(60, 30, &[]))], json!([{ "image": "i", "page": "each", "x": 72, "y": 72, "width": 60, "height": 30 }]));
    let doc = reopen(&out);
    for number in [1, 2] {
        let parts = contents(&doc, number);
        assert_eq!(parts.len(), 4, "page {number}: the q, the page's two streams and the stamp's");
        assert!(parts.iter().all(|p| p.as_reference().is_ok_and(|id| doc.get_object(id).is_ok_and(|o| o.as_stream().is_ok()))));
    }
    // A page with no content of its own gets the stamp alone, with no Q to close a q it doesn't have.
    assert_eq!(contents(&doc, 3).len(), 1);
    assert!(drawn(&doc, 3).starts_with("/Artifact BMC\n"));
}

#[test]
fn inherited_resources_are_copied_onto_the_stamped_page_and_the_parent_keeps_its_own() {
    let pdf = build(&[LETTER, LETTER], Resources::Inherited, false, None);
    let out = stamped(&pdf, &[("i", quarters(60, 30, &[]))], json!([{ "image": "i", "page": "1", "x": 72, "y": 72, "width": 60, "height": 30 }]));
    let doc = reopen(&out);
    let resources = page(&doc, 1).get(b"Resources").and_then(Object::as_dict).expect("the page's own resources");
    assert!(resources.get(b"Font").and_then(Object::as_dict).is_ok_and(|f| f.has(b"F1")), "the inherited font is kept");
    assert!(resources.get(b"XObject").and_then(Object::as_dict).is_ok_and(|x| x.has(b"LpdfStamp0")));
    assert!(!page(&doc, 2).has(b"Resources"), "the page not stamped still inherits");
    let parent = page(&doc, 2).get(b"Parent").and_then(Object::as_reference).unwrap();
    let inherited = doc.get_dictionary(parent).unwrap().get(b"Resources").and_then(Object::as_dict).unwrap();
    assert!(!inherited.has(b"XObject"), "the parent's resources are as they were");
}

#[test]
fn resources_shared_by_the_pages_stay_as_they_were_for_the_others() {
    let pdf = build(&[LETTER, LETTER], Resources::Shared, false, None);
    let out = stamped(&pdf, &[("i", quarters(60, 30, &[]))], json!([{ "image": "i", "page": "2", "x": 72, "y": 72, "width": 60, "height": 30 }]));
    let doc = reopen(&out);
    let shared = page(&doc, 1).get(b"Resources").and_then(Object::as_reference).expect("page 1 still refers to the shared ones");
    assert!(!doc.get_dictionary(shared).unwrap().has(b"XObject"));
    assert!(page(&doc, 2).get(b"Resources").and_then(Object::as_dict).is_ok_and(|r| r.has(b"Font") && r.has(b"XObject")));
}

#[test]
fn an_image_on_several_pages_or_in_several_stamps_is_one_object() {
    let pdf = build(&[LETTER; 4], Resources::OnPage, false, None);
    let before = images_in(&reopen(&pdf));
    let out = stamped(&pdf, &[("a", quarters(60, 30, &[])), ("b", seal(40))], json!([
        { "image": "a", "page": "each", "x": 72, "y": 72, "width": 60, "height": 30 },
        { "image": "a", "page": "1-2", "anchor": "center", "width": 60, "height": 30 },
        { "image": "b", "page": "odd", "anchor": "bottom-right", "x": -36, "y": -36, "width": 40, "height": 40 },
    ]));
    // `a`, and `b` with its soft mask.
    assert_eq!(images_in(&reopen(&out)) - before, 3);
    let doc = reopen(&out);
    assert_eq!(drawn(&doc, 1).matches("/LpdfStamp0 Do").count(), 2);
    assert!(drawn(&doc, 1).contains("/LpdfStamp1 Do"));
    assert!(!drawn(&doc, 2).contains("/LpdfStamp1"));
}

#[test]
fn a_file_stamped_again_gets_names_its_pages_do_not_have() {
    let pdf = build(&[LETTER], Resources::OnPage, false, None);
    let one = json!([{ "image": "i", "page": "1", "x": 72, "y": 72, "width": 60, "height": 30 }]);
    let once = stamped(&pdf, &[("i", quarters(60, 30, &[]))], one.clone());
    let twice = stamped(&once, &[("i", quarters(60, 30, &[]))], one);
    let doc = reopen(&twice);
    assert!(drawn(&doc, 1).contains("/LpdfStamp1 Do"));
    let xobjects = page(&doc, 1).get(b"Resources").and_then(Object::as_dict).and_then(|r| r.get(b"XObject")).and_then(Object::as_dict).unwrap();
    assert!(xobjects.has(b"LpdfStamp0") && xobjects.has(b"LpdfStamp1"));
    // The first stamp is still drawn, inside the second's q and Q.
    assert_eq!(contents(&doc, 1).len(), 5);
}

#[test]
fn a_file_with_object_streams_is_stamped_with_a_cross_reference_stream() {
    let pdf = build(&[LETTER, LETTER], Resources::OnPage, true, None);
    assert!(String::from_utf8_lossy(&pdf).contains("/ObjStm"));
    let out = stamped(&pdf, &[("i", quarters(60, 30, &[]))], json!([{ "image": "i", "page": "last", "x": 72, "y": 72, "width": 60, "height": 30 }]));
    let update = String::from_utf8_lossy(&out[pdf.len()..]).into_owned();
    assert!(update.contains("/XRef") && !update.contains("\nxref"), "the update keeps the file's kind of cross-reference");
    assert!(drawn(&reopen(&out), 2).contains("/LpdfStamp0 Do"));
}

// ── Encryption ────────────────────────────────────────────────────────────────

/// A document of Lpdf's, encrypted with `cipher` and an owner password only: it opens without one, and allows
/// printing and nothing else.
fn owner_only(cipher: &str) -> Vec<u8> {
    let xml = format!(r#"<lpdf><document><output encrypt="{cipher}" allow="print"/><section><layout><column>
        <text>Encrypted with an owner password only.</text></column></layout></section></document></lpdf>"#);
    let encryption = RenderEncryption {
        user_password: None,
        owner_password: Some("owner".to_string()),
        skip_encryption: false,
        entropy: vec![7; 64],
    };
    LpdfEngine::render_xml_with_encryption(&xml, &encryption).expect("renders")
}

#[test]
fn a_file_with_an_owner_password_keeps_its_encryption_and_its_permissions() {
    for cipher in ["rc4-128", "aes-256"] {
        let pdf = owner_only(cipher);
        let out = stamped(&pdf, &[("i", seal(40))], json!([{ "image": "i", "page": "1", "anchor": "center", "width": 40, "height": 40 }]));
        let update = String::from_utf8_lossy(&out[pdf.len()..]).into_owned();
        assert!(update.contains("/Encrypt"), "{cipher}: the update's trailer names the encryption");
        // The stamp's own stream is encrypted: its text is not in the file.
        assert!(!update.contains("/Artifact BMC"), "{cipher}: the stamp's content is written in the clear");
        let before = reopen(&pdf);
        let after = reopen(&out);
        assert!(after.was_encrypted(), "{cipher}");
        let (was, is) = (before.encryption_state.as_ref().unwrap(), after.encryption_state.as_ref().unwrap());
        assert_eq!(is.permissions(), was.permissions(), "{cipher}");
        assert_eq!(is.encrypt_object_id(), was.encrypt_object_id(), "{cipher}: the update refers to the file's own /Encrypt");
        assert!(drawn(&after, 1).contains("/LpdfStamp0 Do"), "{cipher}: the stamp is drawn once decrypted");
    }
}

// ── Without a license key ─────────────────────────────────────────────────────

/// The text of what a stamp appended to `pdf`.
fn update(pdf: &[u8], out: &[u8]) -> String {
    String::from_utf8_lossy(&out[pdf.len()..]).into_owned()
}

#[test]
fn an_unlicensed_stamp_draws_the_line_over_the_images_and_links_it_to_lpdf_io() {
    let pdf = build(&[LETTER], Resources::OnPage, false, None);
    let out = stamped_unlicensed(&pdf, &[("i", quarters(60, 30, &[]))], json!([{ "image": "i", "page": "1", "x": 72, "y": 72, "width": 60, "height": 30 }]));
    let doc = reopen(&out);
    let drawn = drawn(&doc, 1);
    let image = drawn.find("/LpdfStamp0 Do").expect("the image");
    // Over the box: its bottom 2 pt above the box's top, 72 pt down, which is 792 - 70 in the PDF's space.
    let line = drawn.find(" 722 cm ").expect(&drawn);
    assert!(image < line, "the line is drawn after the image");
    assert!(drawn.contains("0.8431 0.4353 0.0157 rg") && drawn.contains(" B Q\n"), "{drawn}");
    let annots = page(&doc, 1).get(b"Annots").and_then(Object::as_array).expect("a link");
    let link = doc.get_dictionary(annots[0].as_reference().unwrap()).unwrap();
    assert_eq!(link.get(b"Subtype").and_then(Object::as_name).unwrap(), b"Link");
    let uri = link.get(b"A").and_then(Object::as_dict).and_then(|a| a.get(b"URI")).and_then(Object::as_str).unwrap();
    assert_eq!(uri, b"https://lpdf.io");
    assert!(!link.has(b"F"), "only PDF/A asks for the print flag");
}

#[test]
fn a_licensed_stamp_draws_no_line() {
    let pdf = build(&[LETTER], Resources::OnPage, false, None);
    let out = stamped(&pdf, &[("i", quarters(60, 30, &[]))], json!([{ "image": "i", "page": "1", "x": 72, "y": 72, "width": 60, "height": 30 }]));
    assert!(!update(&pdf, &out).contains("lpdf.io") && !drawn(&reopen(&out), 1).contains(" B Q"));
}

#[test]
fn a_line_in_a_tagged_file_has_no_link() {
    let pdf = crate::pdfua_render_tests::render_fixture();
    let out = stamped_unlicensed(&pdf, &[("seal", seal(64))], json!([{ "image": "seal", "page": "1", "anchor": "bottom-right", "x": -36, "y": -36, "width": 64, "height": 64 }]));
    let doc = reopen(&out);
    assert!(drawn(&doc, 1).contains(" B Q\n"), "the line is drawn");
    assert!(!update(&pdf, &out).contains("(https://lpdf.io)"), "no link was added");
}

#[test]
fn a_line_on_a_pdfa_file_is_in_srgb_and_its_link_prints() {
    let pdf = build(&[LETTER], Resources::OnPage, false, Some(PDFA_2B));
    let out = stamped_unlicensed(&pdf, &[("i", quarters(60, 30, &[]))], json!([{ "image": "i", "page": "1", "x": 72, "y": 72, "width": 60, "height": 30 }]));
    let doc = reopen(&out);
    assert!(drawn(&doc, 1).contains("/LpdfColor0 cs 0.8431 0.4353 0.0157 sc"));
    let spaces = page(&doc, 1).get(b"Resources").and_then(Object::as_dict).and_then(|r| r.get(b"ColorSpace")).and_then(Object::as_dict).unwrap();
    assert!(spaces.has(b"LpdfColor0"));
    let annots = page(&doc, 1).get(b"Annots").and_then(Object::as_array).unwrap();
    assert_eq!(doc.get_dictionary(annots[0].as_reference().unwrap()).unwrap().get(b"F").and_then(Object::as_i64).unwrap(), 4);
}

// ── Errors ────────────────────────────────────────────────────────────────────

fn error(pdf: &[u8], images: &[(&str, Vec<u8>)], stamps: serde_json::Value) -> String {
    stamp(pdf, images, stamps).err().expect("an error")
}

#[test]
fn a_file_that_needs_a_password_to_open_is_an_error() {
    let xml = r#"<lpdf><document><output encrypt="aes-256"/><section><layout><text>Secret</text></layout></section></document></lpdf>"#;
    let encryption = RenderEncryption {
        user_password: Some("open".to_string()),
        owner_password: Some("owner".to_string()),
        skip_encryption: false,
        entropy: vec![7; 64],
    };
    let pdf = LpdfEngine::render_xml_with_encryption(xml, &encryption).expect("renders");
    let e = error(&pdf, &[("i", seal(40))], json!([{ "image": "i", "page": "1", "x": 0, "y": 0, "width": 40, "height": 40 }]));
    assert!(e.starts_with("the PDF needs a password to open"), "{e}");
}

#[test]
fn a_page_the_file_does_not_have_and_an_image_that_is_not_a_png_are_errors_that_name_the_stamp() {
    let pdf = build(&[LETTER, LETTER], Resources::OnPage, false, None);
    let at = |page: &str, image: &str| json!([
        { "image": "png", "page": "1", "x": 0, "y": 0, "width": 40, "height": 40 },
        { "image": image, "page": page, "x": 0, "y": 0, "width": 40, "height": 40 },
    ]);
    let images = [("png", seal(40)), ("jpeg", b"\xFF\xD8\xFF\xE0 a JPEG".to_vec())];
    assert_eq!(error(&pdf, &images, at("3", "png")), "stamp 2: the PDF has no page 3: it has 2 pages");
    assert_eq!(error(&pdf, &images, at("1", "jpeg")), "stamp 2: the image is a JPEG, and a stamp takes a PNG: convert it to PNG");
    assert_eq!(error(&pdf, &images, at("1", "none")), "stamp 2: the image 'none' was not loaded");
    assert_eq!(error(b"not a pdf at all", &images, at("1", "png")), "the input is not a PDF: it doesn't start with %PDF-");
}

const PDFA_1B: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:pdfaid="http://www.aiim.org/pdfa/ns/id/" rdf:about="" pdfaid:part="1" pdfaid:conformance="B"/></rdf:RDF></x:xmpmeta>"#;
const PDFA_2B: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:pdfaid="http://www.aiim.org/pdfa/ns/id/" rdf:about=""><pdfaid:part>2</pdfaid:part><pdfaid:conformance>B</pdfaid:conformance></rdf:Description></rdf:RDF></x:xmpmeta>"#;

#[test]
fn a_transparent_png_on_a_pdfa_1_file_is_an_error_and_an_opaque_one_is_drawn_in_srgb() {
    let pdf = build(&[LETTER], Resources::OnPage, false, Some(PDFA_1B));
    let at = json!([{ "image": "i", "page": "1", "x": 72, "y": 72, "width": 40, "height": 40 }]);
    let e = error(&pdf, &[("i", seal(40))], at.clone());
    assert_eq!(e, "stamp 1: the PNG has transparency, which a PDF/A-1 file can't hold: give it a PNG without an alpha \
                   channel, flattened onto white");
    let doc = reopen(&stamped(&pdf, &[("i", deep(40, 40))], at));
    let image = doc.objects.values().filter_map(|o| o.as_stream().ok())
        .find(|s| s.dict.get(b"Subtype").and_then(Object::as_name).is_ok_and(|n| n == b"Image")).expect("the image");
    let space = image.dict.get(b"ColorSpace").and_then(Object::as_array).expect("an ICCBased space");
    assert_eq!(space[0].as_name().unwrap(), b"ICCBased");
    assert_eq!(image.dict.get(b"BitsPerComponent").and_then(Object::as_i64).unwrap(), 8, "16 bits are written at 8");
}

#[test]
fn a_transparent_png_on_a_pdfa_2_file_with_no_output_intent_gives_its_page_a_group_in_srgb() {
    let pdf = build(&[LETTER, LETTER], Resources::OnPage, false, Some(PDFA_2B));
    let out = stamped(&pdf, &[("seal", seal(40)), ("opaque", quarters(40, 20, &[]))], json!([
        { "image": "seal", "page": "1", "x": 72, "y": 72, "width": 40, "height": 40 },
        { "image": "opaque", "page": "2", "x": 72, "y": 72, "width": 40, "height": 20 },
    ]));
    let doc = reopen(&out);
    let group = page(&doc, 1).get(b"Group").and_then(Object::as_dict).expect("a group");
    assert_eq!(group.get(b"S").and_then(Object::as_name).unwrap(), b"Transparency");
    assert_eq!(group.get(b"CS").and_then(Object::as_array).unwrap()[0].as_name().unwrap(), b"ICCBased");
    assert!(!page(&doc, 2).has(b"Group"), "a page with no transparency needs none");
}

// ── Fixtures ──────────────────────────────────────────────────────────────────

/// The image of each stamp in the fixtures: quarters of 60 by 30 px, a pixel to a point.
fn small() -> (&'static str, Vec<u8>) { ("small", quarters(60, 30, &[])) }

/// Placement: `x` and `y` alone, and each of the nine anchors with offsets, on a page of US Letter.
fn placement() -> Vec<u8> {
    let pdf = build(&[LETTER], Resources::OnPage, false, None);
    let mut stamps = vec![json!({ "image": "small", "page": "1", "x": "2in", "y": "2in", "width": "60pt", "height": "30pt" })];
    for (anchor, x, y) in [
        ("top-left", 36, 36), ("top-center", 0, 36), ("top-right", -36, 36),
        ("center-left", 36, 0), ("center", 0, 0), ("center-right", -36, 0),
        ("bottom-left", 36, -36), ("bottom-center", 0, -36), ("bottom-right", -36, -36),
    ] {
        stamps.push(json!({ "image": "small", "page": "1", "anchor": anchor, "x": format!("{x}pt"), "y": format!("{y}pt"), "width": "60pt", "height": "30pt" }));
    }
    stamped(&pdf, &[small()], json!(stamps))
}

/// `align` and `justify`, each start, center and end: an image of 60 by 30 pt in boxes of 150 by 90 pt, framed.
fn in_the_box() -> Vec<u8> {
    let pdf = build(&[LETTER], Resources::OnPage, false, None);
    let mut stamps = Vec::new();
    for (row, justify) in ["start", "center", "end"].iter().enumerate() {
        for (column, align) in ["start", "center", "end"].iter().enumerate() {
            let (x, y) = (72 + column * 162, 108 + row * 126);
            stamps.push(json!({ "image": "frame", "page": "1", "x": x, "y": y, "width": 150, "height": 90 }));
            stamps.push(json!({ "image": "small", "page": "1", "x": x, "y": y, "width": 150, "height": 90, "align": align, "justify": justify }));
        }
    }
    stamped(&pdf, &[small(), ("frame", frame(150, 90))], json!(stamps))
}

/// Sizes, each in a framed box of 150 by 100 pt: `fit` with an image smaller than its box and one larger, `fill`,
/// `maxGrow`, a PNG with a resolution and a 1-bit scan at 200 dpi, a transparent seal, and a 16-bit PNG.
fn sizes() -> Vec<u8> {
    let pdf = build(&[LETTER], Resources::OnPage, false, None);
    let cases = [
        json!({ "image": "small" }),
        json!({ "image": "large" }),
        json!({ "image": "small", "scale": "fill" }),
        json!({ "image": "small", "max-grow": 1.5 }),
        json!({ "image": "dpi" }),
        json!({ "image": "scan" }),
        json!({ "image": "seal", "scale": "fill" }),
        json!({ "image": "deep" }),
    ];
    let mut stamps = Vec::new();
    for (i, case) in cases.into_iter().enumerate() {
        let (x, y) = (72 + (i % 2) * 234, 72 + (i / 2) * 162);
        stamps.push(json!({ "image": "frame", "page": "1", "x": x, "y": y, "width": 150, "height": 100 }));
        let mut s = json!({ "page": "1", "x": x, "y": y, "width": 150, "height": 100 });
        s.as_object_mut().unwrap().extend(case.as_object().unwrap().clone());
        stamps.push(s);
    }
    let images = [
        small(),
        ("large", quarters(400, 200, &[])),
        // 600 by 300 px at 300 dpi: 2 by 1 in, 144 by 72 pt.
        ("dpi", quarters(600, 300, &[phys(300.0)])),
        // 200 by 100 px at 200 dpi: 72 by 36 pt.
        ("scan", scan(200, 100, 200.0)),
        ("seal", seal(64)),
        ("deep", deep(60, 30)),
        ("frame", frame(150, 100)),
    ];
    stamped(&pdf, &images, json!(stamps))
}

/// Pages: one page, a range, a list, `each`, `odd` and `last`, of six, each scope at its own place along the top.
fn pages() -> Vec<u8> {
    let pdf = build(&[LETTER; 6], Resources::OnPage, false, None);
    let stamps: Vec<_> = ["2", "2-3", "1,4,6", "each", "odd", "last"].iter().enumerate().map(|(i, page)| {
        json!({ "image": "small", "page": page, "x": 36 + i * 90, "y": 60, "width": 60, "height": 30 })
    }).collect();
    stamped(&pdf, &[small()], json!(stamps))
}

/// Pages turned by 0, 90, 180 and 270 degrees, each with a crop box an inch inside its media box, and a landscape
/// page turned by 90: a stamp at the top left, one in the middle and one at the bottom right of each, as shown.
fn rotated() -> Vec<u8> {
    let crop = [72.0, 72.0, 540.0, 720.0];
    let pages = [
        LETTER.crop(crop), LETTER.crop(crop).rotate(90), LETTER.crop(crop).rotate(180), LETTER.crop(crop).rotate(270),
        Page { media: [0.0, 0.0, 842.0, 595.0], ..LETTER }.rotate(-270),
    ];
    let pdf = build(&pages, Resources::OnPage, false, None);
    let stamps = json!([
        { "image": "small", "page": "each", "x": 36, "y": 36, "width": 60, "height": 30 },
        { "image": "seal", "page": "each", "anchor": "center", "width": 64, "height": 64 },
        { "image": "small", "page": "each", "anchor": "bottom-right", "x": -36, "y": -36, "width": 60, "height": 30 },
    ]);
    stamped(&pdf, &[small(), ("seal", seal(64))], stamps)
}

/// The forms a page holds its content in: a stream, an array, a reference to an array, and none, with resources
/// inherited from the page tree.
fn page_forms() -> Vec<u8> {
    let pages = [LETTER, LETTER.contents(Contents::Array), LETTER.contents(Contents::ArrayRef), LETTER.contents(Contents::Blank)];
    let pdf = build(&pages, Resources::Inherited, false, None);
    stamped(&pdf, &[small()], json!([{ "image": "small", "page": "each", "anchor": "top-right", "x": -36, "y": 36, "width": 60, "height": 30 }]))
}

/// A file with object streams and a cross-reference stream.
fn object_streams() -> Vec<u8> {
    let pdf = build(&[LETTER, LETTER], Resources::Shared, true, None);
    stamped(&pdf, &[small()], json!([{ "image": "small", "page": "each", "anchor": "center", "width": 60, "height": 30 }]))
}

/// A document of Lpdf's with an owner password, in RC4, which has no random part, so the file is the same each run.
fn rc4() -> Vec<u8> {
    stamped(&owner_only("rc4-128"), &[("seal", seal(64))], json!([{ "image": "seal", "page": "1", "anchor": "bottom-right", "x": -36, "y": -36, "width": 64, "height": 64 }]))
}

/// A PDF/A file stamped without a license key, with a transparent seal and a 16-bit PNG, or on PDF/A-1, where no soft
/// mask may go, with an opaque PNG and a 16-bit one.
fn pdfa(pdf: &[u8], part_1: bool) -> Vec<u8> {
    let first = if part_1 { ("first", quarters(60, 30, &[])) } else { ("first", seal(64)) };
    // Unlicensed, so that veraPDF checks the attribution line and its link too.
    stamped_unlicensed(pdf, &[first, ("deep", deep(60, 30))], json!([
        { "image": "first", "page": "each", "anchor": "top-right", "x": -36, "y": 36, "width": 64, "height": 64 },
        { "image": "deep", "page": "last", "anchor": "bottom-right", "x": -36, "y": -36, "width": 60, "height": 30 },
    ]))
}

/// Without a license key: a box in the middle of the page, with its line above it; one at the top of the page, with
/// its line below it; one the size of the page, with its line in the page's top-right corner, over the stamps; and a
/// page turned by 90 degrees, with its lines upright beside their boxes.
fn unlicensed() -> Vec<u8> {
    let pdf = build(&[LETTER, LETTER.rotate(90)], Resources::OnPage, false, None);
    stamped_unlicensed(&pdf, &[small(), ("frame", frame(150, 90)), ("page", frame(612, 792))], json!([
        { "image": "frame", "page": "1", "x": 72, "y": 300, "width": 150, "height": 90 },
        { "image": "small", "page": "1", "x": 72, "y": 300, "width": 150, "height": 90 },
        { "image": "frame", "page": "1", "anchor": "top-center", "y": 4, "width": 150, "height": 90 },
        { "image": "page", "page": "1", "x": 0, "y": 0, "width": 612, "height": 792 },
        { "image": "frame", "page": "2", "anchor": "center", "width": 150, "height": 90 },
        { "image": "small", "page": "2", "anchor": "bottom-left", "x": 36, "y": -36, "width": 60, "height": 30 },
    ]))
}

/// The PDF/A-1b files of veraPDF's corpus, by name.
fn corpus() -> Vec<(String, Vec<u8>)> {
    let dir = test_dir().join("stamp/verapdf-corpus");
    let mut files: Vec<_> = std::fs::read_dir(&dir).expect("the corpus").filter_map(|e| {
        let path = e.ok()?.path();
        if path.extension()? != "pdf" {
            return None;
        }
        Some((path.file_stem()?.to_string_lossy().into_owned(), std::fs::read(&path).ok()?))
    }).collect();
    files.sort();
    files
}

macro_rules! stamp_snapshot {
    ($test:ident, $name:literal, $make:expr) => {
        #[test]
        fn $test() { check_snapshot($name, &$make); }
    };
}

stamp_snapshot!(stamp_placement, "stamp-placement", placement());
stamp_snapshot!(stamp_in_the_box, "stamp-in-the-box", in_the_box());
stamp_snapshot!(stamp_sizes, "stamp-sizes", sizes());
stamp_snapshot!(stamp_pages, "stamp-pages", pages());
stamp_snapshot!(stamp_rotated, "stamp-rotated", rotated());
stamp_snapshot!(stamp_page_forms, "stamp-page-forms", page_forms());
stamp_snapshot!(stamp_object_streams, "stamp-object-streams", object_streams());
stamp_snapshot!(stamp_rc4, "stamp-rc4", rc4());
stamp_snapshot!(stamp_without_a_key, "stamp-unlicensed", unlicensed());
stamp_snapshot!(stamp_pdfa_2b, "stamp-pdfa-2b", pdfa(&crate::pdfa_render_tests::render_fixture("2b"), false));
stamp_snapshot!(stamp_pdfa_2u, "stamp-pdfa-2u", pdfa(&crate::pdfa_render_tests::render_fixture("2u"), false));

#[test]
fn stamp_pdfa_1b_corpus() {
    for (name, pdf) in corpus() {
        check_snapshot(&format!("stamp-pdfa-1b-{name}"), &pdfa(&pdf, true));
    }
}

/// The stamped PDF/A files `scripts/check-pdfa.mjs` hands to veraPDF and qpdf, written into `dir`: the 2b and 2u
/// fixtures of Lpdf's, and the PDF/A-1b files of veraPDF's corpus, each with the attribution line and its link.
/// Called from `writes_pdfa_samples_for_the_external_check`.
pub(crate) fn write_pdfa_samples(dir: &Path) {
    for level in ["2b", "2u"] {
        let pdf = pdfa(&crate::pdfa_render_tests::render_fixture(level), false);
        std::fs::write(dir.join(format!("stamp-pdfa-{level}.pdf")), pdf).expect("a sample");
    }
    for (name, pdf) in corpus() {
        std::fs::write(dir.join(format!("stamp-pdfa-1b-{name}.pdf")), pdfa(&pdf, true)).expect("a sample");
    }
}

/// The PDF/UA fixture, a tagged file, stamped without a license key: the line is an artifact and has no link, so the
/// file stays PDF/UA. Called from `writes_pdfua_samples_for_the_external_check`.
pub(crate) fn write_pdfua_sample(dir: &Path) {
    let pdf = crate::pdfua_render_tests::render_fixture();
    let out = stamped_unlicensed(&pdf, &[("seal", seal(64))], json!([
        { "image": "seal", "page": "each", "anchor": "bottom-right", "x": -36, "y": -36, "width": 64, "height": 64 },
    ]));
    std::fs::write(dir.join("stamp-pdfua-1.pdf"), out).expect("a sample");
}
