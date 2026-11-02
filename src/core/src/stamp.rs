//! Stamping: PNG images drawn over the pages of an existing PDF (`LpdfEngine::stamp`).
//!
//! The engine writes PDFs and reads none, but here: `lopdf` reads the file, and the stamp adds an incremental update
//! after its last byte, which leaves the original bytes as they were, keeps an owner password's encryption and the
//! revision a digital signature signed. The update holds the images, and a copy of each stamped page with its own
//! `/Contents` and `/Resources`, which the stamp edits itself: `lopdf`'s page helpers nest a referenced contents array
//! and hide inherited resources.
//!
//! A stamp's box is placed as a layer element is (`canvas.rs`), on the page as a viewer shows it: its crop box,
//! turned by its `/Rotate`. The image is fitted in the box without distortion and placed in it by `align` and
//! `justify`. A PDF/A file of parts 1 to 3 stays PDF/A: the image is drawn in the sRGB profile `pdfa.rs` bundles, and
//! what PDF/A-1 can't hold, a soft mask, is an error.

use std::collections::{BTreeMap, HashMap};

use lopdf::{Dictionary, Document, IncrementalDocument, Object, ObjectId, Stream};
use miniz_oxide::deflate::compress_to_vec_zlib;

use crate::canvas::{apply_anchor_offset, parse_anchor, resolve_anchor, Anchor};
use crate::page_scope::page_scope_indices;
use crate::parse::{parse_measurement, parse_page_scope, parse_signed_measurement, PageScope};
use crate::pdf::ImageRegistry;
use crate::pdfa;
use crate::png::{self, Png};

/// Where an image goes along one side of its box: `align` across it, `justify` down it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Place { Start, Center, End }

/// How an image is sized in its box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Scale {
    /// Shrunk to fit a box it is larger than, and kept at its natural size, or grown to `max_grow` times it, in one it
    /// is smaller than.
    Fit,
    /// As large as fits the box.
    Fill,
}

/// One stamp: an image, the pages it goes on, and its box on each.
#[derive(Debug, Clone)]
pub(crate) struct Stamp {
    /// The name the image was loaded under (`LpdfEngine::load_image`).
    pub image:    String,
    pub page:     PageScope,
    /// The box's top-left corner, in pt from the page's, y down; with an `anchor`, offsets from that point of the page.
    pub x:        f32,
    pub y:        f32,
    pub anchor:   Option<Anchor>,
    pub width:    f32,
    pub height:   f32,
    pub align:    Place,
    pub justify:  Place,
    pub scale:    Scale,
    /// How many times its natural size `fit` may grow an image, from 1.
    pub max_grow: f32,
}

/// The options a stamp takes, by the names the SDKs send.
const OPTIONS: &[&str] = &["image", "page", "x", "y", "anchor", "width", "height", "align", "justify", "scale", "max-grow"];

/// Read the stamps of a `stamp` call: a list of objects, each with the name of a loaded image.
pub(crate) fn parse_stamps(value: &serde_json::Value) -> Result<Vec<Stamp>, String> {
    let list = value.as_array().ok_or("the stamps must be a list")?;
    if list.is_empty() {
        return Err("no stamps were given: give stamp at least one".to_string());
    }
    list.iter().enumerate()
        .map(|(i, item)| parse_stamp(item).map_err(|e| format!("stamp {}: {e}", i + 1)))
        .collect()
}

