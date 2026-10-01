// Does every attribute in the schema do something?
//
// The schema is what the SDKs and the docs promise. For each attribute of each element this renders a small
// document twice, without the attribute and with it, and requires the two PDFs to differ. An attribute that
// changes nothing is accepted by the parser and then ignored, which no other test notices: the snapshots
// only fail when output changes, and an ignored attribute never changes it.
//
// Three more checks keep the test honest:
// - For an enumeration or a boolean, every value but the default has to change the PDF too, so a value the
//   engine does not implement (cluster align="stretch") is found as well as a whole attribute.
// - What the engine does not use yet is listed in IGNORED and IGNORED_VALUES with the reason, and the test
//   requires it to stay unused: the day it is implemented the test fails, and the entry goes.
// - Every attribute of the schema has to be tested, listed in UNTESTED or be a data-binding attribute, and every
//   entry here has to name something that is in the schema, so a new attribute cannot slip through.
//
// The values to try come from the schema's own types; free text (xs:string) gets its samples from `strings`.
// How an element is exercised is in `subjects`: the document around it and what it needs to show an effect.
// `tweaks` adjusts that for single attributes, such as a radius that only shows on a background.
//
// What it does not cover: it reads XML, and the engine's JSON path shares the parser's attribute code but not
// all of it; a value that is accepted and changes the PDF only for some content (a justify that needs a fixed
// height) is found only if the document here has that content; and a PDF that differs shows that the attribute
// is used, not that it does the right thing. Data-binding attributes (data-*) are tested in data.rs.
//
// Run it with output to see every verdict:
//   cargo test --manifest-path src/core/Cargo.toml attribute_effects -- --nocapture

use std::collections::{BTreeSet, HashMap};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;

use roxmltree::{Document, Node};

// ── Verdicts the engine is known to give ──────────────────────────────────────

/// Attributes the schema declares and the engine does not use yet: `element@attribute`, and why. The test
/// requires them to have no effect; when one is implemented, the test fails and the entry is removed.
const IGNORED: &[(&str, &str)] = &[
    ("canvas:rect@opacity", "a shape reads no opacity of its own; the layer's opacity applies"),
    ("canvas:circle@opacity", "a shape reads no opacity of its own; the layer's opacity applies"),
    ("canvas:ellipse@opacity", "a shape reads no opacity of its own; the layer's opacity applies"),
    ("canvas:path@opacity", "a shape reads no opacity of its own; the layer's opacity applies"),
    (
        "canvas:text@line-height",
        "it spaces the lines of a text, and the content is collapsed to one line before it is drawn, so there is never a second line",
    ),
    (
        "canvas:span@bold",
        "the runs of a canvas text are drawn as one string in the text's own font: a span's attributes are read and not used",
    ),
    (
        "canvas:span@color",
        "the runs of a canvas text are drawn as one string in the text's own font: a span's attributes are read and not used",
    ),
    (
        "canvas:span@href",
        "the runs of a canvas text are drawn as one string in the text's own font: a span's attributes are read and not used",
    ),
    (
        "canvas:span@underline",
        "the runs of a canvas text are drawn as one string in the text's own font: a span's attributes are read and not used",
    ),
    (
        "canvas:span@strike",
        "the runs of a canvas text are drawn as one string in the text's own font: a span's attributes are read and not used",
    ),
    ("layer@clip", "never read"),
    (
        "region@pin",
        "read and not used: a region is a header when it comes first in the layout and a footer when it comes last, whatever its pin says",
    ),
    ("region@w", "read and not used: it is for the left and right pins, which are not implemented"),
    ("section@title", "read and not used: the PDF has no outline to put it in"),
    ("divider@debug", "the debug outline is drawn for boxes, and a divider is not one"),
    ("field@debug", "the debug outline is drawn for boxes, and a field is not one"),
    ("img@font", "an image is laid out as the picture alone: the attribute is read and not used"),
    ("img@font-size", "an image is laid out as the picture alone: the attribute is read and not used"),
    ("img@gap", "an image is laid out as the picture alone: the attribute is read and not used"),
    ("img@padding", "an image is laid out as the picture alone: the attribute is read and not used"),
    ("img@background", "an image is laid out as the picture alone: the attribute is read and not used"),
    ("img@border", "an image is laid out as the picture alone: the attribute is read and not used"),
    ("img@radius", "an image is laid out as the picture alone: the attribute is read and not used"),
    ("img@debug", "an image is laid out as the picture alone: the attribute is read and not used"),
];

/// Values of an enumeration the engine does not use yet: `element@attribute=value`, and why. Each has to stay
/// without effect, like the entries of IGNORED.
const IGNORED_VALUES: &[(&str, &str)] = &[(
    "cluster@align=stretch",
    "a cluster does not stretch its children to the height of the line, so stretch places them as start does",
)];

/// Attributes that change the PDF file and not what a page shows: `element@attribute`, and why. The test cannot
/// tell the two apart; these were found by reading the engine. Each has to keep changing the PDF.
const INVISIBLE: &[(&str, &str)] =
    &[("canvas:span@font", "the run's font is embedded, and the run is drawn in the text's own font")];

