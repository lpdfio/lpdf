// Generated from lpdf.xsd by scripts/gen-sdk-api.mjs.
// Do not edit: change the schema and run `make gen-sdk-api`.

//! The attributes of every element of lpdf.xsd and their types.

use crate::attr_types::{Bound, Kind};

/// The attributes of an element, in the order the schema lists them, without the data-* attributes.
pub(crate) type Attributes = &'static [(&'static str, Kind)];

const RECT: Attributes = &[
    ("x", Kind::Text),
    ("y", Kind::Text),
    ("width", Kind::Text),
    ("height", Kind::Text),
    ("anchor", Kind::Text),
    ("radius", Kind::Text),
    ("fill", Kind::Text),
    ("stroke", Kind::Text),
    ("stroke-width", Kind::Text),
    ("stroke-dash", Kind::Text),
    ("shadow", Kind::Text),
    ("ocg", Kind::Text),
];

const CIRCLE: Attributes = &[
    ("x", Kind::Text),
    ("y", Kind::Text),
    ("r", Kind::Text),
    ("anchor", Kind::Text),
    ("fill", Kind::Text),
    ("stroke", Kind::Text),
    ("stroke-width", Kind::Text),
    ("stroke-dash", Kind::Text),
    ("shadow", Kind::Text),
    ("ocg", Kind::Text),
];

const ELLIPSE: Attributes = &[
    ("x", Kind::Text),
    ("y", Kind::Text),
    ("rx", Kind::Text),
    ("ry", Kind::Text),
    ("anchor", Kind::Text),
    ("fill", Kind::Text),
    ("stroke", Kind::Text),
    ("stroke-width", Kind::Text),
    ("stroke-dash", Kind::Text),
    ("shadow", Kind::Text),
    ("ocg", Kind::Text),
];

const LINE: Attributes = &[
    ("x1", Kind::Text),
    ("y1", Kind::Text),
    ("x2", Kind::Text),
    ("y2", Kind::Text),
    ("stroke", Kind::Text),
    ("stroke-width", Kind::Text),
    ("stroke-dash", Kind::Text),
    ("line-cap", Kind::Text),
    ("ocg", Kind::Text),
];

const PATH: Attributes = &[
    ("d", Kind::Text),
    ("fill", Kind::Text),
    ("stroke", Kind::Text),
    ("fill-rule", Kind::Text),
    ("stroke-width", Kind::Text),
    ("stroke-dash", Kind::Text),
    ("line-cap", Kind::Text),
    ("ocg", Kind::Text),
];

const TEXT_AT: Attributes = &[
    ("x", Kind::Text),
    ("y", Kind::Text),
    ("anchor", Kind::Text),
    ("font-size", Kind::Text),
    ("font", Kind::Text),
    ("bold", Kind::Boolean),
    ("color", Kind::Text),
    ("align", Kind::Text),
    ("line-height", Kind::NameOrDecimal { min: Bound::Exclusive(0.0), max: Bound::Open }),
    ("max-lines", Kind::Integer { min: Some(1), max: None }),
    ("ellipsis", Kind::Boolean),
    ("width", Kind::Text),
    ("ocg", Kind::Text),
];

const IMG_AT: Attributes = &[
    ("name", Kind::Text),
    ("x", Kind::Text),
    ("y", Kind::Text),
    ("anchor", Kind::Text),
    ("width", Kind::Text),
    ("height", Kind::Text),
    ("ocg", Kind::Text),
];

const BARCODE_AT: Attributes = &[
    ("type", Kind::Text),
    ("data", Kind::Text),
    ("height", Kind::Text),
    ("ec", Kind::Text),
    ("hrt", Kind::Boolean),
    ("color", Kind::Text),
    ("background", Kind::Text),
    ("x", Kind::Text),
    ("y", Kind::Text),
    ("anchor", Kind::Text),
    ("width", Kind::Text),
    ("ocg", Kind::Text),
];

const LAYER: Attributes = &[
    ("page", Kind::Text),
    ("opacity", Kind::Decimal { min: Bound::Inclusive(0.0), max: Bound::Inclusive(1.0) }),
    ("transform", Kind::Text),
    ("clip", Kind::Text),
    ("font", Kind::Text),
    ("font-size", Kind::Text),
    ("color", Kind::Text),
    ("alt", Kind::Text),
    ("ocg", Kind::Text),
];