fn parse_stamp(item: &serde_json::Value) -> Result<Stamp, String> {
    let options = item.as_object().ok_or("is not an object of options")?;
    if let Some(name) = options.keys().find(|k| !OPTIONS.contains(&k.as_str())) {
        return Err(format!("has no option '{name}'"));
    }
    // A length or a scope as text, or a number, which is in pt.
    let text = |key: &str| -> Result<Option<String>, String> {
        match options.get(key) {
            None | Some(serde_json::Value::Null) => Ok(None),
            Some(serde_json::Value::String(s))   => Ok(Some(s.clone())),
            Some(serde_json::Value::Number(n))   => Ok(Some(n.to_string())),
            Some(other) => Err(format!("{key} must be text, not {other}")),
        }
    };
    let place = |key: &str| -> Result<Place, String> {
        match text(key)?.as_deref() {
            None | Some("center") => Ok(Place::Center),
            Some("start") => Ok(Place::Start),
            Some("end")   => Ok(Place::End),
            Some(other)   => Err(format!("{key} must be start, center or end, not '{other}'")),
        }
    };
    let side = |key: &str| -> Result<f32, String> {
        let value = text(key)?.ok_or_else(|| format!("has no {key}: give it the {key} of its box, such as 72pt"))?;
        let length = parse_measurement(&value)?;
        if !(length > 0.0 && length.is_finite()) {
            return Err(format!("{key} must be more than 0, not '{value}'"));
        }
        Ok(length)
    };

    let image = text("image")?.ok_or("has no image: give it a PNG's bytes")?;
    let page = text("page")?.ok_or("has no page: give it a page scope, such as 1, 2-3, last or each")?;
    let page = parse_page_scope(&page)?;
    let anchor = text("anchor")?.map(|a| parse_anchor(&a)).transpose()?;
    let x = text("x")?;
    let y = text("y")?;
    if anchor.is_none() && (x.is_none() || y.is_none()) {
        return Err("has no position: give it x and y, or an anchor".to_string());
    }
    let x = x.map(|v| parse_signed_measurement(&v)).transpose()?.unwrap_or(0.0);
    let y = y.map(|v| parse_signed_measurement(&v)).transpose()?.unwrap_or(0.0);
    let scale = match text("scale")?.as_deref() {
        None | Some("fit") => Scale::Fit,
        Some("fill")       => Scale::Fill,
        Some(other)        => return Err(format!("scale must be fit or fill, not '{other}'")),
    };
    let max_grow = match options.get("max-grow") {
        None | Some(serde_json::Value::Null) => 1.0,
        Some(value) => value.as_f64().filter(|g| *g >= 1.0 && g.is_finite())
            .ok_or_else(|| format!("max-grow must be a number from 1, not {value}"))? as f32,
    };

    Ok(Stamp {
        image,
        page,
        x,
        y,
        anchor,
        width: side("width")?,
        height: side("height")?,
        align: place("align")?,
        justify: place("justify")?,
        scale,
        max_grow,
    })
}

