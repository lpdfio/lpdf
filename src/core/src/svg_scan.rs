//! What an SVG may hold, read off its XML before `usvg` builds anything from it (`svg.rs`).
//!
//! `usvg` is lenient: it drops without a word what it can't draw (`<text>`, `<image>`, `<foreignObject>`, a `<use>` of
//! another file), and it makes a shape of every copy a `use` stands for, so a file of a kilobyte whose `use` tags nest
//! five deep, ten each, asks for 100,000 shapes and 173 MB. This pass reads the XML once and
//! - refuses what the engine won't draw, with an error that names the tag and its line, and
//! - counts what the file expands to, which a limit stops before `usvg` runs: the elements, the path commands and the
//!   levels it nests.
//!
//! The counting follows every reference that makes `usvg` build more shapes: a `use`, a `clip-path`, a `mask`, a
//! `filter`, a pattern and a marker, which is built again at each vertex of its path.

use std::collections::{HashMap, HashSet};

use roxmltree21::{Document, Node, NodeId, ParsingOptions};

use crate::nesting::MAX_DEPTH;

/// The most elements an SVG may draw once its references are expanded.
pub(crate) const MAX_ELEMENTS: u64 = 100_000;

/// The most path commands an SVG may draw, each `use` copy counted again.
pub(crate) const MAX_PATH_COMMANDS: u64 = 1_000_000;

/// The most XML nodes a file may have as it is written, text between tags included.
const MAX_XML_NODES: u32 = 4 * MAX_ELEMENTS as u32;

const SVG_NAMESPACE: &str = "http://www.w3.org/2000/svg";
const XLINK_NAMESPACE: &str = "http://www.w3.org/1999/xlink";

/// What the scan learned about the file besides refusing or passing it.
pub(crate) struct Scan {
    /// Some paint says `currentColor`.
    pub(crate) current_color: bool,
    /// The root element sets `color` itself, so `currentColor` is that color and not the one of the text around the
    /// image.
    pub(crate) root_sets_color: bool,
    /// How many points one of the pixels `usvg` sizes the picture in comes to: 3/4 when the root's size is in `pt`, `pc`,
    /// `in`, `mm` or `cm`, which `usvg` reads at `USVG_DPI` pixels to the inch, so that such a size is the size it says,
    /// and 1 otherwise, since the engine takes a pixel as a point.
    pub(crate) points_per_pixel: f32,
}

/// The pixels to the inch `usvg` converts a length in an absolute unit at, which `svg.rs` gives it as its `dpi`.
pub(crate) const USVG_DPI: f32 = 96.0;

/// The units of a length that are true sizes, which a picture takes as they are.
const ABSOLUTE_UNITS: [&str; 5] = ["pt", "pc", "in", "cm", "mm"];

/// The other units a length of the root may have.
const RELATIVE_UNITS: [&str; 4] = ["px", "em", "ex", "%"];

/// Read the XML of an SVG, DTD and entities included, which `usvg` reads too.
pub(crate) fn parse_xml(text: &str) -> Result<Document<'_>, String> {
    let options = ParsingOptions { allow_dtd: true, nodes_limit: MAX_XML_NODES, ..ParsingOptions::default() };
    Document::parse_with_options(text, options).map_err(|error| match error {
        roxmltree21::Error::NodesLimitReached => {
            format!("the SVG has more than {MAX_XML_NODES} XML nodes: the limit is {MAX_XML_NODES}")
        }
        other => format!("the SVG could not be read: {other}"),
    })
}

/// Refuse what the engine won't draw, and what expands past a limit.
pub(crate) fn scan(doc: &Document) -> Result<Scan, String> {
    let root = doc.root_element();
    let mut found = Found::default();
    visit(root, 1, &mut found)?;
    let mut counter = Counter { ids: &found.ids, css_refs: &found.css_refs, memo: HashMap::new(), open: HashSet::new() };
    counter.cost(root, 1)?;
    Ok(Scan {
        current_color: found.current_color,
        root_sets_color: root.attribute("color").is_some() || style_declares(root.attribute("style"), "color"),
        points_per_pixel: points_per_pixel(root),
    })
}