const FONT: Attributes = &[
    ("name", Kind::Text),
    ("core", Kind::Text),
    ("ref", Kind::Text),
    ("src", Kind::Text),
];

const IMAGE: Attributes = &[
    ("name", Kind::Text),
    ("ref", Kind::Text),
    ("src", Kind::Text),
];

const PROFILE: Attributes = &[
    ("name", Kind::Text),
    ("ref", Kind::Text),
    ("src", Kind::Text),
];

const COLOR: Attributes = &[
    ("name", Kind::Text),
    ("value", Kind::Text),
];

const ALIAS: Attributes = &[
    ("name", Kind::Text),
    ("value", Kind::Text),
];

const STOP: Attributes = &[
    ("offset", Kind::Text),
    ("color", Kind::Text),
];

const GRADIENT: Attributes = &[
    ("name", Kind::Text),
    ("type", Kind::Text),
    ("from", Kind::Text),
    ("to", Kind::Text),
    ("angle", Kind::Decimal { min: Bound::Open, max: Bound::Open }),
];

const TOKENS: Attributes = &[
];

const TOKENS_SPACE: Attributes = &[
    ("xs", Kind::Text),
    ("s", Kind::Text),
    ("m", Kind::Text),
    ("l", Kind::Text),
    ("xl", Kind::Text),
    ("xxl", Kind::Text),
];

const TOKENS_BORDER: Attributes = &[
    ("xs", Kind::Text),
    ("s", Kind::Text),
    ("m", Kind::Text),
    ("l", Kind::Text),
    ("xl", Kind::Text),
    ("xxl", Kind::Text),
];

const TOKENS_RADIUS: Attributes = &[
    ("xs", Kind::Text),
    ("s", Kind::Text),
    ("m", Kind::Text),
    ("l", Kind::Text),
    ("xl", Kind::Text),
    ("xxl", Kind::Text),
];

const TOKENS_WIDTH: Attributes = &[
    ("xs", Kind::Text),
    ("s", Kind::Text),
    ("m", Kind::Text),
    ("l", Kind::Text),
    ("xl", Kind::Text),
    ("xxl", Kind::Text),
];

const TOKENS_HEIGHT: Attributes = &[
    ("xs", Kind::Text),
    ("s", Kind::Text),
    ("m", Kind::Text),
    ("l", Kind::Text),
    ("xl", Kind::Text),
    ("xxl", Kind::Text),
];

const TOKENS_TEXT_SIZE: Attributes = &[
    ("xs", Kind::Text),
    ("s", Kind::Text),
    ("m", Kind::Text),
    ("l", Kind::Text),
    ("xl", Kind::Text),
    ("xxl", Kind::Text),
];

const TOKENS_LINE_HEIGHT: Attributes = &[
    ("xs", Kind::Decimal { min: Bound::Exclusive(0.0), max: Bound::Open }),
    ("s", Kind::Decimal { min: Bound::Exclusive(0.0), max: Bound::Open }),
    ("m", Kind::Decimal { min: Bound::Exclusive(0.0), max: Bound::Open }),
    ("l", Kind::Decimal { min: Bound::Exclusive(0.0), max: Bound::Open }),
    ("xl", Kind::Decimal { min: Bound::Exclusive(0.0), max: Bound::Open }),
    ("xxl", Kind::Decimal { min: Bound::Exclusive(0.0), max: Bound::Open }),
];

const TOKENS_GRID: Attributes = &[
    ("xs", Kind::Text),
    ("s", Kind::Text),
    ("m", Kind::Text),
    ("l", Kind::Text),
    ("xl", Kind::Text),
    ("xxl", Kind::Text),
];

const TOKENS_ALIASES: Attributes = &[
];

const TOKENS_COLORS: Attributes = &[
];

const TOKENS_GRADIENTS: Attributes = &[
];

const ASSETS: Attributes = &[
];

const META: Attributes = &[
    ("title", Kind::Text),
    ("author", Kind::Text),
    ("subject", Kind::Text),
    ("keywords", Kind::Text),
    ("creator", Kind::Text),
    ("lang", Kind::Text),
];

const OUTPUT: Attributes = &[
    ("encrypt", Kind::Text),
    ("allow", Kind::Text),
    ("pdfa", Kind::Text),
    ("pdfua", Kind::Text),
    ("pdfx", Kind::Text),
    ("profile", Kind::Text),
    ("linearize", Kind::Boolean),
];