/// Draw each stamp over its pages of `pdf`, and return the file with the update appended: the input, byte for byte,
/// then the new objects, a cross-reference section and a trailer.
pub(crate) fn stamp(pdf: &[u8], stamps: &[Stamp], images: &ImageRegistry) -> Result<Vec<u8>, String> {
    // The images, decoded once each, by name, in the order the stamps first use them.
    let mut decoded: Vec<(String, Png)> = Vec::new();
    for (i, stamp) in stamps.iter().enumerate() {
        if decoded.iter().any(|(name, _)| *name == stamp.image) {
            continue;
        }
        let bytes = images.get(&stamp.image)
            .ok_or_else(|| format!("stamp {}: the image '{}' was not loaded", i + 1, stamp.image))?;
        let png = png::decode(bytes).map_err(|e| format!("stamp {}: {e}", i + 1))?;
        decoded.push((stamp.image.clone(), png));
    }
    crate::memory::check()?;

    if !pdf.windows(5).take(1024).any(|w| w == b"%PDF-") {
        return Err("the input is not a PDF: it doesn't start with %PDF-".to_string());
    }
    let prev = Document::load_mem(pdf).map_err(|e| format!("the PDF could not be read: {e}"))?;
    // `lopdf` opens a file with an owner password only by itself; one that needs a password to open loads with its
    // `/Encrypt` still in place and no pages.
    if prev.is_encrypted() {
        return Err("the PDF needs a password to open, and a stamp can't open it: stamp the file before it gets an open \
                    password, or remove the password first".to_string());
    }
    let pages = prev.get_pages();
    if pages.is_empty() {
        return Err("the PDF has no pages".to_string());
    }
    crate::memory::check()?;

    // The stamps on each page, in the order they were given, which is the order they are drawn in.
    let mut on_page: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, stamp) in stamps.iter().enumerate() {
        let indices = pages_of(&stamp.page, pages.len()).map_err(|e| format!("stamp {}: {e}", i + 1))?;
        for index in indices {
            let list = on_page.entry(index).or_default();
            if !list.contains(&i) {
                list.push(i);
            }
        }
    }

    let pdfa_part = pdfa_part(&prev);
    if pdfa_part == Some(1) {
        if let Some(i) = stamps.iter().position(|s| decoded.iter().any(|(name, png)| *name == s.image && png.alpha.is_some())) {
            return Err(format!(
                "stamp {}: the PNG has transparency, which a PDF/A-1 file can't hold: give it a PNG without an alpha \
                 channel, flattened onto white", i + 1,
            ));
        }
    }

    let mut update = Document::new_from_prev(&prev);

    // The sRGB profile of a PDF/A file's images, in which any output intent allows them.
    let srgb = pdfa_part.map(|_| {
        let mut dict = Dictionary::new();
        dict.set("N", 3);
        dict.set("Alternate", Object::Name(b"DeviceRGB".to_vec()));
        update.add_object(Stream::new(dict, pdfa::SRGB_PROFILE.to_vec()))
    });
    let mut image_ids: HashMap<&str, ObjectId> = HashMap::new();
    for (name, png) in &decoded {
        image_ids.insert(name.as_str(), add_image(&mut update, png, srgb));
    }
    crate::memory::check()?;

    // The `q` that saves the graphics state before a page's own content, which may leave it changed.
    let save = update.add_object(Stream::new(Dictionary::new(), b"q\n".to_vec()));
    let output_intent = prev.catalog().is_ok_and(|c| c.has(b"OutputIntents"));

    for (&index, list) in &on_page {
        let page_id = pages[&(index as u32 + 1)];
        let mut page = prev.get_dictionary(page_id).map_err(|e| format!("page {}: {e}", index + 1))?.clone();
        let geometry = Geometry::of(&prev, &page);

        // The page's own copy of its resources, which may be shared with other pages or inherited from its parent.
        let mut resources = match inherited(&prev, &page, b"Resources") {
            Some(Object::Dictionary(d)) => d.clone(),
            _ => Dictionary::new(),
        };
        let mut xobjects = match resources.get(b"XObject").ok().map(|o| prev.dereference(o)) {
            Some(Ok((_, Object::Dictionary(d)))) => d.clone(),
            _ => Dictionary::new(),
        };
        let mut names: HashMap<&str, String> = HashMap::new();
        let mut next = 0;
        let mut content = String::new();
        let had_content = !contents_of(&prev, &page).is_empty();
        if had_content {
            content.push_str("Q\n");
        }
        content.push_str("/Artifact BMC\n");
        let mut transparent = false;
        for &i in list {
            let stamp = &stamps[i];
            let png = &decoded.iter().find(|(name, _)| *name == stamp.image).expect("decoded above").1;
            transparent |= png.alpha.is_some();
            let name = names.entry(stamp.image.as_str()).or_insert_with(|| {
                while xobjects.has(format!("LpdfStamp{next}").as_bytes()) {
                    next += 1;
                }
                let name = format!("LpdfStamp{next}");
                xobjects.set(name.as_bytes(), Object::Reference(image_ids[stamp.image.as_str()]));
                name
            });
            let [a, b, c, d, e, f] = geometry.matrix(place_image(stamp, png, geometry.size()));
            content.push_str(&format!(
                "q {} {} {} {} {} {} cm /{name} Do Q\n", num(a), num(b), num(c), num(d), num(e), num(f),
            ));
        }
        content.push_str("EMC\n");
        resources.set("XObject", xobjects);

        let drawn = update.add_object(Stream::new(Dictionary::new(), content.into_bytes()));
        let mut new_contents = Vec::new();
        if had_content {
            new_contents.push(Object::Reference(save));
            new_contents.extend(contents_of(&prev, &page));
        }
        new_contents.push(Object::Reference(drawn));
        page.set("Contents", new_contents);
        page.set("Resources", resources);

        // PDF/A-2 and -3 want a page with transparency to name its blending space when the file has no output intent.
        if transparent && pdfa_part.is_some_and(|p| p >= 2) && !output_intent {
            let mut group = match page.get(b"Group").ok().map(|o| prev.dereference(o)) {
                Some(Ok((_, Object::Dictionary(d)))) => d.clone(),
                _ => {
                    let mut d = Dictionary::new();
                    d.set("Type", Object::Name(b"Group".to_vec()));
                    d.set("S", Object::Name(b"Transparency".to_vec()));
                    d
                }
            };
            if !group.has(b"CS") {
                let srgb = srgb.expect("a PDF/A file has the profile");
                group.set("CS", vec![Object::Name(b"ICCBased".to_vec()), Object::Reference(srgb)]);
            }
            page.set("Group", group);
        }
        update.set_object(page_id, page);
    }
    crate::memory::check()?;

    let mut file = IncrementalDocument::create_from(pdf.to_vec(), prev);
    file.new_document = update;
    let mut out = Vec::with_capacity(pdf.len() + 4096);
    file.save_to(&mut out).map_err(|e| format!("the stamped PDF could not be written: {e}"))?;
    crate::memory::check()?;
    Ok(out)
}

