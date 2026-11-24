// Generated from lpdf.xsd by scripts/gen-sdk-api.mjs.
// Do not edit: change the schema and run `make gen-sdk-api`.

//! The attributes of every element of lpdf.xsd and their types.

use crate::attr_types::{Bound, Constant, Kind, Member};

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
        Member { value: "A0", pascal: "A0", snake: "A0" },
        Member { value: "A1", pascal: "A1", snake: "A1" },
        Member { value: "A2", pascal: "A2", snake: "A2" },
        Member { value: "A3", pascal: "A3", snake: "A3" },
        Member { value: "A4", pascal: "A4", snake: "A4" },
        Member { value: "A5", pascal: "A5", snake: "A5" },
        Member { value: "A6", pascal: "A6", snake: "A6" },
        Member { value: "B4", pascal: "B4", snake: "B4" },
        Member { value: "B5", pascal: "B5", snake: "B5" },
        Member { value: "Letter", pascal: "Letter", snake: "LETTER" },
        Member { value: "Legal", pascal: "Legal", snake: "LEGAL" },
        Member { value: "Tabloid", pascal: "Tabloid", snake: "TABLOID" },
    ] },
    Constant { name: "Orientation", area: "kit", members: &[
        Member { value: "portrait", pascal: "Portrait", snake: "PORTRAIT" },
        Member { value: "landscape", pascal: "Landscape", snake: "LANDSCAPE" },
    ] },
    Constant { name: "SectionType", area: "kit", members: &[
        Member { value: "toc", pascal: "Toc", snake: "TOC" },
    ] },
    Constant { name: "ColumnFill", area: "kit", members: &[
        Member { value: "auto", pascal: "Auto", snake: "AUTO" },
        Member { value: "balance", pascal: "Balance", snake: "BALANCE" },
    ] },
    Constant { name: "Grow", area: "kit", members: &[
        Member { value: "width", pascal: "Width", snake: "WIDTH" },
        Member { value: "height", pascal: "Height", snake: "HEIGHT" },
        Member { value: "width height", pascal: "WidthHeight", snake: "WIDTH_HEIGHT" },
    ] },
    Constant { name: "FootnoteCounter", area: "kit", members: &[
        Member { value: "page", pascal: "Page", snake: "PAGE" },
        Member { value: "section", pascal: "Section", snake: "SECTION" },
        Member { value: "document", pascal: "Document", snake: "DOCUMENT" },
    ] },
    Constant { name: "Cipher", area: "kit", members: &[
        Member { value: "aes-256", pascal: "Aes256", snake: "AES_256" },
        Member { value: "rc4-128", pascal: "Rc4128", snake: "RC4_128" },
    ] },
    Constant { name: "PdfALevel", area: "kit", members: &[
        Member { value: "2a", pascal: "Level2a", snake: "LEVEL_2A" },
        Member { value: "2b", pascal: "Level2b", snake: "LEVEL_2B" },
        Member { value: "2u", pascal: "Level2u", snake: "LEVEL_2U" },
        Member { value: "3a", pascal: "Level3a", snake: "LEVEL_3A" },
        Member { value: "3b", pascal: "Level3b", snake: "LEVEL_3B" },
        Member { value: "3u", pascal: "Level3u", snake: "LEVEL_3U" },
    ] },
    Constant { name: "PdfUaPart", area: "kit", members: &[
        Member { value: "1", pascal: "Part1", snake: "PART_1" },
    ] },
    Constant { name: "PdfXVersion", area: "kit", members: &[
        Member { value: "4", pascal: "Version4", snake: "VERSION_4" },
    ] },
    Constant { name: "TextRole", area: "layout", members: &[
        Member { value: "h1", pascal: "H1", snake: "H1" },
        Member { value: "h2", pascal: "H2", snake: "H2" },
        Member { value: "h3", pascal: "H3", snake: "H3" },
        Member { value: "h4", pascal: "H4", snake: "H4" },
        Member { value: "h5", pascal: "H5", snake: "H5" },
        Member { value: "h6", pascal: "H6", snake: "H6" },
    ] },
    Constant { name: "Permission", area: "kit", members: &[
        Member { value: "all", pascal: "All", snake: "ALL" },
        Member { value: "print", pascal: "Print", snake: "PRINT" },
        Member { value: "modify", pascal: "Modify", snake: "MODIFY" },
        Member { value: "copy", pascal: "Copy", snake: "COPY" },
        Member { value: "annotate", pascal: "Annotate", snake: "ANNOTATE" },
        Member { value: "fill-forms", pascal: "FillForms", snake: "FILL_FORMS" },
        Member { value: "accessibility", pascal: "Accessibility", snake: "ACCESSIBILITY" },
        Member { value: "assemble", pascal: "Assemble", snake: "ASSEMBLE" },
    ] },
    Constant { name: "FileRelationship", area: "kit", members: &[
        Member { value: "source", pascal: "Source", snake: "SOURCE" },
        Member { value: "data", pascal: "Data", snake: "DATA" },
        Member { value: "alternative", pascal: "Alternative", snake: "ALTERNATIVE" },
        Member { value: "supplement", pascal: "Supplement", snake: "SUPPLEMENT" },
        Member { value: "unspecified", pascal: "Unspecified", snake: "UNSPECIFIED" },
    ] },
    Constant { name: "GradientType", area: "kit", members: &[
        Member { value: "linear", pascal: "Linear", snake: "LINEAR" },
        Member { value: "radial", pascal: "Radial", snake: "RADIAL" },
    ] },
    Constant { name: "RulingOrigin", area: "kit", members: &[
        Member { value: "page", pascal: "Page", snake: "PAGE" },
        Member { value: "margin", pascal: "Margin", snake: "MARGIN" },
    ] },
    Constant { name: "BoxAlign", area: "layout", members: &[
        Member { value: "start", pascal: "Start", snake: "START" },
        Member { value: "center", pascal: "Center", snake: "CENTER" },
        Member { value: "end", pascal: "End", snake: "END" },
        Member { value: "stretch", pascal: "Stretch", snake: "STRETCH" },
    ] },
    Constant { name: "Align", area: "layout", members: &[
        Member { value: "start", pascal: "Start", snake: "START" },
        Member { value: "center", pascal: "Center", snake: "CENTER" },
        Member { value: "end", pascal: "End", snake: "END" },
    ] },
    Constant { name: "BoxJustify", area: "layout", members: &[
        Member { value: "start", pascal: "Start", snake: "START" },
        Member { value: "center", pascal: "Center", snake: "CENTER" },
        Member { value: "end", pascal: "End", snake: "END" },
        Member { value: "between", pascal: "Between", snake: "BETWEEN" },
    ] },
    Constant { name: "CaptureMode", area: "layout", members: &[
        Member { value: "first", pascal: "First", snake: "FIRST" },
        Member { value: "last", pascal: "Last", snake: "LAST" },
    ] },
    Constant { name: "Paginate", area: "layout", members: &[
        Member { value: "no", pascal: "No", snake: "NO" },
        Member { value: "break-before", pascal: "BreakBefore", snake: "BREAK_BEFORE" },
        Member { value: "break-after", pascal: "BreakAfter", snake: "BREAK_AFTER" },
        Member { value: "keep-next", pascal: "KeepNext", snake: "KEEP_NEXT" },
    ] },
    Constant { name: "RowKeep", area: "layout", members: &[
        Member { value: "with-next", pascal: "WithNext", snake: "WITH_NEXT" },
    ] },
    Constant { name: "Ocg", area: "shared", members: &[
        Member { value: "print", pascal: "Print", snake: "PRINT" },
        Member { value: "screen", pascal: "Screen", snake: "SCREEN" },
    ] },
    Constant { name: "ListType", area: "layout", members: &[
        Member { value: "bullet", pascal: "Bullet", snake: "BULLET" },
        Member { value: "number", pascal: "Number", snake: "NUMBER" },
    ] },
    Constant { name: "ListFormat", area: "shared", members: &[
        Member { value: "decimal", pascal: "Decimal", snake: "DECIMAL" },
        Member { value: "lower-alpha", pascal: "LowerAlpha", snake: "LOWER_ALPHA" },
        Member { value: "upper-alpha", pascal: "UpperAlpha", snake: "UPPER_ALPHA" },
        Member { value: "lower-roman", pascal: "LowerRoman", snake: "LOWER_ROMAN" },
        Member { value: "upper-roman", pascal: "UpperRoman", snake: "UPPER_ROMAN" },
    ] },
    Constant { name: "ListBullet", area: "shared", members: &[
        Member { value: "disc", pascal: "Disc", snake: "DISC" },
        Member { value: "circle", pascal: "Circle", snake: "CIRCLE" },
        Member { value: "square", pascal: "Square", snake: "SQUARE" },
        Member { value: "dash", pascal: "Dash", snake: "DASH" },
    ] },
    Constant { name: "TextAlign", area: "shared", members: &[
        Member { value: "left", pascal: "Left", snake: "LEFT" },
        Member { value: "center", pascal: "Center", snake: "CENTER" },
        Member { value: "right", pascal: "Right", snake: "RIGHT" },
        Member { value: "justify", pascal: "Justify", snake: "JUSTIFY" },
    ] },
    Constant { name: "Anchor", area: "canvas", members: &[
        Member { value: "top-left", pascal: "TopLeft", snake: "TOP_LEFT" },
        Member { value: "top-center", pascal: "TopCenter", snake: "TOP_CENTER" },
        Member { value: "top-right", pascal: "TopRight", snake: "TOP_RIGHT" },
        Member { value: "center-left", pascal: "CenterLeft", snake: "CENTER_LEFT" },
        Member { value: "center", pascal: "Center", snake: "CENTER" },
        Member { value: "center-right", pascal: "CenterRight", snake: "CENTER_RIGHT" },
        Member { value: "bottom-left", pascal: "BottomLeft", snake: "BOTTOM_LEFT" },
        Member { value: "bottom-center", pascal: "BottomCenter", snake: "BOTTOM_CENTER" },
        Member { value: "bottom-right", pascal: "BottomRight", snake: "BOTTOM_RIGHT" },
    ] },
    Constant { name: "PageScope", area: "shared", members: &[
        Member { value: "each", pascal: "Each", snake: "EACH" },
        Member { value: "first", pascal: "First", snake: "FIRST" },
        Member { value: "last", pascal: "Last", snake: "LAST" },
        Member { value: "odd", pascal: "Odd", snake: "ODD" },
        Member { value: "even", pascal: "Even", snake: "EVEN" },
    ] },
    Constant { name: "LineCap", area: "canvas", members: &[
        Member { value: "butt", pascal: "Butt", snake: "BUTT" },
        Member { value: "round", pascal: "Round", snake: "ROUND" },
        Member { value: "square", pascal: "Square", snake: "SQUARE" },
    ] },
    Constant { name: "FillRule", area: "canvas", members: &[
        Member { value: "nonzero", pascal: "Nonzero", snake: "NONZERO" },
        Member { value: "evenodd", pascal: "Evenodd", snake: "EVENODD" },
    ] },
    Constant { name: "Pin", area: "layout", members: &[
        Member { value: "top", pascal: "Top", snake: "TOP" },
        Member { value: "bottom", pascal: "Bottom", snake: "BOTTOM" },
        Member { value: "left", pascal: "Left", snake: "LEFT" },
        Member { value: "right", pascal: "Right", snake: "RIGHT" },
        Member { value: "inset", pascal: "Inset", snake: "INSET" },
    ] },
    Constant { name: "BarcodeType", area: "shared", members: &[
        Member { value: "qr", pascal: "Qr", snake: "QR" },
        Member { value: "code128", pascal: "Code128", snake: "CODE128" },
        Member { value: "ean13", pascal: "Ean13", snake: "EAN13" },
        Member { value: "code39", pascal: "Code39", snake: "CODE39" },
        Member { value: "ean8", pascal: "Ean8", snake: "EAN8" },
        Member { value: "upca", pascal: "Upca", snake: "UPCA" },
        Member { value: "datamatrix", pascal: "Datamatrix", snake: "DATAMATRIX" },
        Member { value: "pdf417", pascal: "Pdf417", snake: "PDF417" },
        Member { value: "gs1-128", pascal: "Gs1128", snake: "GS1_128" },
        Member { value: "gs1-datamatrix", pascal: "Gs1Datamatrix", snake: "GS1_DATAMATRIX" },
    ] },
    Constant { name: "QrEcLevel", area: "shared", members: &[
        Member { value: "L", pascal: "L", snake: "L" },
        Member { value: "M", pascal: "M", snake: "M" },
        Member { value: "Q", pascal: "Q", snake: "Q" },
        Member { value: "H", pascal: "H", snake: "H" },
    ] },
    Constant { name: "Pdf417EcLevel", area: "shared", members: &[
        Member { value: "0", pascal: "Level0", snake: "LEVEL_0" },
        Member { value: "1", pascal: "Level1", snake: "LEVEL_1" },
        Member { value: "2", pascal: "Level2", snake: "LEVEL_2" },
        Member { value: "3", pascal: "Level3", snake: "LEVEL_3" },
        Member { value: "4", pascal: "Level4", snake: "LEVEL_4" },
        Member { value: "5", pascal: "Level5", snake: "LEVEL_5" },
        Member { value: "6", pascal: "Level6", snake: "LEVEL_6" },
        Member { value: "7", pascal: "Level7", snake: "LEVEL_7" },
        Member { value: "8", pascal: "Level8", snake: "LEVEL_8" },
    ] },
    Constant { name: "DividerDirection", area: "layout", members: &[
        Member { value: "horizontal", pascal: "Horizontal", snake: "HORIZONTAL" },
        Member { value: "vertical", pascal: "Vertical", snake: "VERTICAL" },
    ] },
    Constant { name: "ShapeType", area: "layout", members: &[
        Member { value: "rect", pascal: "Rect", snake: "RECT" },
        Member { value: "circle", pascal: "Circle", snake: "CIRCLE" },
        Member { value: "ellipse", pascal: "Ellipse", snake: "ELLIPSE" },
    ] },
    Constant { name: "CellVAlign", area: "layout", members: &[
        Member { value: "top", pascal: "Top", snake: "TOP" },
        Member { value: "middle", pascal: "Middle", snake: "MIDDLE" },
        Member { value: "bottom", pascal: "Bottom", snake: "BOTTOM" },
    ] },
    Constant { name: "FieldType", area: "layout", members: &[
        Member { value: "text", pascal: "Text", snake: "TEXT" },
        Member { value: "checkbox", pascal: "Checkbox", snake: "CHECKBOX" },
        Member { value: "dropdown", pascal: "Dropdown", snake: "DROPDOWN" },
        Member { value: "radio", pascal: "Radio", snake: "RADIO" },
        Member { value: "button", pascal: "Button", snake: "BUTTON" },
    ] },
    Constant { name: "BuiltinFont", area: "kit", members: &[
        Member { value: "Courier", pascal: "Courier", snake: "COURIER" },
        Member { value: "Courier-Bold", pascal: "CourierBold", snake: "COURIER_BOLD" },
        Member { value: "Courier-Oblique", pascal: "CourierOblique", snake: "COURIER_OBLIQUE" },
        Member { value: "Courier-BoldOblique", pascal: "CourierBoldOblique", snake: "COURIER_BOLDOBLIQUE" },
        Member { value: "Helvetica", pascal: "Helvetica", snake: "HELVETICA" },
        Member { value: "Helvetica-Bold", pascal: "HelveticaBold", snake: "HELVETICA_BOLD" },
        Member { value: "Helvetica-Oblique", pascal: "HelveticaOblique", snake: "HELVETICA_OBLIQUE" },
        Member { value: "Helvetica-BoldOblique", pascal: "HelveticaBoldOblique", snake: "HELVETICA_BOLDOBLIQUE" },
        Member { value: "Times-Roman", pascal: "TimesRoman", snake: "TIMES_ROMAN" },
        Member { value: "Times-Bold", pascal: "TimesBold", snake: "TIMES_BOLD" },
        Member { value: "Times-Italic", pascal: "TimesItalic", snake: "TIMES_ITALIC" },
        Member { value: "Times-BoldItalic", pascal: "TimesBoldItalic", snake: "TIMES_BOLDITALIC" },
        Member { value: "Symbol", pascal: "Symbol", snake: "SYMBOL" },
        Member { value: "ZapfDingbats", pascal: "ZapfDingbats", snake: "ZAPFDINGBATS" },
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