const ATTACHMENTS: Attributes = &[
];

const FILE: Attributes = &[
    ("name", Kind::Text),
    ("src", Kind::Text),
    ("ref", Kind::Text),
    ("mime", Kind::Text),
    ("relationship", Kind::Text),
    ("description", Kind::Text),
];

const LAYOUT: Attributes = &[
    ("margin", Kind::Text),
];

const LAYOUT_REGION: Attributes = &[
    ("pin", Kind::Text),
    ("page", Kind::Text),
    ("w", Kind::Text),
    ("debug", Kind::Boolean),
];

const MARKDOWN: Attributes = &[
    ("margin", Kind::Text),
    ("font", Kind::Text),
    ("font-size", Kind::Text),
    ("color", Kind::Text),
    ("gap", Kind::Text),
    ("bookmarks", Kind::Integer { min: Some(0), max: Some(6) }),
    ("debug", Kind::Boolean),
];

const MARKDOWN_MARKDOWN_STYLES: Attributes = &[
    ("src", Kind::Text),
];

const MARKDOWN_STYLES_H1: Attributes = &[
    ("font", Kind::Text),
    ("font-size", Kind::Text),
    ("bold", Kind::Boolean),
    ("color", Kind::Text),
    ("align", Kind::Text),
    ("line-height", Kind::NameOrDecimal { min: Bound::Exclusive(0.0), max: Bound::Open }),
    ("padding", Kind::Text),
    ("background", Kind::Text),
    ("border", Kind::Text),
    ("radius", Kind::Text),
];

const MARKDOWN_STYLES_H2: Attributes = &[
    ("font", Kind::Text),
    ("font-size", Kind::Text),
    ("bold", Kind::Boolean),
    ("color", Kind::Text),
    ("align", Kind::Text),
    ("line-height", Kind::NameOrDecimal { min: Bound::Exclusive(0.0), max: Bound::Open }),
    ("padding", Kind::Text),
    ("background", Kind::Text),
    ("border", Kind::Text),
    ("radius", Kind::Text),
];

const MARKDOWN_STYLES_H3: Attributes = &[
    ("font", Kind::Text),
    ("font-size", Kind::Text),
    ("bold", Kind::Boolean),
    ("color", Kind::Text),
    ("align", Kind::Text),
    ("line-height", Kind::NameOrDecimal { min: Bound::Exclusive(0.0), max: Bound::Open }),
    ("padding", Kind::Text),
    ("background", Kind::Text),
    ("border", Kind::Text),
    ("radius", Kind::Text),
];

const MARKDOWN_STYLES_H4: Attributes = &[
    ("font", Kind::Text),
    ("font-size", Kind::Text),
    ("bold", Kind::Boolean),
    ("color", Kind::Text),
    ("align", Kind::Text),
    ("line-height", Kind::NameOrDecimal { min: Bound::Exclusive(0.0), max: Bound::Open }),
    ("padding", Kind::Text),
    ("background", Kind::Text),
    ("border", Kind::Text),
    ("radius", Kind::Text),
];

const MARKDOWN_STYLES_H5: Attributes = &[
    ("font", Kind::Text),
    ("font-size", Kind::Text),
    ("bold", Kind::Boolean),
    ("color", Kind::Text),
    ("align", Kind::Text),
    ("line-height", Kind::NameOrDecimal { min: Bound::Exclusive(0.0), max: Bound::Open }),
    ("padding", Kind::Text),
    ("background", Kind::Text),
    ("border", Kind::Text),
    ("radius", Kind::Text),
];

const MARKDOWN_STYLES_H6: Attributes = &[
    ("font", Kind::Text),
    ("font-size", Kind::Text),
    ("bold", Kind::Boolean),
    ("color", Kind::Text),
    ("align", Kind::Text),
    ("line-height", Kind::NameOrDecimal { min: Bound::Exclusive(0.0), max: Bound::Open }),
    ("padding", Kind::Text),
    ("background", Kind::Text),
    ("border", Kind::Text),
    ("radius", Kind::Text),
];