/// How many points a pixel of the root's size is: 3/4 when the size is in an absolute unit. The `width` decides for both
/// axes, so the picture keeps its proportions; the `height` does when the `width` is no length.
fn points_per_pixel(root: Node) -> f32 {
    let absolute = |name: &str| root.attribute(name).and_then(is_absolute_length);
    match (absolute("width"), absolute("height")) {
        (Some(true), _) | (None, Some(true)) => 72.0 / USVG_DPI,
        _ => 1.0,
    }
}

/// Whether `value` is a length in an absolute unit, or `None` when it is no length. A length is read as `usvg` reads it
/// (`svgtypes`): white space may come before it and none after, then a number with an optional sign, fraction and
/// exponent, then a unit in lowercase, or none.
fn is_absolute_length(value: &str) -> Option<bool> {
    let value = value.trim_start_matches([' ', '\t', '\r', '\n']);
    let number_before = |unit: &&str| value.strip_suffix(*unit).is_some_and(is_number);
    if ABSOLUTE_UNITS.iter().any(number_before) {
        Some(true)
    } else if RELATIVE_UNITS.iter().any(number_before) || is_number(value) {
        Some(false)
    } else {
        None
    }
}

/// Whether `text` is a number as an SVG writes one. Rust's own reading also takes `inf` and `nan`, which no length is.
fn is_number(text: &str) -> bool {
    text.ends_with(|c: char| c.is_ascii_digit() || c == '.') && text.parse::<f32>().is_ok()
}

/// A refusal: `what` is the tag or the feature, `loc` where it is, `why` what can't be done.
pub(crate) fn refusal(what: &str, loc: &str, why: &str) -> String {
    format!("the SVG has {what}{loc}, which {why}")
}

/// " at line N, col M" of the first element whose `id` is `id`, or nothing when no element has it.
pub(crate) fn line_of(doc: &Document, id: &str) -> String {
    if id.is_empty() {
        return String::new();
    }
    doc.descendants()
        .find(|node| node.is_element() && node.attribute("id") == Some(id))
        .map(|node| loc(node))
        .unwrap_or_default()
}

fn loc(node: Node) -> String {
    let at = node.document().text_pos_at(node.range().start);
    format!(" at line {}, col {}", at.row, at.col)
}

/// What the first walk finds: the ids, what the style sheets refer to, and the colors.
#[derive(Default)]
struct Found<'a, 'input> {
    ids: HashMap<&'a str, Node<'a, 'input>>,
    /// The ids the `<style>` blocks name in a `url(#…)`, which any shape may take through a class.
    css_refs: Vec<&'a str>,
    current_color: bool,
}

/// The tags whose children draw nothing where they stand: the shapes they hold are drawn where something refers to
/// them.
fn is_definition(tag: &str) -> bool {
    matches!(
        tag,
        "defs" | "symbol" | "marker" | "clipPath" | "mask" | "pattern" | "linearGradient" | "radialGradient"
            | "filter" | "style" | "script" | "title" | "desc" | "metadata"
    )
}

/// An element of the SVG vocabulary, the only kind `usvg` reads.
fn is_svg_element(node: Node) -> bool {
    node.is_element() && node.tag_name().namespace() == Some(SVG_NAMESPACE)
}

/// The levels past the limit, as the error of the document's nesting says them.
fn too_deep(node: Node, depth: usize) -> String {
    format!(
        "<{}>{} is nested {depth} levels deep, past the limit of {MAX_DEPTH}: an SVG nests {MAX_DEPTH} levels at most, \
        its <use> tags followed",
        node.tag_name().name(),
        loc(node)
    )
}