/// Attributes this test cannot exercise: `element@attribute`, and why.
const UNTESTED: &[(&str, &str)] = &[
    ("lpdf@version", "selects the format; the parser accepts only 1, so there is no second value to render"),
    (
        "font@src",
        "read by the SDKs, which load the file and hand it to the engine under the font's name or ref; the engine never sees it",
    ),
    (
        "image@src",
        "read by the SDKs, which load the file and hand it to the engine under the image's name or ref; the engine never sees it",
    ),
];

// ── What to try for free text ─────────────────────────────────────────────────

/// Sample values for attributes that are free text, by `element@attribute` and then by attribute name.
fn strings(id: &str, name: &str) -> Option<&'static [&'static str]> {
    let by_id: &[(&str, &[&str])] = &[
        ("table@cols", &["2fr 1fr", "100pt 1fr"]),
        ("img@name", &["other"]),
        ("canvas:img@name", &["other"]),
        ("font@name", &["renamed"]),
        ("image@name", &["renamed"]),
        ("color@name", &["renamed"]),
        ("font@ref", &["custom"]),
        ("image@ref", &["other"]),
        ("barcode@data", &["Another payload"]),
    ];
    if let Some((_, values)) = by_id.iter().find(|(key, _)| *key == id) {
        return Some(values);
    }
    Some(match name {
        "font" => &["Courier"],
        "stroke-dash" => &["6 3"],
        "transform" => &["matrix(1,0,0,1,40,40)"],
        "clip" => &["rect(10 10 80 80)"],
        "d" => &["M 20 20 L 180 20 L 100 160 Z"],
        "name" => &["n2"],
        "value" => &["v2"],
        "label" => &["Label"],
        "options" => &["red, green, blue"],
        "group" => &["g1"],
        "href" => &["https://b.example"],
        "action-url" => &["https://c.example/submit"],
        "title" => &["Title"],
        "author" => &["Author"],
        "subject" => &["Subject"],
        "keywords" => &["key words"],
        "creator" => &["Creator"],
        _ => return None,
    })
}

/// Sample values for the schema's pattern types.
fn pattern_samples(type_name: &str) -> Vec<String> {
    let sample: &[&str] = match type_name {
        "PtValue" | "SignedPtValue" => &["37pt", "143pt"],
        "SpacingValue" => &["17pt", "61pt"],
        "PageSizeCustom" => &["300pt 400pt"],
        "ColorHex" | "Color" => &["#ff0000"],
        "BorderValue" => &["2pt #ff0000"],
        "PageScope" => &["2"],
        other => panic!("no sample value for the pattern type {other}: add one to pattern_samples"),
    };
    strs(sample)
}

// ── The schema ────────────────────────────────────────────────────────────────

/// Elements the schema declares once and the engine reads in more than one place: the name to try the second
/// place under, and the element it is.
const CONTEXTS: &[(&str, &str)] = &[("canvas:span", "span")];

/// One attribute of one element, with the values to try for it.
struct Attribute {
    /// The element: its name, `canvas:` in front of a drawing primitive, `tokens:` in front of a token row.
    element: String,
    name: String,
    required: bool,
    /// Empty for free text, which `strings` gives samples for.
    values: Vec<String>,
    /// True for an enumeration or a boolean: every value but the default has to change the PDF.
    one_of: bool,
}

impl Attribute {
    fn id(&self) -> String {
        format!("{}@{}", self.element, self.name)
    }
}

fn is(node: Node, tag: &str) -> bool {
    node.is_element() && node.tag_name().name() == tag
}

/// A top-level declaration of the schema, such as a simple type or an attribute group.
fn top<'a, 'i>(doc: &'a Document<'i>, tag: &str, name: &str) -> Option<Node<'a, 'i>> {
    doc.root_element().children().find(|n| is(*n, tag) && n.attribute("name") == Some(name))
}

fn attribute_nodes<'a, 'i>(doc: &'a Document<'i>, owner: Node<'a, 'i>, found: &mut Vec<Node<'a, 'i>>) {
    for child in owner.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "attribute" if child.attribute("name").is_some() => found.push(child),
            "attributeGroup" => {
                if let Some(group) = child.attribute("ref").and_then(|r| top(doc, "attributeGroup", r)) {
                    attribute_nodes(doc, group, found);
                }
            }
            _ => {}
        }
    }
}

fn declared_attributes<'a, 'i>(doc: &'a Document<'i>, declaration: Node<'a, 'i>) -> Vec<Node<'a, 'i>> {
    let complex = declaration
        .children()
        .find(|n| is(*n, "complexType"))
        .or_else(|| declaration.attribute("type").and_then(|t| top(doc, "complexType", t)));
    let mut found = Vec::new();
    if let Some(complex) = complex {
        attribute_nodes(doc, complex, &mut found);
    }
    found
}

fn strs(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| v.to_string()).collect()
}

