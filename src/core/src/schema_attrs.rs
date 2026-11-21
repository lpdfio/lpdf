// Generated from lpdf.xsd by scripts/gen-sdk-api.mjs.
// Do not edit: change the schema and run `make gen-sdk-api`.

//! The attributes of every element of lpdf.xsd and their types.

use crate::attr_types::{Bound, Constant, Kind};

/// The attributes of an element, in the order the schema lists them, without the data-* attributes.
pub(crate) type Attributes = &'static [(&'static str, Kind)];

const RECT: Attributes = &[
    ("x", Kind::Text),
    ("y", Kind::Text),
    ("width", Kind::Text),
    ("height", Kind::Text),
    ("anchor", Kind::Constants(&["Anchor"])),
    ("radius", Kind::Text),
    ("fill", Kind::Text),
    ("stroke", Kind::Text),
    ("stroke-width", Kind::Text),
    ("stroke-dash", Kind::Text),
    ("shadow", Kind::Text),
    ("ocg", Kind::Constants(&["Ocg"])),
];

const CIRCLE: Attributes = &[
    ("x", Kind::Text),
    ("y", Kind::Text),
    ("r", Kind::Text),
    ("anchor", Kind::Constants(&["Anchor"])),
    ("fill", Kind::Text),
    ("stroke", Kind::Text),
    ("stroke-width", Kind::Text),
    ("stroke-dash", Kind::Text),
    ("shadow", Kind::Text),
    ("ocg", Kind::Constants(&["Ocg"])),
];

const ELLIPSE: Attributes = &[
    ("x", Kind::Text),
    ("y", Kind::Text),
    ("rx", Kind::Text),
    ("ry", Kind::Text),
    ("anchor", Kind::Constants(&["Anchor"])),
    ("fill", Kind::Text),
    ("stroke", Kind::Text),
    ("stroke-width", Kind::Text),
    ("stroke-dash", Kind::Text),
    ("shadow", Kind::Text),
    ("ocg", Kind::Constants(&["Ocg"])),
];

const LINE: Attributes = &[
    ("x1", Kind::Text),
    ("y1", Kind::Text),
    ("x2", Kind::Text),
    ("y2", Kind::Text),
    ("stroke", Kind::Text),
    ("stroke-width", Kind::Text),
    ("stroke-dash", Kind::Text),
    ("line-cap", Kind::Constants(&["LineCap"])),
    ("ocg", Kind::Constants(&["Ocg"])),
];

const PATH: Attributes = &[
    ("d", Kind::Text),
    ("fill", Kind::Text),
    ("stroke", Kind::Text),
    ("fill-rule", Kind::Constants(&["FillRule"])),
    ("stroke-width", Kind::Text),
    ("stroke-dash", Kind::Text),
    ("line-cap", Kind::Constants(&["LineCap"])),
    ("ocg", Kind::Constants(&["Ocg"])),
];

const TEXT_AT: Attributes = &[
    ("x", Kind::Text),
    ("y", Kind::Text),
    ("anchor", Kind::Constants(&["Anchor"])),
    ("font-size", Kind::Text),
    ("font", Kind::Text),
    ("bold", Kind::Boolean),
    ("color", Kind::Text),
    ("align", Kind::Constants(&["TextAlign"])),
    ("line-height", Kind::NameOrDecimal { min: Bound::Exclusive(0.0), max: Bound::Open }),
    ("max-lines", Kind::Integer { min: Some(1), max: None }),
    ("ellipsis", Kind::Boolean),
    ("width", Kind::Text),
    ("ocg", Kind::Constants(&["Ocg"])),
];

const IMG_AT: Attributes = &[
    ("name", Kind::Text),
    ("x", Kind::Text),
    ("y", Kind::Text),
    ("anchor", Kind::Constants(&["Anchor"])),
    ("width", Kind::Text),
    ("height", Kind::Text),
    ("ocg", Kind::Constants(&["Ocg"])),
];