const MARKDOWN_STYLES_P: Attributes = &[
    ("font", Kind::Text),
    ("font-size", Kind::Text),
    ("bold", Kind::Boolean),
    ("color", Kind::Text),
    ("align", Kind::Text),
    ("line-height", Kind::NameOrDecimal { min: Bound::Exclusive(0.0), max: Bound::Open }),
    ("padding", Kind::Text),
    ("background", Kind::Text),
    ("border", Kind::Text),
    ("radius", Kind::Text),
];

const MARKDOWN_STYLES_CODE: Attributes = &[
    ("font", Kind::Text),
    ("font-size", Kind::Text),
    ("color", Kind::Text),
    ("line-height", Kind::NameOrDecimal { min: Bound::Exclusive(0.0), max: Bound::Open }),
    ("padding", Kind::Text),
    ("background", Kind::Text),
    ("border", Kind::Text),
    ("radius", Kind::Text),
    ("tab-size", Kind::Integer { min: Some(1), max: None }),
];

const MARKDOWN_STYLES_QUOTE: Attributes = &[
    ("font", Kind::Text),
    ("font-size", Kind::Text),
    ("bold", Kind::Boolean),
    ("color", Kind::Text),
    ("align", Kind::Text),
    ("line-height", Kind::NameOrDecimal { min: Bound::Exclusive(0.0), max: Bound::Open }),
    ("padding", Kind::Text),
    ("background", Kind::Text),
    ("border", Kind::Text),
    ("radius", Kind::Text),
    ("bar-color", Kind::Text),
    ("bar-width", Kind::Text),
];

const MARKDOWN_STYLES_LI: Attributes = &[
    ("font", Kind::Text),
    ("font-size", Kind::Text),
    ("color", Kind::Text),
    ("gap", Kind::Text),
    ("padding", Kind::Text),
    ("background", Kind::Text),
    ("border", Kind::Text),
    ("indent", Kind::Text),
    ("bullet", Kind::Text),
];

const MARKDOWN_STYLES_HR: Attributes = &[
    ("color", Kind::Text),
    ("thickness", Kind::Text),
];

const MARKDOWN_STYLES_A: Attributes = &[
    ("font", Kind::Text),
    ("color", Kind::Text),
    ("bold", Kind::Boolean),
    ("underline", Kind::Boolean),
];

const MARKDOWN_STYLES_STRONG: Attributes = &[
    ("font", Kind::Text),
    ("color", Kind::Text),
    ("bold", Kind::Boolean),
];

const MARKDOWN_STYLES_EM: Attributes = &[
    ("font", Kind::Text),
    ("color", Kind::Text),
];

const MARKDOWN_STYLES_DEL: Attributes = &[
    ("font", Kind::Text),
    ("color", Kind::Text),
    ("strike", Kind::Boolean),
];

const MARKDOWN_STYLES_TABLE: Attributes = &[
    ("border", Kind::Text),
    ("stripe", Kind::Text),
    ("head-border", Kind::Text),
    ("row-border", Kind::Text),
    ("gap", Kind::Text),
    ("padding", Kind::Text),
    ("background", Kind::Text),
];

const MARKDOWN_STYLES_TH: Attributes = &[
    ("font", Kind::Text),
    ("font-size", Kind::Text),
    ("bold", Kind::Boolean),
    ("color", Kind::Text),
    ("align", Kind::Text),
    ("padding", Kind::Text),
    ("background", Kind::Text),
    ("border", Kind::Text),
];

const MARKDOWN_STYLES_TD: Attributes = &[
    ("font", Kind::Text),
    ("font-size", Kind::Text),
    ("bold", Kind::Boolean),
    ("color", Kind::Text),
    ("align", Kind::Text),
    ("padding", Kind::Text),
    ("background", Kind::Text),
    ("border", Kind::Text),
];

const MARKDOWN_MARKDOWN_CONTENT: Attributes = &[
];

const DOCUMENT: Attributes = &[
    ("font", Kind::Text),
    ("size", Kind::Text),
    ("orientation", Kind::Text),
    ("bleed", Kind::Text),
    ("background", Kind::Text),
    ("debug", Kind::Boolean),
    ("footnote-counter", Kind::Text),
];

const SECTION: Attributes = &[
    ("size", Kind::Text),
    ("orientation", Kind::Text),
    ("bleed", Kind::Text),
    ("background", Kind::Text),
    ("title", Kind::Text),
    ("debug", Kind::Boolean),
    ("type", Kind::Text),
    ("footnote-counter", Kind::Text),
    ("toc-indent-step", Kind::Text),
    ("page-start", Kind::Integer { min: Some(1), max: Some(2147483647) }),
    ("page-format", Kind::Text),
    ("page-prefix", Kind::Text),
];