/// The values to try for a named type, and whether the type is a plain enumeration.
fn values_of_type(doc: &Document, type_name: &str) -> (Vec<String>, bool) {
    match type_name {
        "xs:boolean" => (strs(&["true", "false"]), true),
        "xs:string" | "xs:anyURI" => (Vec::new(), false),
        "xs:positiveInteger" => (strs(&["2", "7"]), false),
        "xs:decimal" => (strs(&["0.5"]), false),
        other => {
            let simple = top(doc, "simpleType", other).unwrap_or_else(|| panic!("the schema has no type {other}"));
            values_of_simple(doc, simple, other)
        }
    }
}

fn values_of_simple(doc: &Document, simple: Node, name: &str) -> (Vec<String>, bool) {
    let inner = simple
        .children()
        .find(|n| is(*n, "restriction") || is(*n, "union"))
        .unwrap_or_else(|| panic!("the type {name} is neither a restriction nor a union"));

    if is(inner, "union") {
        let mut values = Vec::new();
        let mut one_of = true;
        for member in inner.attribute("memberTypes").unwrap_or("").split_whitespace() {
            let (v, o) = values_of_type(doc, member);
            values.extend(v);
            one_of &= o;
        }
        for member in inner.children().filter(|n| is(*n, "simpleType")) {
            let (v, o) = values_of_simple(doc, member, name);
            values.extend(v);
            one_of &= o;
        }
        return (values, one_of);
    }

    let enumeration: Vec<String> =
        inner.children().filter(|n| is(*n, "enumeration")).filter_map(|n| n.attribute("value")).map(str::to_string).collect();
    if !enumeration.is_empty() {
        return (enumeration, true);
    }
    if inner.children().any(|n| is(n, "pattern")) {
        return (pattern_samples(name), false);
    }
    (values_of_type(doc, inner.attribute("base").unwrap_or("xs:string")).0, false)
}

/// The attributes of the elements in CONTEXTS, once more under their other name.
fn with_contexts(mut attributes: Vec<Attribute>) -> Vec<Attribute> {
    for (alias, source) in CONTEXTS {
        let copies: Vec<Attribute> = attributes
            .iter()
            .filter(|a| a.element == *source)
            .map(|a| Attribute {
                element: alias.to_string(),
                name: a.name.clone(),
                required: a.required,
                values: a.values.clone(),
                one_of: a.one_of,
            })
            .collect();
        assert!(!copies.is_empty(), "CONTEXTS names {source}, which is not in the schema");
        attributes.extend(copies);
    }
    attributes
}

fn schema_attributes(doc: &Document) -> Vec<Attribute> {
    let mut found = Vec::new();
    for outer in doc.root_element().children().filter(|n| n.is_element()) {
        let (own, nested) = match (outer.tag_name().name(), outer.attribute("name")) {
            ("element", Some("tokens")) => ("", "tokens:"),
            ("element", Some(_)) => ("", ""),
            ("group", Some("CanvasPrimitives")) => ("canvas:", "canvas:"),
            _ => continue,
        };
        for declaration in outer.descendants().filter(|n| is(*n, "element") && n.attribute("name").is_some()) {
            let name = declaration.attribute("name").unwrap();
            let element = if declaration == outer { format!("{own}{name}") } else { format!("{nested}{name}") };
            let mut seen = BTreeSet::new();
            for attribute in declared_attributes(doc, declaration) {
                let attribute_name = attribute.attribute("name").unwrap().to_string();
                if !seen.insert(attribute_name.clone()) {
                    continue;
                }
                let (values, one_of) = match attribute.attribute("type") {
                    Some(t) => values_of_type(doc, t),
                    None => match attribute.children().find(|n| is(*n, "simpleType")) {
                        Some(simple) => values_of_simple(doc, simple, &attribute_name),
                        None => (Vec::new(), false),
                    },
                };
                found.push(Attribute {
                    element: element.clone(),
                    name: attribute_name,
                    required: attribute.attribute("use") == Some("required"),
                    values,
                    one_of,
                });
            }
        }
    }
    found
}

// ── Rendering ─────────────────────────────────────────────────────────────────

const RED_PIXEL: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00,
    0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78,
    0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0, 0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, 0x49,
    0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];
const BLUE_PIXEL: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00,
    0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78,
    0x9c, 0x63, 0x60, 0x60, 0xf8, 0xff, 0x1f, 0x00, 0x03, 0x02, 0x01, 0xff, 0xe6, 0x77, 0x0b, 0xae, 0x00, 0x00, 0x00, 0x00, 0x49,
    0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

/// Renders with the images `logo` (red) and `other` (blue) and the font `custom` loaded on the engine.
/// A panic in the engine is an error like any other, so one bad value cannot hide the rest.
fn render(xml: &str) -> Result<Vec<u8>, String> {
    catch_unwind(AssertUnwindSafe(|| {
        let mut doc = crate::parse::parse(xml)?;
        let layouts = doc.section_layouts();
        let pages: Vec<crate::render::RenderPage> = layouts.iter().flat_map(crate::layout::layout_page).collect();
        let mut fonts = crate::pdf::FontRegistry::new();
        fonts.register("custom", crate::pdf::ATTRIBUTION_FONT.to_vec());
        let mut images = crate::pdf::ImageRegistry::new();
        images.load("logo", RED_PIXEL.to_vec());
        images.load("other", BLUE_PIXEL.to_vec());
        crate::pdf::render_pdf(&pages, &doc.fonts, &fonts, &images, &doc.meta, None, false)
    }))
    .unwrap_or_else(|_| Err("the engine panicked".to_string()))
}