const BARCODE_AT: Attributes = &[
    ("type", Kind::Constants(&["BarcodeType"])),
    ("data", Kind::Text),
    ("height", Kind::Text),
    ("ec", Kind::Constants(&["QrEcLevel", "Pdf417EcLevel"])),
    ("hrt", Kind::Boolean),
    ("rows", Kind::Integer { min: Some(1), max: Some(144) }),
    ("columns", Kind::Integer { min: Some(1), max: Some(144) }),
    ("check", Kind::Boolean),
    ("color", Kind::Text),
    ("background", Kind::Text),
    ("x", Kind::Text),
    ("y", Kind::Text),
    ("anchor", Kind::Constants(&["Anchor"])),
    ("width", Kind::Text),
    ("ocg", Kind::Constants(&["Ocg"])),
];

const LAYER: Attributes = &[
    ("page", Kind::Constants(&["PageScope"])),
    ("opacity", Kind::Decimal { min: Bound::Inclusive(0.0), max: Bound::Inclusive(1.0) }),
    ("transform", Kind::Text),
    ("clip", Kind::Text),
    ("font", Kind::Text),
    ("font-size", Kind::Text),
    ("color", Kind::Text),
    ("alt", Kind::Text),
    ("ocg", Kind::Constants(&["Ocg"])),
];

