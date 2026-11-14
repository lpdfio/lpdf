//! SVG images, drawn as vector.
//!
//! An SVG handed to `load_image` is read once, when it is loaded, into a `Drawing`: its size, and the form XObjects
//! that draw it. The XML is read by `roxmltree` and checked by `svg_scan.rs`, which refuses what the engine won't draw
//! and what expands past a limit; `usvg` then resolves `use`, `<style>`, units and markers into a tree of groups and
//! paths, which `svg_draw.rs` writes as PDF content. The form's box is the unit square, so the image draws where a
//! JPEG or a PNG would: the layout scales the unit square with the CTM, as it does for every image.

use miniz_oxide::deflate::compress_to_vec_zlib;
use usvg::Tree;

use crate::svg_draw::{self, DrawnForm};
use crate::{svg_entities, svg_scan};

/// A form XObject of the picture, its content compressed.
#[derive(Debug)]
pub(crate) struct Form {
    pub(crate) stream: Vec<u8>,
    pub(crate) bbox: [f32; 4],
    /// The transparency groups the form invokes, by the index of their form.
    pub(crate) groups: Vec<usize>,
}

/// An SVG read and written as PDF content, ready to go into the file.
#[derive(Debug)]
pub(crate) struct Drawing {
    /// The size the picture is when its `<img>` gives none, in the pixels the engine takes as points, which the layout
    /// takes the proportions of the image from, as the file gives it: not rounded, which would distort a small picture.
    /// A size in pixels, or with no unit, is a pixel to the point; one in `pt`, `pc`, `in`, `mm` or `cm` is its true size.
    pub(crate) width: f32,
    pub(crate) height: f32,
    /// Some shape is painted in `currentColor`, the color of the text where the image sits, which the page sets
    /// before it draws the picture.
    pub(crate) current_color: bool,
    /// The picture first, then the transparency groups it invokes, named `G<index>`.
    pub(crate) forms: Vec<Form>,
    /// The fill and stroke alpha of each graphics state the forms name, `S<index>`.
    pub(crate) states: Vec<(f32, f32)>,
}

impl Drawing {
    /// How many objects the picture's file needs besides the first: one for each transparency group.
    pub(crate) fn extra_forms(&self) -> usize {
        self.forms.len() - 1
    }
}

pub(crate) use svg_draw::{form_name, state_name};

/// Whether `bytes` are an SVG document: after an optional byte order mark, whitespace, an XML declaration, comments
/// and a doctype, the root element is `svg`.
pub(crate) fn is_svg(bytes: &[u8]) -> bool {
    let mut rest = strip_bom(bytes);
    loop {
        rest = rest.trim_ascii_start();
        let skipped = if rest.starts_with(b"<?") {
            skip_past(rest, b"?>")
        } else if rest.starts_with(b"<!--") {
            skip_past(rest, b"-->")
        } else if rest.starts_with(b"<!DOCTYPE") {
            skip_doctype(rest)
        } else {
            return rest.starts_with(b"<svg") && rest.get(4).is_some_and(|c| c.is_ascii_whitespace() || *c == b'>');
        };
        match skipped {
            Some(after) => rest = after,
            None => return false,
        }
    }
}

fn strip_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)
}

/// What follows the first `end` in `bytes`.
fn skip_past<'a>(bytes: &'a [u8], end: &[u8]) -> Option<&'a [u8]> {
    bytes.windows(end.len()).position(|window| window == end).map(|at| &bytes[at + end.len()..])
}

/// What follows a doctype, whose internal subset in `[` `]` may hold a `>`.
fn skip_doctype(bytes: &[u8]) -> Option<&[u8]> {
    let mut depth = 0usize;
    for (at, byte) in bytes.iter().enumerate() {
        match byte {
            b'[' => depth += 1,
            b']' => depth = depth.saturating_sub(1),
            b'>' if depth == 0 => return Some(&bytes[at + 1..]),
            _ => {}
        }
    }
    None
}

/// Read an SVG and write its picture, or say why it can't be drawn.
pub(crate) fn prepare(bytes: &[u8]) -> Result<Drawing, String> {
    let text = std::str::from_utf8(strip_bom(bytes)).map_err(|_| "the SVG is not UTF-8 text".to_string())?;
    svg_entities::check_entities(text)?;
    let doc = svg_scan::parse_xml(text)?;
    let scan = svg_scan::scan(&doc)?;
    // `currentColor` reads as black unless the root sets `color`, so the SVG is read with a color of its own for the
    // root, and each paint that comes back as that color is the page's.
    let stand_in = (scan.current_color && !scan.root_sets_color).then(|| svg_scan::stand_in_color(text));
    let options = options(stand_in);
    let tree = Tree::from_xmltree(&doc, &options).map_err(|error| format!("the SVG could not be read: {error}"))?;
    let size = tree.size();
    let drawn = svg_draw::draw(&tree, &doc, stand_in)?;
    crate::memory::check()?;
    Ok(Drawing {
        width: size.width() * scan.points_per_pixel,
        height: size.height() * scan.points_per_pixel,
        current_color: drawn.current_color,
        forms: drawn.forms.into_iter().map(compressed).collect(),
        states: drawn.states,
    })
}

fn compressed(form: DrawnForm) -> Form {
    Form { stream: compress_to_vec_zlib(&form.content, 6), bbox: form.bbox, groups: form.groups }
}

/// What `usvg` reads the SVG with. It reads no file and no data URL: the scan refuses what names one, and this is the
/// second lock on that door, since the default resolvers read a local file by its path and parse an SVG held in a
/// data URL, neither of which the scan's limits would see.
fn options(stand_in: Option<(u8, u8, u8)>) -> usvg::Options<'static> {
    usvg::Options {
        dpi: svg_scan::USVG_DPI,
        style_sheet: stand_in.map(|(r, g, b)| format!("svg {{ color: rgb({r}, {g}, {b}) }}")),
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..usvg::Options::default()
    }
}

#[cfg(test)]
pub(crate) fn resolves_no_picture() -> bool {
    let options = options(None);
    (options.image_href_resolver.resolve_string)("/etc/hosts", &options).is_none()
        && (options.image_href_resolver.resolve_data)("image/svg+xml", std::sync::Arc::new(b"<svg/>".to_vec()), &options)
            .is_none()
}