// ── The documents ─────────────────────────────────────────────────────────────

/// How to exercise one element: the attributes every document of it has, the content it holds, and the
/// document around it, given the element's attributes and its content.
struct Subject {
    element: &'static str,
    /// Set in every document of this element: the required attributes, and what the others need.
    base: &'static [(&'static str, &'static str)],
    content: &'static str,
    build: fn(&str, &str) -> String,
}

impl Subject {
    fn new(element: &'static str, build: fn(&str, &str) -> String) -> Subject {
        Subject { element, base: &[], content: "", build }
    }
    fn base(mut self, base: &'static [(&'static str, &'static str)]) -> Subject {
        self.base = base;
        self
    }
    fn content(mut self, content: &'static str) -> Subject {
        self.content = content;
        self
    }
}

const IMAGES: &str = r#"<assets><image name="logo"/><image name="other"/></assets>"#;
const TWO: &str = "<text>Alpha</text><text>Beta</text>";
const FOUR: &str = "<text>A</text><text>B</text><text>C</text><text>D</text>";
const ONE: &str = "<text>Alpha</text>";
const LONG: &str = "The quick brown fox jumps over the lazy dog, and then it jumps back again, because a line of text \
    has to be long enough to wrap before alignment, width and justification can show: more words, and then even more words.";
const SLOTS: [&str; 6] = ["xs", "s", "m", "l", "xl", "xxl"];