const LPDF: Attributes = &[
    ("version", Kind::Integer { min: Some(1), max: None }),
];

const COLUMN: Attributes = &[
    ("font", Kind::Text),
    ("font-size", Kind::Text),
    ("color", Kind::Text),
    ("gap", Kind::Text),
    ("padding", Kind::Text),
    ("height", Kind::Text),
    ("background", Kind::Text),
    ("border", Kind::Text),
    ("radius", Kind::Text),
    ("opacity", Kind::Decimal { min: Bound::Inclusive(0.0), max: Bound::Inclusive(1.0) }),
    ("shadow", Kind::Text),
    ("paginate", Kind::Text),
    ("debug", Kind::Boolean),
    ("ocg", Kind::Text),
    ("align", Kind::Text),
    ("justify", Kind::Text),
    ("width", Kind::Text),
];

const ROW: Attributes = &[
    ("font", Kind::Text),
    ("font-size", Kind::Text),
    ("color", Kind::Text),
    ("gap", Kind::Text),
    ("padding", Kind::Text),
    ("height", Kind::Text),
    ("background", Kind::Text),
    ("border", Kind::Text),
    ("radius", Kind::Text),
    ("opacity", Kind::Decimal { min: Bound::Inclusive(0.0), max: Bound::Inclusive(1.0) }),
    ("shadow", Kind::Text),
    ("paginate", Kind::Text),
    ("debug", Kind::Boolean),
    ("ocg", Kind::Text),
    ("align", Kind::Text),
    ("justify", Kind::Text),
    ("width", Kind::Text),
];

const LIST: Attributes = &[
    ("font", Kind::Text),
    ("font-size", Kind::Text),
    ("color", Kind::Text),
    ("gap", Kind::Text),
    ("padding", Kind::Text),
    ("height", Kind::Text),
    ("background", Kind::Text),
    ("border", Kind::Text),
    ("radius", Kind::Text),
    ("opacity", Kind::Decimal { min: Bound::Inclusive(0.0), max: Bound::Inclusive(1.0) }),
    ("shadow", Kind::Text),
    ("paginate", Kind::Text),
    ("debug", Kind::Boolean),
    ("ocg", Kind::Text),
    ("type", Kind::Text),
    ("format", Kind::Text),
    ("start", Kind::Integer { min: None, max: None }),
    ("prefix", Kind::Text),
    ("suffix", Kind::Text),
    ("bullet", Kind::Text),
    ("indent", Kind::Text),
    ("width", Kind::Text),
];

const LI: Attributes = &[
    ("font", Kind::Text),
    ("font-size", Kind::Text),
    ("color", Kind::Text),
    ("gap", Kind::Text),
    ("padding", Kind::Text),
    ("height", Kind::Text),
    ("background", Kind::Text),
    ("border", Kind::Text),
    ("radius", Kind::Text),
    ("opacity", Kind::Decimal { min: Bound::Inclusive(0.0), max: Bound::Inclusive(1.0) }),
    ("shadow", Kind::Text),
    ("paginate", Kind::Text),
    ("debug", Kind::Boolean),
    ("ocg", Kind::Text),
    ("checked", Kind::Boolean),
];

const GRID: Attributes = &[
    ("font", Kind::Text),
    ("font-size", Kind::Text),
    ("color", Kind::Text),
    ("gap", Kind::Text),
    ("padding", Kind::Text),
    ("height", Kind::Text),
    ("background", Kind::Text),
    ("border", Kind::Text),
    ("radius", Kind::Text),
    ("opacity", Kind::Decimal { min: Bound::Inclusive(0.0), max: Bound::Inclusive(1.0) }),
    ("shadow", Kind::Text),
    ("paginate", Kind::Text),
    ("debug", Kind::Boolean),
    ("ocg", Kind::Text),
    ("width", Kind::Text),
    ("col-width", Kind::Text),
    ("cols", Kind::Integer { min: Some(1), max: Some(12) }),
];