/// Refuse what can't be drawn, in the element and below, and note its ids and colors. A level of nesting is a level
/// of this recursion, which `MAX_DEPTH` bounds.
fn visit<'a, 'input>(
    node: Node<'a, 'input>,
    depth: usize,
    found: &mut Found<'a, 'input>,
) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err(too_deep(node, depth));
    }
    crate::deadline::check();
    let tag = node.tag_name().name();
    match tag {
        "title" | "desc" | "metadata" => return Ok(()),
        "text" => {
            return Err(refusal("a <text>", &loc(node), "can't be drawn yet: convert the text to outlines"));
        }
        "image" => return Err(refusal("an <image>", &loc(node), "can't be drawn: an SVG can't hold a picture")),
        "foreignObject" => return Err(refusal("a <foreignObject>", &loc(node), "can't be drawn")),
        "feImage" => {
            // A filter is refused where it is used, but `usvg` would read what an `feImage` names before then.
            if href(node).is_none_or(|href| !href.starts_with('#')) {
                return Err(refusal(
                    "an <feImage>",
                    &loc(node),
                    "refers to another file or holds a picture, which an SVG may not: only #id references are followed",
                ));
            }
        }
        "style" => note_style(node, found)?,
        "use" => {
            if let Some(href) = href(node)
                && !href.starts_with('#')
            {
                return Err(refusal(
                    "a <use>",
                    &loc(node),
                    "refers to another file or a URL, which an SVG may not: only #id references are followed",
                ));
            }
        }
        _ => {}
    }
    for attribute in node.attributes() {
        let value = attribute.value();
        if attribute.name() == "id" && attribute.namespace().is_none() {
            found.ids.entry(value).or_insert(node);
        }
        if is_href(attribute.name(), attribute.namespace()) {
            continue;
        }
        found.current_color |= mentions(value, "currentcolor");
        if refers_elsewhere(value) {
            return Err(refusal(
                &format!("a `url(` in {}=\"…\"", attribute.name()),
                &loc(node),
                "refers to another file or a URL, which an SVG may not: only #id references are followed",
            ));
        }
    }
    for child in node.children().filter(|child| is_svg_element(*child)) {
        visit(child, depth + 1, found)?;
    }
    Ok(())
}

/// Read a `<style>` block: refuse an `@import` and a `url()` of another file, and note the ids it refers to.
fn note_style<'a, 'input>(node: Node<'a, 'input>, found: &mut Found<'a, 'input>) -> Result<(), String> {
    let css: &'a str = node.children().find_map(|child| child.text()).unwrap_or("");
    if mentions(css, "@import") || refers_elsewhere(css) {
        return Err(refusal(
            "a <style> with an @import or a `url()` of another file",
            &loc(node),
            "can't be followed: an SVG may not refer to another file or a URL",
        ));
    }
    found.current_color |= mentions(css, "currentcolor");
    found.css_refs.extend(url_ids(css));
    Ok(())
}

/// The `href` of an element, whichever way it is spelled.
fn href<'a>(node: Node<'a, '_>) -> Option<&'a str> {
    node.attributes()
        .find(|attribute| is_href(attribute.name(), attribute.namespace()))
        .map(|attribute| attribute.value())
}

fn is_href(name: &str, namespace: Option<&str>) -> bool {
    name == "href" && matches!(namespace, None | Some(XLINK_NAMESPACE))
}

/// Whether `haystack` holds `needle`, whatever the case of its letters. `needle` is lowercase.
fn mentions(haystack: &str, needle: &str) -> bool {
    haystack.as_bytes().windows(needle.len()).any(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

/// Whether the style declares `property`, as in `style="fill: red; color: blue"`.
fn style_declares(style: Option<&str>, property: &str) -> bool {
    style.is_some_and(|style| {
        style.split(';').any(|declaration| {
            declaration.split(':').next().is_some_and(|name| name.trim().eq_ignore_ascii_case(property))
        })
    })
}

/// What follows each `url(` in `value`, with its spaces and quotes taken off.
fn url_targets(value: &str) -> impl Iterator<Item = &str> {
    let bytes = value.as_bytes();
    (0..bytes.len().saturating_sub(3))
        .filter(move |at| bytes[*at..*at + 4].eq_ignore_ascii_case(b"url("))
        .map(move |at| value[at + 4..].trim_start_matches([' ', '\t', '\r', '\n', '"', '\'']))
}

/// Whether `value` has a `url(` that points anywhere but at an element of the same file.
fn refers_elsewhere(value: &str) -> bool {
    url_targets(value).any(|target| !target.starts_with('#'))
}

/// The ids `value` names in a `url(#id)`.
fn url_ids(value: &str) -> impl Iterator<Item = &str> {
    url_targets(value).filter_map(|target| {
        let target = target.strip_prefix('#')?;
        let end = target.find([')', '"', '\'', ' ']).unwrap_or(target.len());
        Some(&target[..end])
    })
}

/// A count of what an element draws.
#[derive(Clone, Copy, Default)]
struct Cost {
    elements: u64,
    commands: u64,
    /// The levels from the element down to the deepest thing it draws, itself one.
    depth: usize,
}

impl Cost {
    fn plus(self, other: Cost) -> Cost {
        Cost {
            elements: self.elements.saturating_add(other.elements),
            commands: self.commands.saturating_add(other.commands),
            depth: self.depth.max(other.depth),
        }
    }

    fn times(self, factor: u64) -> Cost {
        Cost { elements: self.elements.saturating_mul(factor), commands: self.commands.saturating_mul(factor), ..self }
    }

    /// What an element that draws this holds: one level deeper.
    fn below(self) -> Cost {
        Cost { depth: self.depth + 1, ..self }
    }

    fn check(self) -> Result<(), String> {
        if self.elements > MAX_ELEMENTS {
            return Err(format!(
                "the SVG draws more than {MAX_ELEMENTS} elements once its <use>, marker and clip references are \
                followed: the limit is {MAX_ELEMENTS}"
            ));
        }
        if self.commands > MAX_PATH_COMMANDS {
            return Err(format!(
                "the SVG has more than {MAX_PATH_COMMANDS} path commands, each <use> copy counted again: the limit is \
                {MAX_PATH_COMMANDS}"
            ));
        }
        Ok(())
    }
}

/// Counts what each element draws once its references are followed, each element once.
struct Counter<'f, 'a, 'input> {
    ids: &'f HashMap<&'a str, Node<'a, 'input>>,
    css_refs: &'f [&'a str],
    memo: HashMap<NodeId, Cost>,
    /// The elements being counted, which a reference may not come back to.
    open: HashSet<NodeId>,
}