/// The 0-based pages a stamp's scope names in a PDF of `count` pages. A page number past the end is an error.
fn pages_of(scope: &PageScope, count: usize) -> Result<Vec<usize>, String> {
    if let PageScope::Pages(ranges) = scope {
        for range in ranges {
            let past = |page: u32| page as usize > count;
            if range.start == 0 {
                return Err("pages count from 1, and the page is 0".to_string());
            }
            if let Some(page) = Some(range.start).filter(|&p| past(p)).or(range.end.filter(|&p| past(p))) {
                let pages = if count == 1 { "1 page".to_string() } else { format!("{count} pages") };
                return Err(format!("the PDF has no page {page}: it has {pages}"));
            }
            if range.end.is_some_and(|end| end < range.start) {
                return Err(format!("the range {}-{} runs backwards", range.start, range.end.unwrap_or(0)));
            }
        }
    }
    Ok(page_scope_indices(&Some(scope.clone()), count, 1))
}

/// The part of PDF/A a file says it conforms to in its XMP packet (`pdfaid:part`), if any.
fn pdfa_part(doc: &Document) -> Option<u8> {
    let metadata = doc.catalog().ok()?.get(b"Metadata").ok()?;
    let stream = doc.dereference(metadata).ok()?.1.as_stream().ok()?;
    let content = stream.decompressed_content().unwrap_or_else(|_| stream.content.clone());
    pdfaid_part(&String::from_utf8_lossy(&content))
}

/// `pdfaid:part` of an XMP packet, written as an attribute, `pdfaid:part="2"`, or as an element.
fn pdfaid_part(xmp: &str) -> Option<u8> {
    let at = xmp.find("pdfaid:part")? + "pdfaid:part".len();
    let rest = xmp[at..].trim_start();
    let value = match rest.strip_prefix('=') {
        Some(attribute) => attribute.trim_start().trim_start_matches(['"', '\'']),
        None => rest.strip_prefix('>')?,
    };
    value.trim_start().chars().next()?.to_digit(10).map(|d| d as u8)
}

/// Add a PNG as an image XObject, with its alpha as a soft mask: in the sRGB profile `srgb` when the file is PDF/A,
/// in the device's gray or RGB otherwise.
fn add_image(doc: &mut Document, png: &Png, srgb: Option<ObjectId>) -> ObjectId {
    let image = |space: Object, bytes: &[u8]| {
        let mut dict = Dictionary::new();
        dict.set("Type", Object::Name(b"XObject".to_vec()));
        dict.set("Subtype", Object::Name(b"Image".to_vec()));
        dict.set("Width", png.width as i64);
        dict.set("Height", png.height as i64);
        dict.set("ColorSpace", space);
        dict.set("BitsPerComponent", 8);
        dict.set("Filter", Object::Name(b"FlateDecode".to_vec()));
        Stream::new(dict, compress_to_vec_zlib(bytes, 6))
    };
    let mask = png.alpha.as_ref().map(|alpha| doc.add_object(image(Object::Name(b"DeviceGray".to_vec()), alpha)));
    let mut stream = match srgb {
        Some(profile) => image(vec![Object::Name(b"ICCBased".to_vec()), Object::Reference(profile)].into(), &png.rgb()),
        None => {
            let space = if png.colors == 1 { "DeviceGray" } else { "DeviceRGB" };
            image(Object::Name(space.as_bytes().to_vec()), &png.pixels)
        }
    };
    if let Some(mask) = mask {
        stream.dict.set("SMask", Object::Reference(mask));
    }
    doc.add_object(stream)
}