const FONT: Attributes = &[
    ("name", Kind::Text),
    ("core", Kind::Constants(&["BuiltinFont"])),
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

const SPOT: Attributes = &[
    ("name", Kind::Text),
    ("ink", Kind::Text),
    ("value", Kind::Text),
    ("overprint", Kind::Boolean),
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
    ("type", Kind::Constants(&["GradientType"])),
    ("from", Kind::Text),
    ("to", Kind::Text),
    ("angle", Kind::Decimal { min: Bound::Open, max: Bound::Open }),
];

const LINES: Attributes = &[
    ("step", Kind::Text),
    ("step-x", Kind::Text),
    ("step-y", Kind::Text),
    ("count", Kind::Integer { min: Some(1), max: Some(100) }),
    ("count-x", Kind::Integer { min: Some(1), max: Some(100) }),
    ("count-y", Kind::Integer { min: Some(1), max: Some(100) }),
    ("color", Kind::Text),
    ("thickness", Kind::Text),
];

const RULING: Attributes = &[
    ("name", Kind::Text),
    ("origin", Kind::Constants(&["RulingOrigin"])),
    ("edges", Kind::Boolean),
    ("ocg", Kind::Constants(&["Ocg"])),
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

const TOKENS_SPOTS: Attributes = &[
];

const TOKENS_GRADIENTS: Attributes = &[
];

const TOKENS_RULINGS: Attributes = &[
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
    ("encrypt", Kind::Constants(&["Cipher"])),
    ("allow", Kind::Text),
    ("pdfa", Kind::Constants(&["PdfALevel"])),
    ("pdfua", Kind::Constants(&["PdfUaPart"])),
    ("pdfx", Kind::Constants(&["PdfXVersion"])),
    ("profile", Kind::Text),
    ("linearize", Kind::Boolean),
    ("facturx-strict", Kind::Boolean),
];

const ATTACHMENTS: Attributes = &[
];

const FILE: Attributes = &[
    ("name", Kind::Text),
    ("src", Kind::Text),
    ("ref", Kind::Text),
    ("mime", Kind::Text),
    ("relationship", Kind::Constants(&["FileRelationship"])),
    ("description", Kind::Text),
];

const LAYOUT: Attributes = &[
    ("margin", Kind::Text),
];

const LAYOUT_REGION: Attributes = &[
    ("pin", Kind::Constants(&["Pin"])),
    ("page", Kind::Constants(&["PageScope"])),
    ("w", Kind::Text),
    ("debug", Kind::Boolean),
    ("col", Kind::Integer { min: Some(1), max: Some(12) }),
    ("colspan", Kind::Integer { min: Some(1), max: Some(12) }),
    ("top", Kind::Text),
    ("height", Kind::Text),
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
    ("align", Kind::Constants(&["TextAlign"])),
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
    ("align", Kind::Constants(&["TextAlign"])),
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
    ("align", Kind::Constants(&["TextAlign"])),
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
    ("align", Kind::Constants(&["TextAlign"])),
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
    ("align", Kind::Constants(&["TextAlign"])),
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
    ("align", Kind::Constants(&["TextAlign"])),
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
    ("align", Kind::Constants(&["TextAlign"])),
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
    ("align", Kind::Constants(&["TextAlign"])),
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
    ("bullet", Kind::Constants(&["ListBullet"])),
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
    ("align", Kind::Constants(&["TextAlign"])),
    ("padding", Kind::Text),
    ("background", Kind::Text),
    ("border", Kind::Text),
];

const MARKDOWN_STYLES_TD: Attributes = &[
    ("font", Kind::Text),
    ("font-size", Kind::Text),
    ("bold", Kind::Boolean),
    ("color", Kind::Text),
    ("align", Kind::Constants(&["TextAlign"])),
    ("padding", Kind::Text),
    ("background", Kind::Text),
    ("border", Kind::Text),
];

const MARKDOWN_MARKDOWN_CONTENT: Attributes = &[
];

const DOCUMENT: Attributes = &[
    ("font", Kind::Text),
    ("size", Kind::Constants(&["PageSize"])),
    ("orientation", Kind::Constants(&["Orientation"])),
    ("bleed", Kind::Text),
    ("background", Kind::Text),
    ("debug", Kind::Boolean),
    ("footnote-counter", Kind::Constants(&["FootnoteCounter"])),
];

const SECTION: Attributes = &[
    ("size", Kind::Constants(&["PageSize"])),
    ("orientation", Kind::Constants(&["Orientation"])),
    ("bleed", Kind::Text),
    ("background", Kind::Text),
    ("ruling", Kind::Text),
    ("title", Kind::Text),
    ("debug", Kind::Boolean),
    ("type", Kind::Constants(&["SectionType"])),
    ("footnote-counter", Kind::Constants(&["FootnoteCounter"])),
    ("toc-indent-step", Kind::Text),
    ("page-start", Kind::Integer { min: Some(1), max: Some(2147483647) }),
    ("page-format", Kind::Constants(&["ListFormat"])),
    ("page-prefix", Kind::Text),
    ("columns", Kind::Integer { min: Some(1), max: Some(12) }),
    ("column-gap", Kind::Text),
    ("column-rule", Kind::Text),
    ("column-fill", Kind::Constants(&["ColumnFill"])),
    ("grow", Kind::Constants(&["Grow"])),
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
    ("paginate", Kind::Constants(&["Paginate"])),
    ("debug", Kind::Boolean),
    ("ocg", Kind::Constants(&["Ocg"])),
    ("align", Kind::Constants(&["BoxAlign"])),
    ("justify", Kind::Constants(&["BoxJustify"])),
    ("width", Kind::Text),
    ("bulk", Kind::Boolean),
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
    ("paginate", Kind::Constants(&["Paginate"])),
    ("debug", Kind::Boolean),
    ("ocg", Kind::Constants(&["Ocg"])),
    ("align", Kind::Constants(&["BoxAlign"])),
    ("justify", Kind::Constants(&["BoxJustify"])),
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
    ("paginate", Kind::Constants(&["Paginate"])),
    ("debug", Kind::Boolean),
    ("ocg", Kind::Constants(&["Ocg"])),
    ("type", Kind::Constants(&["ListType"])),
    ("format", Kind::Constants(&["ListFormat"])),
    ("start", Kind::Integer { min: None, max: None }),
    ("prefix", Kind::Text),
    ("suffix", Kind::Text),
    ("bullet", Kind::Constants(&["ListBullet"])),
    ("indent", Kind::Text),
    ("width", Kind::Text),
    ("bulk", Kind::Boolean),
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
    ("paginate", Kind::Constants(&["Paginate"])),
    ("debug", Kind::Boolean),
    ("ocg", Kind::Constants(&["Ocg"])),
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
    ("paginate", Kind::Constants(&["Paginate"])),
    ("debug", Kind::Boolean),
    ("ocg", Kind::Constants(&["Ocg"])),
    ("width", Kind::Text),
    ("col-width", Kind::Text),
    ("cols", Kind::Integer { min: Some(1), max: Some(12) }),
    ("bulk", Kind::Boolean),
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
    ("paginate", Kind::Constants(&["Paginate"])),
    ("debug", Kind::Boolean),
    ("ocg", Kind::Constants(&["Ocg"])),
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
    ("paginate", Kind::Constants(&["Paginate"])),
    ("debug", Kind::Boolean),
    ("ocg", Kind::Constants(&["Ocg"])),
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
    ("ocg", Kind::Constants(&["Ocg"])),
];

const BOOKMARK: Attributes = &[
    ("label", Kind::Text),
    ("level", Kind::Integer { min: Some(1), max: Some(6) }),
    ("id", Kind::Text),
    ("ref-only", Kind::Boolean),
];

const DIVIDER: Attributes = &[
    ("direction", Kind::Constants(&["DividerDirection"])),
    ("color", Kind::Text),
    ("thickness", Kind::Text),
    ("width", Kind::Text),
    ("debug", Kind::Boolean),
    ("ocg", Kind::Constants(&["Ocg"])),
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
    ("paginate", Kind::Constants(&["Paginate"])),
    ("debug", Kind::Boolean),
    ("ocg", Kind::Constants(&["Ocg"])),
];

const BARCODE: Attributes = &[
    ("type", Kind::Constants(&["BarcodeType"])),
    ("data", Kind::Text),
    ("height", Kind::Text),
    ("ec", Kind::Constants(&["QrEcLevel", "Pdf417EcLevel"])),
    ("hrt", Kind::Boolean),
    ("rows", Kind::Integer { min: Some(1), max: Some(144) }),
    ("columns", Kind::Integer { min: Some(1), max: Some(144) }),
    ("check", Kind::Boolean),
    ("color", Kind::Text),
    ("background", Kind::Text),
    ("width", Kind::Text),
    ("alt", Kind::Text),
    ("paginate", Kind::Constants(&["Paginate"])),
    ("debug", Kind::Boolean),
    ("ocg", Kind::Constants(&["Ocg"])),
];

const SHAPE: Attributes = &[
    ("type", Kind::Constants(&["ShapeType"])),
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
    ("ocg", Kind::Constants(&["Ocg"])),
];

const TEXT: Attributes = &[
    ("font-size", Kind::Text),
    ("font", Kind::Text),
    ("bold", Kind::Boolean),
    ("color", Kind::Text),
    ("align", Kind::Constants(&["TextAlign"])),
    ("line-height", Kind::NameOrDecimal { min: Bound::Exclusive(0.0), max: Bound::Open }),
    ("max-lines", Kind::Integer { min: Some(1), max: None }),
    ("ellipsis", Kind::Boolean),
    ("width", Kind::Text),
    ("role", Kind::Constants(&["TextRole"])),
    ("widows", Kind::Integer { min: Some(1), max: Some(10) }),
    ("orphans", Kind::Integer { min: Some(1), max: Some(10) }),
    ("capture", Kind::Text),
    ("capture-ref", Kind::Text),
    ("capture-mode", Kind::Constants(&["CaptureMode"])),
    ("capture-fallback", Kind::Text),
    ("paginate", Kind::Constants(&["Paginate"])),
    ("debug", Kind::Boolean),
    ("ocg", Kind::Constants(&["Ocg"])),
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
    ("paginate", Kind::Constants(&["Paginate"])),
    ("debug", Kind::Boolean),
    ("ocg", Kind::Constants(&["Ocg"])),
    ("colspan", Kind::Integer { min: Some(1), max: None }),
    ("rowspan", Kind::Integer { min: Some(1), max: None }),
    ("align", Kind::Constants(&["Align"])),
    ("valign", Kind::Constants(&["CellVAlign"])),
];

const THEAD: Attributes = &[
    ("background", Kind::Text),
    ("ocg", Kind::Constants(&["Ocg"])),
];

const TR: Attributes = &[
    ("keep", Kind::Constants(&["RowKeep"])),
    ("background", Kind::Text),
    ("ocg", Kind::Constants(&["Ocg"])),
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
    ("paginate", Kind::Constants(&["Paginate"])),
    ("debug", Kind::Boolean),
    ("bulk", Kind::Boolean),
    ("ocg", Kind::Constants(&["Ocg"])),
];

const FIELD: Attributes = &[
    ("type", Kind::Constants(&["FieldType"])),
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
    ("ocg", Kind::Constants(&["Ocg"])),
];

/// The constants of the SDKs, one for each closed enumeration of the schema: the name, the area it is in (the .NET
/// and PHP namespace) and its members, each a value with its name in Pascal case and in upper snake case.
#[allow(dead_code, reason = "only codegen asks, and the WASI binary and the native library have no codegen")]
pub(crate) const CONSTANTS: &[Constant] = &[
    Constant { name: "PageSize", area: "kit", members: &[
        ("a4", "A4", "A4"),
        ("a3", "A3", "A3"),
        ("a5", "A5", "A5"),
        ("letter", "Letter", "LETTER"),
        ("legal", "Legal", "LEGAL"),
    ] },
    Constant { name: "Orientation", area: "kit", members: &[
        ("portrait", "Portrait", "PORTRAIT"),
        ("landscape", "Landscape", "LANDSCAPE"),
    ] },
    Constant { name: "SectionType", area: "kit", members: &[
        ("toc", "Toc", "TOC"),
    ] },
    Constant { name: "ColumnFill", area: "kit", members: &[
        ("auto", "Auto", "AUTO"),
        ("balance", "Balance", "BALANCE"),
    ] },
    Constant { name: "Grow", area: "kit", members: &[
        ("width", "Width", "WIDTH"),
        ("height", "Height", "HEIGHT"),
        ("width height", "WidthHeight", "WIDTH_HEIGHT"),
    ] },
    Constant { name: "FootnoteCounter", area: "kit", members: &[
        ("page", "Page", "PAGE"),
        ("section", "Section", "SECTION"),
        ("document", "Document", "DOCUMENT"),
    ] },
    Constant { name: "Cipher", area: "kit", members: &[
        ("aes-256", "Aes256", "AES_256"),
        ("rc4-128", "Rc4128", "RC4_128"),
    ] },
    Constant { name: "PdfALevel", area: "kit", members: &[
        ("2a", "Level2a", "LEVEL_2A"),
        ("2b", "Level2b", "LEVEL_2B"),
        ("2u", "Level2u", "LEVEL_2U"),
        ("3a", "Level3a", "LEVEL_3A"),
        ("3b", "Level3b", "LEVEL_3B"),
        ("3u", "Level3u", "LEVEL_3U"),
    ] },
    Constant { name: "PdfUaPart", area: "kit", members: &[
        ("1", "Part1", "PART_1"),
    ] },
    Constant { name: "PdfXVersion", area: "kit", members: &[
        ("4", "Version4", "VERSION_4"),
    ] },
    Constant { name: "TextRole", area: "layout", members: &[
        ("h1", "H1", "H1"),
        ("h2", "H2", "H2"),
        ("h3", "H3", "H3"),
        ("h4", "H4", "H4"),
        ("h5", "H5", "H5"),
        ("h6", "H6", "H6"),
    ] },
    Constant { name: "Permission", area: "kit", members: &[
        ("all", "All", "ALL"),
        ("print", "Print", "PRINT"),
        ("modify", "Modify", "MODIFY"),
        ("copy", "Copy", "COPY"),
        ("annotate", "Annotate", "ANNOTATE"),
        ("fill-forms", "FillForms", "FILL_FORMS"),
        ("accessibility", "Accessibility", "ACCESSIBILITY"),
        ("assemble", "Assemble", "ASSEMBLE"),
    ] },
    Constant { name: "FileRelationship", area: "kit", members: &[
        ("source", "Source", "SOURCE"),
        ("data", "Data", "DATA"),
        ("alternative", "Alternative", "ALTERNATIVE"),
        ("supplement", "Supplement", "SUPPLEMENT"),
        ("unspecified", "Unspecified", "UNSPECIFIED"),
    ] },
    Constant { name: "GradientType", area: "kit", members: &[
        ("linear", "Linear", "LINEAR"),
        ("radial", "Radial", "RADIAL"),
    ] },
    Constant { name: "RulingOrigin", area: "kit", members: &[
        ("page", "Page", "PAGE"),
        ("margin", "Margin", "MARGIN"),
    ] },
    Constant { name: "BoxAlign", area: "layout", members: &[
        ("start", "Start", "START"),
        ("center", "Center", "CENTER"),
        ("end", "End", "END"),
        ("stretch", "Stretch", "STRETCH"),
    ] },
    Constant { name: "Align", area: "layout", members: &[
        ("start", "Start", "START"),
        ("center", "Center", "CENTER"),
        ("end", "End", "END"),
    ] },
    Constant { name: "BoxJustify", area: "layout", members: &[
        ("start", "Start", "START"),
        ("center", "Center", "CENTER"),
        ("end", "End", "END"),
        ("between", "Between", "BETWEEN"),
    ] },
    Constant { name: "CaptureMode", area: "layout", members: &[
        ("first", "First", "FIRST"),
        ("last", "Last", "LAST"),
    ] },
    Constant { name: "Paginate", area: "layout", members: &[
        ("no", "No", "NO"),
        ("break-before", "BreakBefore", "BREAK_BEFORE"),
        ("break-after", "BreakAfter", "BREAK_AFTER"),
        ("keep-next", "KeepNext", "KEEP_NEXT"),
    ] },
    Constant { name: "RowKeep", area: "layout", members: &[
        ("with-next", "WithNext", "WITH_NEXT"),
    ] },
    Constant { name: "Ocg", area: "shared", members: &[
        ("print", "Print", "PRINT"),
        ("screen", "Screen", "SCREEN"),
    ] },
    Constant { name: "ListType", area: "layout", members: &[
        ("bullet", "Bullet", "BULLET"),
        ("number", "Number", "NUMBER"),
    ] },
    Constant { name: "ListFormat", area: "shared", members: &[
        ("decimal", "Decimal", "DECIMAL"),
        ("lower-alpha", "LowerAlpha", "LOWER_ALPHA"),
        ("upper-alpha", "UpperAlpha", "UPPER_ALPHA"),
        ("lower-roman", "LowerRoman", "LOWER_ROMAN"),
        ("upper-roman", "UpperRoman", "UPPER_ROMAN"),
    ] },
    Constant { name: "ListBullet", area: "shared", members: &[
        ("disc", "Disc", "DISC"),
        ("circle", "Circle", "CIRCLE"),
        ("square", "Square", "SQUARE"),
        ("dash", "Dash", "DASH"),
    ] },
    Constant { name: "TextAlign", area: "shared", members: &[
        ("left", "Left", "LEFT"),
        ("center", "Center", "CENTER"),
        ("right", "Right", "RIGHT"),
        ("justify", "Justify", "JUSTIFY"),
    ] },
    Constant { name: "Anchor", area: "canvas", members: &[
        ("top-left", "TopLeft", "TOP_LEFT"),
        ("top-center", "TopCenter", "TOP_CENTER"),
        ("top-right", "TopRight", "TOP_RIGHT"),
        ("center-left", "CenterLeft", "CENTER_LEFT"),
        ("center", "Center", "CENTER"),
        ("center-right", "CenterRight", "CENTER_RIGHT"),
        ("bottom-left", "BottomLeft", "BOTTOM_LEFT"),
        ("bottom-center", "BottomCenter", "BOTTOM_CENTER"),
        ("bottom-right", "BottomRight", "BOTTOM_RIGHT"),
    ] },
    Constant { name: "PageScope", area: "shared", members: &[
        ("each", "Each", "EACH"),
        ("first", "First", "FIRST"),
        ("last", "Last", "LAST"),
        ("odd", "Odd", "ODD"),
        ("even", "Even", "EVEN"),
    ] },
    Constant { name: "LineCap", area: "canvas", members: &[
        ("butt", "Butt", "BUTT"),
        ("round", "Round", "ROUND"),
        ("square", "Square", "SQUARE"),
    ] },
    Constant { name: "FillRule", area: "canvas", members: &[
        ("nonzero", "Nonzero", "NONZERO"),
        ("evenodd", "Evenodd", "EVENODD"),
    ] },
    Constant { name: "Pin", area: "layout", members: &[
        ("top", "Top", "TOP"),
        ("bottom", "Bottom", "BOTTOM"),
        ("left", "Left", "LEFT"),
        ("right", "Right", "RIGHT"),
        ("inset", "Inset", "INSET"),
    ] },
    Constant { name: "BarcodeType", area: "shared", members: &[
        ("qr", "Qr", "QR"),
        ("code128", "Code128", "CODE128"),
        ("ean13", "Ean13", "EAN13"),
        ("code39", "Code39", "CODE39"),
        ("ean8", "Ean8", "EAN8"),
        ("upca", "Upca", "UPCA"),
        ("datamatrix", "Datamatrix", "DATAMATRIX"),
        ("pdf417", "Pdf417", "PDF417"),
        ("gs1-128", "Gs1128", "GS1_128"),
        ("gs1-datamatrix", "Gs1Datamatrix", "GS1_DATAMATRIX"),
    ] },
    Constant { name: "QrEcLevel", area: "shared", members: &[
        ("L", "L", "L"),
        ("M", "M", "M"),
        ("Q", "Q", "Q"),
        ("H", "H", "H"),
    ] },
    Constant { name: "Pdf417EcLevel", area: "shared", members: &[
        ("0", "Level0", "LEVEL_0"),
        ("1", "Level1", "LEVEL_1"),
        ("2", "Level2", "LEVEL_2"),
        ("3", "Level3", "LEVEL_3"),
        ("4", "Level4", "LEVEL_4"),
        ("5", "Level5", "LEVEL_5"),
        ("6", "Level6", "LEVEL_6"),
        ("7", "Level7", "LEVEL_7"),
        ("8", "Level8", "LEVEL_8"),
    ] },
    Constant { name: "DividerDirection", area: "layout", members: &[
        ("horizontal", "Horizontal", "HORIZONTAL"),
        ("vertical", "Vertical", "VERTICAL"),
    ] },
    Constant { name: "ShapeType", area: "layout", members: &[
        ("rect", "Rect", "RECT"),
        ("circle", "Circle", "CIRCLE"),
        ("ellipse", "Ellipse", "ELLIPSE"),
    ] },
    Constant { name: "CellVAlign", area: "layout", members: &[
        ("top", "Top", "TOP"),
        ("middle", "Middle", "MIDDLE"),
        ("bottom", "Bottom", "BOTTOM"),
    ] },
    Constant { name: "FieldType", area: "layout", members: &[
        ("text", "Text", "TEXT"),
        ("checkbox", "Checkbox", "CHECKBOX"),
        ("dropdown", "Dropdown", "DROPDOWN"),
        ("radio", "Radio", "RADIO"),
        ("button", "Button", "BUTTON"),
    ] },
    Constant { name: "BuiltinFont", area: "kit", members: &[
        ("Courier", "Courier", "COURIER"),
        ("Courier-Bold", "CourierBold", "COURIER_BOLD"),
        ("Courier-Oblique", "CourierOblique", "COURIER_OBLIQUE"),
        ("Courier-BoldOblique", "CourierBoldOblique", "COURIER_BOLDOBLIQUE"),
        ("Helvetica", "Helvetica", "HELVETICA"),
        ("Helvetica-Bold", "HelveticaBold", "HELVETICA_BOLD"),
        ("Helvetica-Oblique", "HelveticaOblique", "HELVETICA_OBLIQUE"),
        ("Helvetica-BoldOblique", "HelveticaBoldOblique", "HELVETICA_BOLDOBLIQUE"),
        ("Times-Roman", "TimesRoman", "TIMES_ROMAN"),
        ("Times-Bold", "TimesBold", "TIMES_BOLD"),
        ("Times-Italic", "TimesItalic", "TIMES_ITALIC"),
        ("Times-BoldItalic", "TimesBoldItalic", "TIMES_BOLDITALIC"),
        ("Symbol", "Symbol", "SYMBOL"),
        ("ZapfDingbats", "ZapfDingbats", "ZAPFDINGBATS"),
    ] },
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
        ("tokens", "spots") => Some(TOKENS_SPOTS),
        ("tokens", "gradients") => Some(TOKENS_GRADIENTS),
        ("tokens", "rulings") => Some(TOKENS_RULINGS),
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
        (_, "spot") => Some(SPOT),
        (_, "alias") => Some(ALIAS),
        (_, "stop") => Some(STOP),
        (_, "gradient") => Some(GRADIENT),
        (_, "lines") => Some(LINES),
        (_, "ruling") => Some(RULING),
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