impl<'a, 'input> Counter<'_, 'a, 'input> {
    /// What `node` draws: itself, its children that draw where they stand, and what it refers to. A shape that refers
    /// to a marker draws it again at each vertex.
    fn cost(&mut self, node: Node<'a, 'input>, depth: usize) -> Result<Cost, String> {
        if let Some(known) = self.memo.get(&node.id()) {
            // Counted before, at another level: it nests as deep below this one.
            let deepest = depth + known.depth - 1;
            if deepest > MAX_DEPTH {
                return Err(too_deep(node, deepest));
            }
            return Ok(*known);
        }
        if depth > MAX_DEPTH {
            return Err(too_deep(node, depth));
        }
        if !self.open.insert(node.id()) {
            return Err(refusal(
                &format!("a <{}> that refers to itself", node.tag_name().name()),
                &loc(node),
                "can't be drawn: its references make a loop",
            ));
        }
        crate::deadline::check();
        crate::memory::check()?;
        let (ids, css_refs) = (self.ids, self.css_refs);
        let own = own_commands(node);
        let mut total = Cost { elements: 1, commands: own, depth: 1 };
        for child in node.children().filter(|child| is_svg_element(*child) && !is_definition(child.tag_name().name())) {
            total = total.plus(self.cost(child, depth + 1)?.below());
            total.check()?;
        }
        if node.tag_name().name() == "use"
            && let Some(target) = href(node).and_then(|href| href.strip_prefix('#')).and_then(|id| ids.get(id).copied())
        {
            total = total.plus(self.cost(target, depth + 1)?.below());
        }
        for id in references(node, own > 0, css_refs) {
            if let Some(target) = ids.get(id).copied() {
                // A style sheet's rule reaches the shapes of what it refers to too, which is no loop, as an attribute
                // that refers to its own element is.
                if self.open.contains(&target.id()) && css_refs.contains(&id) {
                    continue;
                }
                let drawn = self.cost(target, depth + 1)?;
                let copies = if target.tag_name().name() == "marker" { own.max(1) } else { 1 };
                total = total.plus(drawn.times(copies).below());
            }
        }
        total.check()?;
        self.open.remove(&node.id());
        self.memo.insert(node.id(), total);
        Ok(total)
    }
}

/// The ids `node` refers to through its properties, in attributes and in `style`, and, for a shape, through the
/// style sheets. A gradient costs nothing: it is no shape.
fn references<'a>(node: Node<'a, '_>, is_shape: bool, css_refs: &[&'a str]) -> Vec<&'a str> {
    let mut ids: Vec<&str> = node
        .attributes()
        .filter(|attribute| !is_href(attribute.name(), attribute.namespace()))
        .flat_map(|attribute| url_ids(attribute.value()))
        .collect();
    if is_shape {
        ids.extend_from_slice(css_refs);
    }
    ids
}