/// Where the image of a stamp lands on a page of the given size as a viewer shows it: `[x, y, width, height]`, from
/// the top-left corner, y down.
fn place_image(stamp: &Stamp, png: &Png, (page_w, page_h): (f32, f32)) -> [f32; 4] {
    let (x, y) = match &stamp.anchor {
        Some(anchor) => {
            let (ax, ay) = resolve_anchor(anchor, page_w, page_h);
            apply_anchor_offset(anchor, ax + stamp.x, ay + stamp.y, stamp.width, stamp.height)
        }
        None => (stamp.x, stamp.y),
    };
    // The natural size: from the resolution the PNG records, or a pixel to a point.
    let (natural_w, natural_h) = match png.dpi {
        Some((across, down)) => (png.width as f32 / across * 72.0, png.height as f32 / down * 72.0),
        None => (png.width as f32, png.height as f32),
    };
    let fill = (stamp.width / natural_w).min(stamp.height / natural_h);
    let ratio = match stamp.scale {
        Scale::Fill => fill,
        Scale::Fit  => fill.min(stamp.max_grow),
    };
    let (w, h) = (natural_w * ratio, natural_h * ratio);
    let offset = |place: Place, room: f32| match place {
        Place::Start  => 0.0,
        Place::Center => room / 2.0,
        Place::End    => room,
    };
    [x + offset(stamp.align, stamp.width - w), y + offset(stamp.justify, stamp.height - h), w, h]
}

/// A page as a viewer shows it: its crop box, in the PDF's own space, and the turn `/Rotate` gives it.
struct Geometry {
    left:   f32,
    bottom: f32,
    right:  f32,
    top:    f32,
    /// Clockwise: 0, 90, 180 or 270.
    rotate: i64,
}

impl Geometry {
    fn of(doc: &Document, page: &Dictionary) -> Self {
        // US Letter when a page has no media box, as viewers take it.
        let media = inherited(doc, page, b"MediaBox").and_then(|o| rect(doc, o)).unwrap_or([0.0, 0.0, 612.0, 792.0]);
        // The crop box, cut to the media box.
        let [left, bottom, right, top] = match inherited(doc, page, b"CropBox").and_then(|o| rect(doc, o)) {
            Some(crop) => {
                let cut = [crop[0].max(media[0]), crop[1].max(media[1]), crop[2].min(media[2]), crop[3].min(media[3])];
                if cut[0] < cut[2] && cut[1] < cut[3] { cut } else { media }
            }
            None => media,
        };
        let rotate = inherited(doc, page, b"Rotate")
            .and_then(|o| doc.dereference(o).ok()?.1.as_i64().ok())
            .map(|r| r.rem_euclid(360))
            .filter(|r| r % 90 == 0)
            .unwrap_or(0);
        Geometry { left, bottom, right, top, rotate }
    }

    /// The page's width and height as a viewer shows it.
    fn size(&self) -> (f32, f32) {
        let (w, h) = (self.right - self.left, self.top - self.bottom);
        if self.rotate % 180 == 90 { (h, w) } else { (w, h) }
    }

    /// The point of the PDF's space under the point `(u, v)` of the page as a viewer shows it, from its top-left
    /// corner, y down.
    fn point(&self, u: f32, v: f32) -> (f32, f32) {
        match self.rotate {
            90  => (self.left + v,  self.bottom + u),
            180 => (self.right - u, self.bottom + v),
            270 => (self.right - v, self.top - u),
            _   => (self.left + u,  self.top - v),
        }
    }

    /// The matrix that draws an image, a unit square, in `[x, y, w, h]` of the page as a viewer shows it, upright.
    fn matrix(&self, [x, y, w, h]: [f32; 4]) -> [f32; 6] {
        let origin = self.point(x, y + h);
        let across = self.point(x + w, y + h);
        let up     = self.point(x, y);
        [across.0 - origin.0, across.1 - origin.1, up.0 - origin.0, up.1 - origin.1, origin.0, origin.1]
    }
}