const FRAME: Attributes = &[
    ("font", Kind::Text),
    ("font-size", Kind::Text),
    ("color", Kind::Text),
    ("padding", Kind::Text),
    ("height", Kind::Text),
    ("background", Kind::Text),
    ("border", Kind::Text),
    ("radius", Kind::Text),
    ("opacity", Kind::Decimal { min: Bound::Inclusive(0.0), max: Bound::Inclusive(1.0) }),
    ("shadow", Kind::Text),
    ("paginate", Kind::Text),
    ("debug", Kind::Boolean),
    ("ocg", Kind::Text),
    ("width", Kind::Text),
];

const CODE: Attributes = &[
    ("font", Kind::Text),
    ("font-size", Kind::Text),
    ("color", Kind::Text),
    ("padding", Kind::Text),
    ("height", Kind::Text),
    ("background", Kind::Text),
    ("border", Kind::Text),
    ("radius", Kind::Text),
    ("opacity", Kind::Decimal { min: Bound::Inclusive(0.0), max: Bound::Inclusive(1.0) }),
    ("shadow", Kind::Text),
    ("paginate", Kind::Text),
    ("debug", Kind::Boolean),
    ("ocg", Kind::Text),
    ("width", Kind::Text),
    ("tab-size", Kind::Integer { min: Some(1), max: Some(16) }),
    ("line-height", Kind::NameOrDecimal { min: Bound::Exclusive(0.0), max: Bound::Open }),
    ("widows", Kind::Integer { min: Some(1), max: Some(10) }),
    ("orphans", Kind::Integer { min: Some(1), max: Some(10) }),
];

const LINK: Attributes = &[
    ("href", Kind::Text),
    ("gap", Kind::Text),
    ("width", Kind::Text),
    ("height", Kind::Text),
    ("debug", Kind::Boolean),
    ("ocg", Kind::Text),
];

const BOOKMARK: Attributes = &[
    ("label", Kind::Text),
    ("level", Kind::Integer { min: Some(1), max: Some(6) }),
    ("id", Kind::Text),
    ("ref-only", Kind::Boolean),
];

const DIVIDER: Attributes = &[
    ("direction", Kind::Text),
    ("color", Kind::Text),
    ("thickness", Kind::Text),
    ("width", Kind::Text),
    ("debug", Kind::Boolean),
    ("ocg", Kind::Text),
];

const SPAN: Attributes = &[
    ("font", Kind::Text),
    ("bold", Kind::Boolean),
    ("color", Kind::Text),
    ("href", Kind::Text),
    ("bookmark-ref", Kind::Text),
    ("underline", Kind::Boolean),
    ("strike", Kind::Boolean),
];

const CODE_SPAN: Attributes = &[
    ("font", Kind::Text),
    ("color", Kind::Text),
    ("background", Kind::Text),
];

const FN: Attributes = &[
];

const IMG: Attributes = &[
    ("name", Kind::Text),
    ("alt", Kind::Text),
    ("height", Kind::Text),
    ("width", Kind::Text),
    ("font", Kind::Text),
    ("font-size", Kind::Text),
    ("gap", Kind::Text),
    ("padding", Kind::Text),
    ("background", Kind::Text),
    ("border", Kind::Text),
    ("radius", Kind::Text),
    ("paginate", Kind::Text),
    ("debug", Kind::Boolean),
    ("ocg", Kind::Text),
];

const BARCODE: Attributes = &[
    ("type", Kind::Text),
    ("data", Kind::Text),
    ("height", Kind::Text),
    ("ec", Kind::Text),
    ("hrt", Kind::Boolean),
    ("color", Kind::Text),
    ("background", Kind::Text),
    ("width", Kind::Text),
    ("alt", Kind::Text),
    ("paginate", Kind::Text),
    ("debug", Kind::Boolean),
    ("ocg", Kind::Text),
];

const SHAPE: Attributes = &[
    ("type", Kind::Text),
    ("width", Kind::Text),
    ("height", Kind::Text),
    ("r", Kind::Text),
    ("radius", Kind::Text),
    ("fill", Kind::Text),
    ("stroke", Kind::Text),
    ("stroke-width", Kind::Text),
    ("stroke-dash", Kind::Text),
    ("opacity", Kind::Decimal { min: Bound::Inclusive(0.0), max: Bound::Inclusive(1.0) }),
    ("debug", Kind::Boolean),
    ("ocg", Kind::Text),
];