/// The commands of a shape: its path segments, its points, or the one of a basic shape. A marker is drawn at each.
fn own_commands(node: Node) -> u64 {
    match node.tag_name().name() {
        "path" => node.attribute("d").map_or(0, path_segments),
        "polyline" | "polygon" => node.attribute("points").map_or(0, |points| (count_numbers(points) / 2).max(1)),
        "line" => 2,
        "rect" | "circle" | "ellipse" => 1,
        _ => 0,
    }
}

/// The segments of path data: each command letter, and each further set of numbers a command repeats on.
fn path_segments(data: &str) -> u64 {
    let bytes = data.as_bytes();
    let (mut segments, mut arity, mut numbers, mut at) = (0u64, 0usize, 0usize, 0usize);
    while at < bytes.len() {
        let byte = bytes[at];
        if byte.is_ascii_alphabetic() {
            segments += 1;
            arity = arity_of(byte);
            numbers = 0;
            at += 1;
        } else if byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'+') {
            at = after_number(bytes, at);
            numbers += 1;
            if arity > 0 && numbers > arity && (numbers - 1) % arity == 0 {
                segments += 1;
            }
        } else {
            at += 1;
        }
    }
    segments
}

/// The numbers one segment of the path command `letter` takes.
fn arity_of(letter: u8) -> usize {
    match letter.to_ascii_lowercase() {
        b'm' | b'l' | b't' => 2,
        b'h' | b'v' => 1,
        b'c' => 6,
        b's' | b'q' => 4,
        b'a' => 7,
        _ => 0,
    }
}

fn count_numbers(text: &str) -> u64 {
    let bytes = text.as_bytes();
    let (mut count, mut at) = (0u64, 0usize);
    while at < bytes.len() {
        if bytes[at].is_ascii_digit() || matches!(bytes[at], b'.' | b'-' | b'+') {
            at = after_number(bytes, at);
            count += 1;
        } else {
            at += 1;
        }
    }
    count
}

/// Where the number that starts at `start` ends: a sign, digits, a point, digits and an exponent. Always past `start`.
fn after_number(bytes: &[u8], start: usize) -> usize {
    let digits = |from: usize| from + bytes[from..].iter().take_while(|byte| byte.is_ascii_digit()).count();
    let mut at = start;
    if matches!(bytes[at], b'+' | b'-') {
        at += 1;
    }
    at = digits(at);
    if bytes.get(at) == Some(&b'.') {
        at = digits(at + 1);
    }
    if matches!(bytes.get(at), Some(b'e' | b'E')) {
        let mut exponent = at + 1;
        if matches!(bytes.get(exponent), Some(b'+' | b'-')) {
            exponent += 1;
        }
        if bytes.get(exponent).is_some_and(u8::is_ascii_digit) {
            at = digits(exponent);
        }
    }
    at.max(start + 1)
}

/// A color the file doesn't write, to stand in for `currentColor` while `usvg` reads it, which resolves that word to
/// the color of the `color` property: the SVG is read with this one as the root's `color`, and each paint that comes
/// back as it is the text color of the page.
///
/// The color is picked so that no spelling the file uses for it is in the file: `#0102xx`, and `1,2,xx` written with
/// commas or spaces.
pub(crate) fn stand_in_color(text: &str) -> (u8, u8, u8) {
    let squeezed = squeezed_separators(text);
    (3..=u8::MAX)
        .find(|blue| !mentions(text, &format!("#0102{blue:02x}")) && !squeezed.contains(&format!("1,2,{blue}")))
        .map_or((1, 2, 3), |blue| (1, 2, blue))
}

/// `text` with each run of spaces and commas as one comma, so that `rgb(1 2 3)` and `rgb(1, 2, 3)` read alike.
fn squeezed_separators(text: &str) -> String {
    let mut squeezed = String::with_capacity(text.len());
    let mut in_separator = false;
    for character in text.chars() {
        if character.is_whitespace() || character == ',' {
            if !in_separator {
                squeezed.push(',');
            }
            in_separator = true;
        } else {
            squeezed.push(character.to_ascii_lowercase());
            in_separator = false;
        }
    }
    squeezed
}