/// The value of an attribute a page may inherit from its parents (PDF 1.7, 7.7.3.4), resolved.
fn inherited<'a>(doc: &'a Document, page: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
    let mut node = page;
    // A cycle in the page tree stops here rather than looping.
    for _ in 0..64 {
        if let Ok(value) = node.get(key) {
            return doc.dereference(value).ok().map(|(_, o)| o);
        }
        node = node.get(b"Parent").ok().and_then(|p| doc.dereference(p).ok()?.1.as_dict().ok())?;
    }
    None
}

/// A rectangle, `[llx lly urx ury]` with its corners in either order, as left, bottom, right, top.
fn rect(doc: &Document, value: &Object) -> Option<[f32; 4]> {
    let items = value.as_array().ok()?;
    let n: Vec<f32> = items.iter().filter_map(|o| doc.dereference(o).ok()?.1.as_float().ok()).collect();
    let [x1, y1, x2, y2] = n[..] else { return None };
    Some([x1.min(x2), y1.min(y2), x1.max(x2), y1.max(y2)])
}

/// The references to a page's content streams, from a stream, an array or a reference to an array, flattened.
fn contents_of(doc: &Document, page: &Dictionary) -> Vec<Object> {
    match page.get(b"Contents") {
        Ok(Object::Reference(id)) => match doc.get_object(*id) {
            Ok(Object::Stream(_)) => vec![Object::Reference(*id)],
            Ok(Object::Array(items)) => items.iter().filter(|o| matches!(o, Object::Reference(_))).cloned().collect(),
            _ => Vec::new(),
        },
        Ok(Object::Array(items)) => items.iter().filter(|o| matches!(o, Object::Reference(_))).cloned().collect(),
        _ => Vec::new(),
    }
}