const TEXT: Attributes = &[
    ("font-size", Kind::Text),
    ("font", Kind::Text),
    ("bold", Kind::Boolean),
    ("color", Kind::Text),
    ("align", Kind::Text),
    ("line-height", Kind::NameOrDecimal { min: Bound::Exclusive(0.0), max: Bound::Open }),
    ("max-lines", Kind::Integer { min: Some(1), max: None }),
    ("ellipsis", Kind::Boolean),
    ("width", Kind::Text),
    ("role", Kind::Text),
    ("widows", Kind::Integer { min: Some(1), max: Some(10) }),
    ("orphans", Kind::Integer { min: Some(1), max: Some(10) }),
    ("capture", Kind::Text),
    ("capture-ref", Kind::Text),
    ("capture-mode", Kind::Text),
    ("capture-fallback", Kind::Text),
    ("paginate", Kind::Text),
    ("debug", Kind::Boolean),
    ("ocg", Kind::Text),
];

const TD: Attributes = &[
    ("font", Kind::Text),
    ("font-size", Kind::Text),
    ("color", Kind::Text),
    ("gap", Kind::Text),
    ("padding", Kind::Text),
    ("height", Kind::Text),
    ("background", Kind::Text),
    ("border", Kind::Text),
    ("radius", Kind::Text),
    ("opacity", Kind::Decimal { min: Bound::Inclusive(0.0), max: Bound::Inclusive(1.0) }),
    ("shadow", Kind::Text),
    ("paginate", Kind::Text),
    ("debug", Kind::Boolean),
    ("ocg", Kind::Text),
    ("colspan", Kind::Integer { min: Some(1), max: None }),
    ("rowspan", Kind::Integer { min: Some(1), max: None }),
    ("align", Kind::Text),
    ("valign", Kind::Text),
];

const THEAD: Attributes = &[
    ("background", Kind::Text),
    ("ocg", Kind::Text),
];

const TR: Attributes = &[
    ("keep", Kind::Text),
    ("background", Kind::Text),
    ("ocg", Kind::Text),
];

const TABLE: Attributes = &[
    ("cols", Kind::Text),
    ("border", Kind::Text),
    ("stripe", Kind::Text),
    ("head-border", Kind::Text),
    ("row-border", Kind::Text),
    ("gap", Kind::Text),
    ("padding", Kind::Text),
    ("background", Kind::Text),
    ("width", Kind::Text),
    ("height", Kind::Text),
    ("paginate", Kind::Text),
    ("debug", Kind::Boolean),
    ("ocg", Kind::Text),
];

const FIELD: Attributes = &[
    ("type", Kind::Text),
    ("name", Kind::Text),
    ("value", Kind::Text),
    ("label", Kind::Text),
    ("options", Kind::Text),
    ("group", Kind::Text),
    ("checked", Kind::Boolean),
    ("required", Kind::Boolean),
    ("readonly", Kind::Boolean),
    ("max-len", Kind::Integer { min: Some(1), max: None }),
    ("action-url", Kind::Text),
    ("width", Kind::Text),
    ("height", Kind::Text),
    ("background", Kind::Text),
    ("border", Kind::Text),
    ("debug", Kind::Boolean),
    ("ocg", Kind::Text),
];