fn lpdf(head: &str, body: &str) -> String {
    format!(r#"<lpdf version="1">{head}{body}</lpdf>"#)
}

fn flow(inner: &str) -> String {
    lpdf("", &format!("<document><section><layout>{inner}</layout></section></document>"))
}

fn flow_with_images(inner: &str) -> String {
    lpdf(IMAGES, &format!("<document><section><layout>{inner}</layout></section></document>"))
}

/// A canvas of one layer, on a page of its own.
fn draw(layer: &str, shapes: &str) -> String {
    lpdf(IMAGES, &format!("<document><section><canvas><layer {layer}>{shapes}</layer></canvas></section></document>"))
}

/// Enough text for three pages, for what depends on the page.
fn pages() -> String {
    (1..=200).map(|n| format!("<text>Line {n}</text>")).collect()
}

fn each_slot(one: impl Fn(&str) -> String) -> String {
    SLOTS.iter().map(|slot| one(slot)).collect()
}

/// The document around one of the six rows of the token scale, with every slot of the row used once.
fn token_row(row: &str, attrs: &str, used: impl Fn(&str) -> String) -> String {
    lpdf(
        &format!("<tokens><{row} {attrs}/></tokens>"),
        &format!("<document><section><layout>{}</layout></section></document>", each_slot(used)),
    )
}

/// A layout container with its content, and a text after it, so a height or a gap moves something.
fn boxed(tag: &str, attrs: &str, content: &str) -> String {
    flow(&format!("<{tag} {attrs}>{content}</{tag}><text>After</text>"))
}

fn subjects() -> Vec<Subject> {
    vec![
        // The document and its parts.
        Subject::new("document", |a, _| lpdf("", &format!("<document {a}><section><layout>{ONE}</layout></section></document>"))),
        Subject::new("section", |a, _| lpdf("", &format!("<document><section {a}><layout>{ONE}</layout></section></document>"))),
        Subject::new("meta", |a, _| lpdf("", &format!("<document><meta {a}/><section><layout>{ONE}</layout></section></document>"))),
        Subject::new("font", |a, _| {
            lpdf(&format!("<assets><font {a}/></assets>"), r#"<document><section><layout><text font="body">Alpha</text></layout></section></document>"#)
        })
        .base(&[("name", "body"), ("core", "Times-Roman")]),
        Subject::new("image", |a, _| {
            lpdf(
                &format!(r#"<assets><image {a}/><image name="other"/></assets>"#),
                r#"<document><section><layout><img name="logo" width="80pt"/></layout></section></document>"#,
            )
        })
        .base(&[("name", "logo")]),
        Subject::new("color", |a, _| {
            lpdf(
                &format!("<tokens><colors><color {a}/></colors></tokens>"),
                r#"<document><section><layout><text color="brand">Alpha</text></layout></section></document>"#,
            )
        })
        .base(&[("name", "brand"), ("value", "#336699")]),
        // The token scale: each row is used by the attribute it scales.
        Subject::new("tokens:space", |a, _| token_row("space", a, |s| format!(r#"<stack gap="{s}">{TWO}</stack>"#)))
            .base(&[("xs", "2pt"), ("s", "4pt"), ("m", "8pt"), ("l", "16pt"), ("xl", "24pt"), ("xxl", "32pt")]),
        Subject::new("tokens:border", |a, _| token_row("border", a, |s| format!(r##"<stack border="{s} #000000">{TWO}</stack>"##)))
            .base(&[("xs", "1pt"), ("s", "2pt"), ("m", "3pt"), ("l", "4pt"), ("xl", "5pt"), ("xxl", "6pt")]),
        Subject::new("tokens:radius", |a, _| {
            token_row("radius", a, |s| format!(r##"<stack background="#cccccc" radius="{s}" height="100pt">{TWO}</stack>"##))
        })
        .base(&[("xs", "1pt"), ("s", "2pt"), ("m", "4pt"), ("l", "8pt"), ("xl", "12pt"), ("xxl", "16pt")]),
        Subject::new("tokens:width", |a, _| {
            token_row("width", a, |s| format!(r##"<stack background="#cccccc" width="{s}">{TWO}</stack>"##))
        })
        .base(&[("xs", "40pt"), ("s", "80pt"), ("m", "120pt"), ("l", "160pt"), ("xl", "200pt"), ("xxl", "240pt")]),
        Subject::new("tokens:text-size", |a, _| token_row("text-size", a, |s| format!(r#"<text font-size="{s}">Alpha</text>"#)))
            .base(&[("xs", "8pt"), ("s", "10pt"), ("m", "12pt"), ("l", "16pt"), ("xl", "20pt"), ("xxl", "28pt")]),
        Subject::new("tokens:grid", |a, _| token_row("grid", a, |s| format!(r#"<grid col-width="{s}">{FOUR}</grid>"#)))
            .base(&[("xs", "40pt"), ("s", "80pt"), ("m", "120pt"), ("l", "160pt"), ("xl", "200pt"), ("xxl", "240pt")]),
        // Layout containers.
        Subject::new("stack", |a, c| boxed("stack", a, c)).content(TWO),
        Subject::new("flank", |a, c| boxed("flank", a, c)).content(TWO),
        Subject::new("split", |a, c| boxed("split", a, c)).content(TWO),
        Subject::new("cluster", |a, c| boxed("cluster", a, c)).content(TWO),
        Subject::new("grid", |a, c| boxed("grid", a, c)).base(&[("cols", "2")]).content(FOUR),
        Subject::new("frame", |a, c| boxed("frame", a, c)).content(ONE),
        Subject::new("link", |a, c| boxed("link", a, c)).base(&[("href", "https://a.example")]).content(TWO),
        // Layout leaves.
        Subject::new("divider", |a, _| flow(&format!("<text>Before</text><divider {a}/><text>After</text>"))),
        Subject::new("span", |a, _| flow(&format!("<text>Before <span {a}>middle</span> after</text>"))),
        Subject::new("img", |a, _| flow_with_images(&format!("<img {a}/><text>After</text>"))).base(&[("name", "logo")]),
        Subject::new("barcode", |a, _| flow(&format!("<barcode {a}/><text>After</text>")))
            .base(&[("type", "qr"), ("data", "1234567890128"), ("size", "100pt")]),
        Subject::new("text", |a, c| flow(&format!("<text {a}>{c}</text><text>After</text>"))).content(LONG),
        Subject::new("field", |a, _| flow(&format!("<field {a}/><text>After</text>"))).base(&[("type", "text"), ("name", "f1")]),
        // Tables.
        Subject::new("table", |a, _| flow(&format!("<table {a}>{TABLE_ROWS}</table><text>After</text>"))).base(&[("cols", "1fr 1fr")]),
        Subject::new("thead", |a, _| {
            flow(&format!(r#"<table cols="1fr 1fr"><thead {a}><td>{ONE}</td><td>{ONE}</td></thead><tr><td>{ONE}</td><td>{ONE}</td></tr></table>"#))
        }),
        Subject::new("tr", |a, _| {
            flow(&format!(
                r#"<table cols="1fr 1fr"><tr {a}><td>{ONE}</td><td>{ONE}</td></tr><tr><td>{ONE}</td><td>{ONE}</td></tr></table>"#
            ))
        }),
        Subject::new("td", |a, c| {
            flow(&format!(
                r#"<table cols="1fr 1fr"><tr><td {a}>{c}</td><td><text>one</text><text>two</text><text>three</text></td></tr></table><text>After</text>"#
            ))
        })
        .content(TWO),
        // Regions and the canvas.
        Subject::new("region", |a, _| {
            lpdf("", &format!("<document><section><layout><region {a}><text>Header</text></region>{}</layout></section></document>", pages()))
        })
        .base(&[("pin", "top")]),
        Subject::new("layer", |a, _| {
            lpdf(
                "",
                &format!(
                    r##"<document><section><canvas><layer {a}><rect x="50pt" y="50pt" w="100pt" h="60pt" fill="#ff0000"/></layer></canvas><layout>{}</layout></section></document>"##,
                    pages()
                ),
            )
        }),
        Subject::new("canvas:rect", |a, _| draw("", &format!("<rect {a}/>")))
            .base(&[("x", "50pt"), ("y", "50pt"), ("w", "100pt"), ("h", "60pt"), ("fill", "#00aa00")]),
        Subject::new("canvas:circle", |a, _| draw("", &format!("<circle {a}/>")))
            .base(&[("cx", "150pt"), ("cy", "150pt"), ("r", "40pt"), ("fill", "#00aa00")]),
        Subject::new("canvas:ellipse", |a, _| draw("", &format!("<ellipse {a}/>")))
            .base(&[("cx", "150pt"), ("cy", "150pt"), ("rx", "60pt"), ("ry", "30pt"), ("fill", "#00aa00")]),
        Subject::new("canvas:line", |a, _| draw("", &format!("<line {a}/>")))
            .base(&[("x1", "50pt"), ("y1", "50pt"), ("x2", "200pt"), ("y2", "120pt"), ("stroke", "#000000")]),
        Subject::new("canvas:path", |a, _| draw("", &format!("<path {a}/>")))
            .base(&[("d", "M 50 50 L 200 50 L 125 150 Z"), ("fill", "#00aa00")]),
        Subject::new("canvas:text", |a, c| draw("", &format!("<text {a}>{c}</text>")))
            .base(&[("x", "50pt"), ("y", "100pt")])
            .content("Canvas text"),
        Subject::new("canvas:span", |a, _| draw("", &format!(r#"<text x="50pt" y="100pt">Before <span {a}>middle</span> after</text>"#))),
        Subject::new("canvas:img", |a, _| draw("", &format!("<img {a}/>")))
            .base(&[("name", "logo"), ("x", "50pt"), ("y", "50pt"), ("w", "80pt"), ("h", "80pt")]),
    ]
}

const TABLE_ROWS: &str = "<thead><td><text>H1</text></td><td><text>H2</text></td></thead>\
    <tr><td><text>A1</text></td><td><text>A2</text></td></tr>\
    <tr><td><text>B1</text></td><td><text>B2</text></td></tr>\
    <tr><td><text>C1</text></td><td><text>C2</text></td></tr>";

// ── What an attribute needs to show ───────────────────────────────────────────

/// Adjustments for one attribute: what it needs around it to show, and what to try.
#[derive(Default)]
struct Tweak {
    /// Attributes set in every document of this attribute, besides the subject's own.
    set: Vec<(&'static str, &'static str)>,
    /// Attributes of the subject left out.
    remove: Vec<&'static str>,
    /// Attributes left out of the documents with the attribute, because they exclude it.
    without: Vec<&'static str>,
    /// Another content than the subject's.
    content: Option<&'static str>,
    /// An error from the engine is the attribute being read: a name that looks something up.
    error_counts: bool,
}

impl Tweak {
    fn set(mut self, name: &'static str, value: &'static str) -> Tweak {
        self.set.push((name, value));
        self
    }
    fn remove(mut self, name: &'static str) -> Tweak {
        self.remove.push(name);
        self
    }
    fn without(mut self, name: &'static str) -> Tweak {
        self.without.push(name);
        self
    }
    fn content(mut self, content: &'static str) -> Tweak {
        self.content = Some(content);
        self
    }
    fn error_counts(mut self) -> Tweak {
        self.error_counts = true;
        self
    }
}

const BOXES: [&str; 7] = ["stack", "flank", "split", "cluster", "grid", "frame", "td"];

fn tweaks() -> HashMap<String, Tweak> {
    let mut t: HashMap<String, Tweak> = HashMap::new();
    let mut add = |id: &str, tweak: Tweak| {
        t.insert(id.to_string(), tweak);
    };

    // A radius or a width shows on a box that is drawn.
    for element in BOXES.iter().chain(["img"].iter()) {
        add(&format!("{element}@radius"), Tweak::default().set("background", "#cccccc"));
    }
    for element in ["stack", "flank", "split", "cluster", "grid", "frame", "table"] {
        add(&format!("{element}@width"), Tweak::default().set("background", "#cccccc"));
    }

    // Alignment across the row or column needs children that differ in size.
    const NARROW: &str = "<text width=\"90pt\">Alpha</text><text width=\"90pt\">Beta</text>";
    const UNEVEN: &str = "<stack background=\"#cccccc\"><text font-size=\"24pt\">Alpha</text></stack>\
        <stack background=\"#cccccc\"><text font-size=\"8pt\">Beta, a little longer</text></stack>";
    // A stack's children shrink to their text and sit at the start, the middle or the end of it; the
    // texts here are aligned differently inside, so no two ways of placing them look alike.
    add(
        "stack@align",
        Tweak::default()
            .content("<text align=\"left\">Alpha</text><text align=\"center\">Beta</text><text align=\"right\">Gamma</text>"),
    );
    // A split puts its children at the two edges, so a gap shows once they share the width.
    add("split@gap", Tweak::default().set("equal", "true"));
    add("flank@align", Tweak::default().content(UNEVEN));
    add("split@align", Tweak::default().content(UNEVEN));
    add("cluster@align", Tweak::default().content(UNEVEN));
    add("td@align", Tweak::default().content("<text width=\"40pt\">Alpha</text>"));
    add("stack@justify", Tweak::default().set("height", "300pt"));
    add("cluster@justify", Tweak::default().content(NARROW));

    // A grid has either cols or col-width.
    add("grid@col-width", Tweak::default().remove("cols"));

    // Names that something is looked up by: a name nothing matches is an error, and the attribute was read.
    add("font@name", Tweak::default().error_counts());
    add("image@name", Tweak::default().error_counts());
    add("color@name", Tweak::default().error_counts());
    // A font is a core font or a registry key, not both.
    add("font@ref", Tweak::default().without("core"));

    // Barcodes: the options of one type show on that type.
    add("barcode@hrt", Tweak::default().set("type", "code128"));
    add("barcode@width", Tweak::default().set("type", "code128"));
    add("barcode@height", Tweak::default().set("type", "code128"));

    // Form fields: an option of one type shows on that type.
    add("field@options", Tweak::default().set("type", "dropdown"));
    add("field@group", Tweak::default().set("type", "radio").set("value", "a"));
    add("field@checked", Tweak::default().set("type", "checkbox"));
    add("field@action-url", Tweak::default().set("type", "button"));
    add("field@label", Tweak::default().set("type", "button"));

    // Canvas shapes: what is drawn only with a fill or a stroke.
    for element in ["canvas:rect", "canvas:circle", "canvas:ellipse"] {
        add(&format!("{element}@stroke-width"), Tweak::default().set("stroke", "#000000"));
        add(&format!("{element}@stroke-dash"), Tweak::default().set("stroke", "#000000"));
    }
    add("canvas:path@stroke-width", Tweak::default().set("stroke", "#000000"));
    add("canvas:path@stroke-dash", Tweak::default().set("stroke", "#000000"));
    add("canvas:path@line-cap", Tweak::default().set("stroke", "#000000"));
    // The width of a canvas text is what its align works in.
    add("canvas:text@w", Tweak::default().set("align", "center"));
    add(
        "canvas:path@fill-rule",
        Tweak::default().set("d", "M 50 50 L 250 50 L 250 250 L 50 250 Z M 100 100 L 200 100 L 200 200 L 100 200 Z"),
    );
    t
}

// ── Running it ────────────────────────────────────────────────────────────────

fn attribute_string(pairs: &[(String, String)]) -> String {
    pairs.iter().map(|(n, v)| format!("{n}=\"{v}\"")).collect::<Vec<_>>().join(" ")
}

enum Verdict {
    /// Some value changed the PDF. `same` are the values that did not: one may be the default, which
    /// `allowed` counts. Empty unless the type is an enumeration.
    Effect { same: Vec<String>, allowed: usize },
    /// No value changed the PDF; these are the values that rendered the same.
    NoEffect { tried: Vec<String> },
    /// Every value that did not change the PDF failed to render, so nothing is known.
    Inconclusive(String),
    /// The document without the attribute does not render, or there is nothing to try: the test needs fixing.
    Broken(String),
}

fn judge(attribute: &Attribute, subject: &Subject, tweak: &Tweak) -> Verdict {
    let id = attribute.id();
    let mut base: Vec<(String, String)> = subject
        .base
        .iter()
        .filter(|pair| !tweak.remove.contains(&pair.0))
        .map(|pair| (pair.0.to_string(), pair.1.to_string()))
        .collect();
    for &(n, v) in &tweak.set {
        match base.iter_mut().find(|pair| pair.0 == n) {
            Some(existing) => existing.1 = v.to_string(),
            None => base.push((n.to_string(), v.to_string())),
        }
    }
    let content = tweak.content.unwrap_or(subject.content);
    let base_value = base.iter().find(|pair| pair.0 == attribute.name).map(|pair| pair.1.clone());
    if attribute.required && base_value.is_none() {
        return Verdict::Broken(format!("it is required: give the subject {} a value for it", subject.element));
    }

    let baseline_xml = (subject.build)(&attribute_string(&base), content);
    let baseline = match render(&baseline_xml) {
        Ok(bytes) => bytes,
        Err(e) => return Verdict::Broken(format!("the document without it does not render: {e}\n    {baseline_xml}")),
    };

    let values: Vec<String> = if !attribute.values.is_empty() {
        attribute.values.clone()
    } else if let Some(samples) = strings(&id, &attribute.name) {
        strs(samples)
    } else {
        return Verdict::Broken("no sample values: add them to `strings`".to_string());
    };

    let mut changed = false;
    let mut same = Vec::new();
    let mut errors = Vec::new();
    for value in values.iter().filter(|v| Some(*v) != base_value.as_ref()) {
        let mut pairs: Vec<(String, String)> =
            base.iter().filter(|pair| !tweak.without.iter().any(|w| *w == pair.0)).cloned().collect();
        match pairs.iter_mut().find(|pair| pair.0 == attribute.name) {
            Some(existing) => existing.1 = value.clone(),
            None => pairs.push((attribute.name.clone(), value.clone())),
        }
        match render(&(subject.build)(&attribute_string(&pairs), content)) {
            Ok(bytes) if bytes != baseline => changed = true,
            Ok(_) => same.push(value.clone()),
            Err(_) if tweak.error_counts => changed = true,
            Err(e) => errors.push(format!("{value}: {e}")),
        }
    }

    if !changed {
        return if same.is_empty() && !errors.is_empty() {
            Verdict::Inconclusive(errors.join("; "))
        } else {
            Verdict::NoEffect { tried: same }
        };
    }
    // Without the attribute the default applies, and one value may be the same as it. A value of a required
    // attribute is compared with the one the document has, so none may be the same.
    let allowed = if base_value.is_some() { 0 } else { 1 };
    Verdict::Effect { same: if attribute.one_of { same } else { Vec::new() }, allowed }
}

fn reason<'a>(list: &'a [(&'a str, &'a str)], id: &str) -> Option<&'a str> {
    list.iter().find(|(key, _)| *key == id).map(|(_, why)| *why)
}

#[test]
fn every_attribute_of_the_schema_does_something() {
    let text = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schema/lpdf.xsd"))
        .expect("schema/lpdf.xsd");
    let doc = Document::parse(&text).expect("the schema is XML");
    let attributes = with_contexts(schema_attributes(&doc));
    let subjects = subjects();
    let tweaks = tweaks();

    let ids: BTreeSet<String> = attributes.iter().map(Attribute::id).collect();
    let elements: BTreeSet<&str> = attributes.iter().map(|a| a.element.as_str()).collect();
    let mut problems: Vec<String> = Vec::new();

    // Everything named here has to be in the schema.
    for (id, _) in IGNORED.iter().chain(INVISIBLE.iter()).chain(UNTESTED.iter()) {
        if !ids.contains(*id) {
            problems.push(format!("{id} is listed here and is not in the schema"));
        }
    }
    for (id, _) in IGNORED_VALUES {
        let attribute = id.split('=').next().unwrap();
        if !ids.contains(attribute) {
            problems.push(format!("{id} is listed here and is not in the schema"));
        }
    }
    for id in tweaks.keys() {
        if !ids.contains(id) {
            problems.push(format!("a tweak for {id}, which is not in the schema"));
        }
    }
    for subject in &subjects {
        if !elements.contains(subject.element) {
            problems.push(format!("a subject for {}, which is not in the schema", subject.element));
        }
    }

    let (mut tested, mut ignored, mut invisible, mut skipped) = (0, 0, 0, 0);
    for attribute in &attributes {
        let id = attribute.id();
        if attribute.name.starts_with("data-") || reason(UNTESTED, &id).is_some() {
            skipped += 1;
            continue;
        }
        let Some(subject) = subjects.iter().find(|s| s.element == attribute.element) else {
            problems.push(format!("{id}: no subject for the element {}: add one to `subjects`", attribute.element));
            continue;
        };
        let default = Tweak::default();
        let tweak = tweaks.get(&id).unwrap_or(&default);
        let verdict = judge(attribute, subject, tweak);
        tested += 1;

        match (&verdict, reason(IGNORED, &id), reason(INVISIBLE, &id)) {
            (Verdict::Effect { .. }, Some(why), _) => {
                problems.push(format!("{id} has an effect now; it is listed as ignored ({why}). Remove it from IGNORED"));
            }
            (Verdict::Effect { .. }, None, Some(_)) => invisible += 1,
            (Verdict::NoEffect { .. }, Some(_), _) => ignored += 1,
            (Verdict::NoEffect { .. }, None, Some(why)) => {
                problems.push(format!("{id} changes nothing now; it is listed as invisible ({why}). Move it to IGNORED"));
            }
            (Verdict::NoEffect { tried }, None, None) => {
                problems.push(format!("{id}: no value changes the PDF (tried {})", tried.join(", ")));
            }
            (Verdict::Inconclusive(why), ..) => problems.push(format!("{id}: inconclusive, every value failed to render: {why}")),
            (Verdict::Broken(why), ..) => problems.push(format!("{id}: {why}")),
            (Verdict::Effect { same, allowed }, None, None) => {
                let unexplained: Vec<String> =
                    same.iter().filter(|v| reason(IGNORED_VALUES, &format!("{id}={v}")).is_none()).cloned().collect();
                if unexplained.len() > *allowed {
                    for value in unexplained {
                        problems.push(format!("{id}={value}: this value changes nothing"));
                    }
                }
            }
        }
    }

    // A value listed as ignored has to be one that still changes nothing.
    for (value_id, why) in IGNORED_VALUES {
        let Some((id, value)) = value_id.split_once('=') else {
            problems.push(format!("{value_id} should read element@attribute=value"));
            continue;
        };
        // An attribute or an element that is not there has been reported above.
        let Some(attribute) = attributes.iter().find(|a| a.id() == id) else { continue };
        let Some(subject) = subjects.iter().find(|s| s.element == attribute.element) else { continue };
        let default = Tweak::default();
        let tweak = tweaks.get(id).unwrap_or(&default);
        if let Verdict::Effect { same, .. } = judge(attribute, subject, tweak)
            && !same.iter().any(|v| v == value)
        {
            problems
                .push(format!("{value_id} has an effect now; it is listed as ignored ({why}). Remove it from IGNORED_VALUES"));
        }
    }

    println!(
        "{tested} attributes tried: {ignored} known to be ignored, {invisible} change the file and not the page; {skipped} not tried"
    );
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}