/// A number for a content stream: four decimals at most, no trailing zeros.
fn num(value: f32) -> String {
    let text = format!("{:.4}", value);
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" { "0".to_string() } else { text.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn stamp(options: serde_json::Value) -> Result<Stamp, String> {
        parse_stamps(&json!([options])).map(|mut s| s.remove(0))
    }

    /// A stamp with a box of `width` by `height` at the page's top-left corner.
    fn boxed(width: f32, height: f32) -> Stamp {
        Stamp {
            image: "i".into(), page: PageScope::Each, x: 0.0, y: 0.0, anchor: None, width, height,
            align: Place::Center, justify: Place::Center, scale: Scale::Fit, max_grow: 1.0,
        }
    }

    fn image(width: u32, height: u32, dpi: Option<f32>) -> Png {
        Png { width, height, colors: 3, pixels: Vec::new(), alpha: None, dpi: dpi.map(|d| (d, d)) }
    }

    fn close(a: [f32; 4], b: [f32; 4]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.01)
    }

    #[test]
    fn a_stamp_reads_its_options_with_their_defaults() {
        let s = stamp(json!({ "image": "sig", "page": "last", "x": "72pt", "y": "1in", "width": "144pt", "height": 36 })).unwrap();
        assert_eq!((s.image.as_str(), s.page, s.x, s.y), ("sig", PageScope::Last, 72.0, 72.0));
        assert_eq!((s.width, s.height, s.anchor), (144.0, 36.0, None));
        assert_eq!((s.align, s.justify, s.scale, s.max_grow), (Place::Center, Place::Center, Scale::Fit, 1.0));
        let s = stamp(json!({ "image": "seal", "page": "1-2", "anchor": "bottom-right", "x": "-10mm", "width": "1in",
            "height": "1in", "align": "end", "justify": "start", "scale": "fill", "max-grow": 1.5 })).unwrap();
        assert_eq!((s.anchor, s.y, s.align, s.justify, s.scale, s.max_grow),
            (Some(Anchor::BottomRight), 0.0, Place::End, Place::Start, Scale::Fill, 1.5));
        assert!((s.x + 28.346).abs() < 0.01);
    }

    #[test]
    fn each_missing_or_wrong_option_is_an_error_that_says_what_to_give() {
        let base = json!({ "image": "i", "page": "1", "x": "0", "y": "0", "width": "10", "height": "10" });
        let without = |key: &str| { let mut o = base.clone(); o.as_object_mut().unwrap().remove(key); stamp(o).err().unwrap() };
        let with = |key: &str, value: serde_json::Value| { let mut o = base.clone(); o[key] = value; stamp(o).err().unwrap() };
        assert_eq!(without("page"), "stamp 1: has no page: give it a page scope, such as 1, 2-3, last or each");
        assert_eq!(without("image"), "stamp 1: has no image: give it a PNG's bytes");
        assert_eq!(without("x"), "stamp 1: has no position: give it x and y, or an anchor");
        assert_eq!(without("width"), "stamp 1: has no width: give it the width of its box, such as 72pt");
        assert_eq!(with("height", json!("0pt")), "stamp 1: height must be more than 0, not '0pt'");
        assert_eq!(with("align", json!("left")), "stamp 1: align must be start, center or end, not 'left'");
        assert_eq!(with("scale", json!("crop")), "stamp 1: scale must be fit or fill, not 'crop'");
        assert_eq!(with("max-grow", json!(0.5)), "stamp 1: max-grow must be a number from 1, not 0.5");
        assert_eq!(with("maxGrow", json!(2)), "stamp 1: has no option 'maxGrow'");
        assert_eq!(parse_stamps(&json!([])).err().unwrap(), "no stamps were given: give stamp at least one");
        assert_eq!(parse_stamps(&json!([base, 3])).err().unwrap(), "stamp 2: is not an object of options");
    }

    #[test]
    fn fit_keeps_a_small_image_at_its_natural_size_and_shrinks_a_large_one() {
        // 50 by 25 px at a pixel to a point, in a box of 200 by 100: kept, in the box's middle.
        assert!(close(place_image(&boxed(200.0, 100.0), &image(50, 25, None), (612.0, 792.0)), [75.0, 37.5, 50.0, 25.0]));
        // 400 by 100 px: halved to the box's width, and centered down it.
        assert!(close(place_image(&boxed(200.0, 100.0), &image(400, 100, None), (612.0, 792.0)), [0.0, 25.0, 200.0, 50.0]));
    }

    #[test]
    fn the_natural_size_comes_from_the_dpi_the_png_records() {
        // 600 px at 300 dpi is 2 in, 144 pt.
        let placed = place_image(&boxed(300.0, 300.0), &image(600, 300, Some(300.0)), (612.0, 792.0));
        assert!(close(placed, [78.0, 114.0, 144.0, 72.0]), "{placed:?}");
    }

    #[test]
    fn max_grow_lets_fit_grow_a_small_image_up_to_it_but_never_past_the_box() {
        let mut s = boxed(200.0, 100.0);
        s.max_grow = 1.5;
        assert!(close(place_image(&s, &image(50, 25, None), (612.0, 792.0)), [62.5, 31.25, 75.0, 37.5]));
        s.max_grow = 10.0;
        assert!(close(place_image(&s, &image(50, 25, None), (612.0, 792.0)), [0.0, 0.0, 200.0, 100.0]));
    }

    #[test]
    fn fill_takes_the_largest_size_that_fits_the_box() {
        let mut s = boxed(200.0, 200.0);
        s.scale = Scale::Fill;
        assert!(close(place_image(&s, &image(50, 25, None), (612.0, 792.0)), [0.0, 50.0, 200.0, 100.0]));
    }

    #[test]
    fn align_and_justify_place_the_image_in_the_room_the_box_leaves() {
        let mut s = boxed(200.0, 100.0);
        s.x = 10.0;
        s.y = 20.0;
        let png = image(50, 25, None);
        for (align, justify, at) in [
            (Place::Start, Place::Start, [10.0, 20.0]),
            (Place::End, Place::Start, [160.0, 20.0]),
            (Place::Start, Place::End, [10.0, 95.0]),
            (Place::End, Place::End, [160.0, 95.0]),
            (Place::Center, Place::End, [85.0, 95.0]),
        ] {
            s.align = align;
            s.justify = justify;
            assert!(close(place_image(&s, &png, (612.0, 792.0)), [at[0], at[1], 50.0, 25.0]), "{align:?} {justify:?}");
        }
    }

    #[test]
    fn an_anchor_places_the_box_from_that_point_of_the_page() {
        let mut s = boxed(100.0, 50.0);
        s.anchor = Some(Anchor::BottomRight);
        s.x = -36.0;
        s.y = -36.0;
        // The box's bottom-right corner 36 pt in from the page's, and the 100 by 50 image fills it.
        assert!(close(place_image(&s, &image(100, 50, None), (612.0, 792.0)), [476.0, 706.0, 100.0, 50.0]));
        s.anchor = Some(Anchor::Center);
        s.x = 0.0;
        s.y = 0.0;
        assert!(close(place_image(&s, &image(100, 50, None), (612.0, 792.0)), [256.0, 371.0, 100.0, 50.0]));
    }

    fn geometry(rotate: i64) -> Geometry {
        // A crop box of 400 by 600 at (100, 50).
        Geometry { left: 100.0, bottom: 50.0, right: 500.0, top: 650.0, rotate }
    }

    #[test]
    fn a_page_turned_by_a_quarter_shows_its_sides_swapped() {
        assert_eq!(geometry(0).size(), (400.0, 600.0));
        assert_eq!(geometry(90).size(), (600.0, 400.0));
        assert_eq!(geometry(180).size(), (400.0, 600.0));
        assert_eq!(geometry(270).size(), (600.0, 400.0));
    }

    #[test]
    fn the_image_lands_upright_in_its_box_on_each_turn_of_the_page() {
        // A box 10 pt from the left and 20 pt from the top of the page as shown, 30 wide and 40 high: the image's
        // top-left corner, its top-right and its bottom-left, in the PDF's space.
        let corners = |rotate| {
            let [a, b, c, d, e, f] = geometry(rotate).matrix([10.0, 20.0, 30.0, 40.0]);
            let at = |x: f32, y: f32| (a * x + c * y + e, b * x + d * y + f);
            (at(0.0, 1.0), at(1.0, 1.0), at(0.0, 0.0))
        };
        // Upright: the top-left corner is 10 in from the crop box's left and 20 down from its top.
        assert_eq!(corners(0), ((110.0, 630.0), (140.0, 630.0), (110.0, 590.0)));
        // Turned clockwise: the shown top-left is the crop box's bottom-left, and the shown right is the PDF's up.
        assert_eq!(corners(90), ((120.0, 60.0), (120.0, 90.0), (160.0, 60.0)));
        assert_eq!(corners(180), ((490.0, 70.0), (460.0, 70.0), (490.0, 110.0)));
        assert_eq!(corners(270), ((480.0, 640.0), (480.0, 610.0), (440.0, 640.0)));
    }

    #[test]
    fn a_page_number_past_the_end_is_an_error_and_odd_or_even_take_what_there_is() {
        let scope = |s: &str| parse_page_scope(s).unwrap();
        assert_eq!(pages_of(&scope("2-last"), 3).unwrap(), [1, 2]);
        assert_eq!(pages_of(&scope("1,3"), 3).unwrap(), [0, 2]);
        assert_eq!(pages_of(&scope("even"), 1).unwrap(), Vec::<usize>::new());
        assert_eq!(pages_of(&scope("4"), 3).err().unwrap(), "the PDF has no page 4: it has 3 pages");
        assert_eq!(pages_of(&scope("1-2"), 1).err().unwrap(), "the PDF has no page 2: it has 1 page");
        assert_eq!(pages_of(&scope("0"), 3).err().unwrap(), "pages count from 1, and the page is 0");
        assert_eq!(pages_of(&scope("3-2"), 3).err().unwrap(), "the range 3-2 runs backwards");
    }

    #[test]
    fn the_pdfa_part_is_read_as_an_attribute_or_an_element() {
        assert_eq!(pdfaid_part(r#"<rdf:Description pdfaid:part="1" pdfaid:conformance="B"/>"#), Some(1));
        assert_eq!(pdfaid_part("<pdfaid:part>2</pdfaid:part><pdfaid:conformance>U</pdfaid:conformance>"), Some(2));
        assert_eq!(pdfaid_part("<pdfaid:part = '3'/>"), Some(3));
        assert_eq!(pdfaid_part("<dc:title>no standard</dc:title>"), None);
    }

    #[test]
    fn numbers_in_a_content_stream_have_four_decimals_at_most() {
        assert_eq!((num(72.0), num(0.5), num(-0.00001), num(1.23456), num(-12.5)), ("72".into(), "0.5".into(), "0".into(), "1.2346".into(), "-12.5".into()));
    }
}