/// The attributes of the element `name` written in the element `parent`, or `None` for an element the schema
/// does not have there.
pub(crate) fn attributes_of(parent: &str, name: &str) -> Option<Attributes> {
    match (parent, name) {
        ("tokens", "space") => Some(TOKENS_SPACE),
        ("tokens", "border") => Some(TOKENS_BORDER),
        ("tokens", "radius") => Some(TOKENS_RADIUS),
        ("tokens", "width") => Some(TOKENS_WIDTH),
        ("tokens", "height") => Some(TOKENS_HEIGHT),
        ("tokens", "text-size") => Some(TOKENS_TEXT_SIZE),
        ("tokens", "line-height") => Some(TOKENS_LINE_HEIGHT),
        ("tokens", "grid") => Some(TOKENS_GRID),
        ("tokens", "aliases") => Some(TOKENS_ALIASES),
        ("tokens", "colors") => Some(TOKENS_COLORS),
        ("tokens", "gradients") => Some(TOKENS_GRADIENTS),
        ("layout", "region") => Some(LAYOUT_REGION),
        ("markdown", "markdown-styles") => Some(MARKDOWN_MARKDOWN_STYLES),
        ("markdown-styles", "h1") => Some(MARKDOWN_STYLES_H1),
        ("markdown-styles", "h2") => Some(MARKDOWN_STYLES_H2),
        ("markdown-styles", "h3") => Some(MARKDOWN_STYLES_H3),
        ("markdown-styles", "h4") => Some(MARKDOWN_STYLES_H4),
        ("markdown-styles", "h5") => Some(MARKDOWN_STYLES_H5),
        ("markdown-styles", "h6") => Some(MARKDOWN_STYLES_H6),
        ("markdown-styles", "p") => Some(MARKDOWN_STYLES_P),
        ("markdown-styles", "code") => Some(MARKDOWN_STYLES_CODE),
        ("markdown-styles", "quote") => Some(MARKDOWN_STYLES_QUOTE),
        ("markdown-styles", "li") => Some(MARKDOWN_STYLES_LI),
        ("markdown-styles", "hr") => Some(MARKDOWN_STYLES_HR),
        ("markdown-styles", "a") => Some(MARKDOWN_STYLES_A),
        ("markdown-styles", "strong") => Some(MARKDOWN_STYLES_STRONG),
        ("markdown-styles", "em") => Some(MARKDOWN_STYLES_EM),
        ("markdown-styles", "del") => Some(MARKDOWN_STYLES_DEL),
        ("markdown-styles", "table") => Some(MARKDOWN_STYLES_TABLE),
        ("markdown-styles", "th") => Some(MARKDOWN_STYLES_TH),
        ("markdown-styles", "td") => Some(MARKDOWN_STYLES_TD),
        ("markdown", "markdown-content") => Some(MARKDOWN_MARKDOWN_CONTENT),
        ("layer", "rect") => Some(RECT),
        ("layer", "circle") => Some(CIRCLE),
        ("layer", "ellipse") => Some(ELLIPSE),
        ("layer", "line") => Some(LINE),
        ("layer", "path") => Some(PATH),
        ("layer", "text-at") => Some(TEXT_AT),
        ("layer", "img-at") => Some(IMG_AT),
        ("layer", "barcode-at") => Some(BARCODE_AT),
        ("layer", _) => None,
        (_, "rect") => Some(RECT),
        (_, "circle") => Some(CIRCLE),
        (_, "ellipse") => Some(ELLIPSE),
        (_, "line") => Some(LINE),
        (_, "path") => Some(PATH),
        (_, "text-at") => Some(TEXT_AT),
        (_, "img-at") => Some(IMG_AT),
        (_, "barcode-at") => Some(BARCODE_AT),
        (_, "layer") => Some(LAYER),
        (_, "font") => Some(FONT),
        (_, "image") => Some(IMAGE),
        (_, "profile") => Some(PROFILE),
        (_, "color") => Some(COLOR),
        (_, "alias") => Some(ALIAS),
        (_, "stop") => Some(STOP),
        (_, "gradient") => Some(GRADIENT),
        (_, "tokens") => Some(TOKENS),
        (_, "assets") => Some(ASSETS),
        (_, "meta") => Some(META),
        (_, "output") => Some(OUTPUT),
        (_, "attachments") => Some(ATTACHMENTS),
        (_, "file") => Some(FILE),
        (_, "layout") => Some(LAYOUT),
        (_, "markdown") => Some(MARKDOWN),
        (_, "document") => Some(DOCUMENT),
        (_, "section") => Some(SECTION),
        (_, "lpdf") => Some(LPDF),
        (_, "column") => Some(COLUMN),
        (_, "row") => Some(ROW),
        (_, "list") => Some(LIST),
        (_, "li") => Some(LI),
        (_, "grid") => Some(GRID),
        (_, "frame") => Some(FRAME),
        (_, "code") => Some(CODE),
        (_, "link") => Some(LINK),
        (_, "bookmark") => Some(BOOKMARK),
        (_, "divider") => Some(DIVIDER),
        (_, "span") => Some(SPAN),
        (_, "code-span") => Some(CODE_SPAN),
        (_, "fn") => Some(FN),
        (_, "img") => Some(IMG),
        (_, "barcode") => Some(BARCODE),
        (_, "shape") => Some(SHAPE),
        (_, "text") => Some(TEXT),
        (_, "td") => Some(TD),
        (_, "thead") => Some(THEAD),
        (_, "tr") => Some(TR),
        (_, "table") => Some(TABLE),
        (_, "field") => Some(FIELD),
        _ => None,
    }
}
