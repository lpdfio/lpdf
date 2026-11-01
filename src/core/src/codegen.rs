//! # codegen
//!
//! XML → SDK code generator. Walks an Lpdf XML document and emits idiomatic
//! source code for the requested target language.
//!
//! ## Supported targets
//!
//! | ID       | Language   | Style      |
//! |----------|------------|------------|
//! | `js`     | TypeScript | camelCase  |
//! | `dotnet` | C#         | PascalCase |
//! | `php`    | PHP        | camelCase  |
//! | `python` | Python     | snake_case |

use roxmltree::{Document, Node, NodeType};

use crate::attr_types::Kind;

// ── Public API ────────────────────────────────────────────────────────────────

/// Options for the code generator.
#[derive(Debug, Clone)]
pub struct CodegenOptions {
    /// Target language/SDK. Currently only `"js"` is supported.
    pub target: String,
    /// Indentation size in spaces (2 or 4). Default: 4.
    pub indent: u8,
}

impl Default for CodegenOptions {
    fn default() -> Self {
        CodegenOptions { target: "js".into(), indent: 4 }
    }
}

/// Generate SDK source code from an Lpdf XML string.
///
/// Returns the generated source as a `String`, or an error message.
pub fn codegen(xml: &str, options: &CodegenOptions) -> Result<String, String> {
    let doc = Document::parse(xml).map_err(|e| format!("XML parse error: {e}"))?;
    // A value the engine refuses has no literal of its type to be written as.
    crate::attr_types::check_xml(doc.root_element())?;

    match options.target.as_str() {
        "js" => {
            let emitter = JsEmitter { indent: options.indent };
            Ok(emitter.emit_document(&doc))
        }
        "dotnet" => {
            let emitter = DotnetEmitter { indent: options.indent };
            Ok(emitter.emit_document(&doc))
        }
        "php" => {
            let emitter = PhpEmitter { indent: options.indent };
            Ok(emitter.emit_document(&doc))
        }
        "python" => {
            let emitter = PythonEmitter { indent: options.indent };
            Ok(emitter.emit_document(&doc))
        }
        other => Err(format!("Unknown target: '{other}'. Supported: js, dotnet, php, python")),
    }
}

/// Generate SDK source code from one or more Lpdf XML element nodes (a fragment).
///
/// Unlike [`codegen`], this accepts a bare snippet — one or more lpdf elements
/// **without** the `<lpdf>` wrapper.  The output is just the node expression(s)
/// with no imports, no engine setup, and no boilerplate, making it suitable for
/// inline documentation code examples.
///
/// Multiple top-level elements are emitted one per line.
/// Returns an error if the XML is malformed or the target is unsupported.
pub fn codegen_fragment(xml: &str, options: &CodegenOptions) -> Result<String, String> {
    // Wrap in a synthetic root so the parser gets a single-root document.
    let wrapped = format!("<_f_>{xml}</_f_>");
    let doc = Document::parse(&wrapped).map_err(|e| format!("XML parse error: {e}"))?;
    let root = doc.root_element();
    crate::attr_types::check_xml(root)?;

    let elements: Vec<Node> = root.children().filter(|n| n.is_element()).collect();
    if elements.is_empty() {
        return Err("no elements found in fragment".into());
    }

    let lines: Vec<String> = match options.target.as_str() {
        "js" => {
            let emitter = JsEmitter { indent: options.indent };
            elements.iter().map(|n| emitter.emit_node(n, 0)).collect()
        }
        "dotnet" => {
            let emitter = DotnetEmitter { indent: options.indent };
            elements.iter().map(|n| emitter.emit_node(n, 0)).collect()
        }
        "php" => {
            let emitter = PhpEmitter { indent: options.indent };
            elements.iter().map(|n| emitter.emit_node(n, 0)).collect()
        }
        "python" => {
            let emitter = PythonEmitter { indent: options.indent };
            elements.iter().map(|n| emitter.emit_node(n, 0)).collect()
        }
        other => return Err(format!("Unknown target: '{other}'. Supported: js, dotnet, php, python")),
    };

    Ok(lines.join("\n"))
}

// ── Name conversion helpers ───────────────────────────────────────────────────

/// Convert a kebab-case XML attribute name to camelCase (JS/PHP style).
fn to_camel_case(s: &str) -> String {
    let mut result = String::new();
    let mut upper_next = false;
    for ch in s.chars() {
        if ch == '-' {
            upper_next = true;
        } else if upper_next {
            result.push(ch.to_ascii_uppercase());
            upper_next = false;
        } else {
            result.push(ch);
        }
    }
    result
}

/// Convert a kebab-case XML attribute name to snake_case (Python style).
fn to_snake_case(s: &str) -> String {
    s.replace('-', "_")
}

/// Convert a kebab-case XML attribute name to PascalCase (C# style). The name of a PDF standard keeps its letters
/// capitals, as the standard writes it: `pdfa` is `PdfA`, for PDF/A, and `pdfua` is `PdfUa`, for PDF/UA.
fn to_pascal_case(s: &str) -> String {
    match s {
        "pdfa" => return "PdfA".to_string(),
        "pdfua" => return "PdfUa".to_string(),
        _ => {}
    }
    s.split('-')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None    => String::new(),
                Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
            }
        })
        .collect()
}

/// Whether an element holds text and `span` children: the layout's `text` and `code` and the layer's `text-at`.
fn is_text_tag(tag: &str) -> bool {
    matches!(tag, "text" | "text-at" | "code" | "markdown-content")
}

/// The elements that stand inside the words of a text: a `span` and a `code-span`, and a `fn`, whose own words
/// are inline too.
fn is_inline_tag(tag: &str) -> bool {
    matches!(tag, "span" | "code-span" | "fn")
}

/// The rules a `<markdown-styles>` holds.
const MARKDOWN_RULES: [&str; 18] = [
    "h1", "h2", "h3", "h4", "h5", "h6", "p", "code", "quote", "li", "a", "strong", "em", "del", "table", "th", "td",
    "markdown-styles",
];

/// The element a node is. A rule of a `<markdown-styles>` shares its name with an element of the layout (`code`,
/// `li`, `td`, `table`), so it is told apart as `markdown-` and its name: `markdown-code`.
fn family_tag(node: &Node) -> String {
    let tag = node.tag_name().name();
    let in_styles = node.parent_element().is_some_and(|parent| parent.has_tag_name("markdown-styles"));
    if in_styles && MARKDOWN_RULES.contains(&tag) { format!("markdown-{tag}") } else { tag.to_string() }
}

/// The name a part of a `<markdown>` has in `LMarkdown`: `styles`, `content` or a rule such as `h1`. `None` for
/// `markdown` itself, which `L` holds, and for every other element.
fn markdown_member(family: &str) -> Option<&str> {
    match family {
        "markdown-styles"  => Some("styles"),
        "markdown-content" => Some("content"),
        other              => other.strip_prefix("markdown-"),
    }
}

/// Return the canonical JS method name for an XML element tag.
fn js_method(tag: &str) -> &'static str {
    match tag {
        "document" => "document",
        "section"  => "section",
        "layout"   => "layout",
        "layer"    => "layer",
        "tokens"   => "tokens",
        "column"   => "column",
        "row"      => "row",
        "grid"     => "grid",
        "list"     => "list",
        "li"       => "li",
        "frame"    => "frame",
        "link"     => "link",
        "bookmark" => "bookmark",
        "text"     => "text",
        "code"     => "code",
        "img"      => "img",
        "shape"    => "shape",
        "divider"  => "divider",
        "table"    => "table",
        "thead"    => "thead",
        "tr"       => "tr",
        "td"       => "td",
        "barcode"  => "barcode",
        "field"    => "field",
        "region"   => "region",
        "span"     => "span",
        "code-span" => "codeSpan",
        "fn"       => "fn",
        "rect"     => "rect",
        "circle"   => "circle",
        "ellipse"  => "ellipse",
        "line"     => "line",
        "path"     => "path",
        "text-at"    => "textAt",
        "img-at"     => "imgAt",
        "barcode-at" => "barcodeAt",
        _          => "element",
    }
}

/// What a JS or Python call for a node is called: `L.text`, and for the parts of a markdown `L.markdown` and
/// `LMarkdown.styles`.
fn js_callee(tag: &str, family: &str) -> String {
    match (tag, markdown_member(family)) {
        ("markdown", _)   => "L.markdown".to_string(),
        (_, Some(member)) => format!("LMarkdown.{member}"),
        _                 => format!("L.{}", js_method(tag)),
    }
}

/// What a C# call for a node is called.
fn dotnet_callee(tag: &str, family: &str) -> String {
    match (tag, markdown_member(family)) {
        ("markdown", _)   => "L.Markdown".to_string(),
        (_, Some(member)) => format!("LMarkdown.{}", to_pascal_case(member)),
        _                 => format!("L.{}", dotnet_method(tag)),
    }
}

/// What a PHP call for a node is called.
fn php_callee(tag: &str, family: &str) -> String {
    match (tag, markdown_member(family)) {
        ("markdown", _)   => "L::markdown".to_string(),
        (_, Some(member)) => format!("LMarkdown::{member}"),
        _                 => format!("L::{}", js_method(tag)),
    }
}

/// What a Python call for a node is called.
fn python_callee(tag: &str, family: &str) -> String {
    match (tag, markdown_member(family)) {
        ("markdown", _)   => "L.markdown".to_string(),
        (_, Some(member)) => format!("LMarkdown.{member}"),
        _                 => format!("L.{}", python_method(tag)),
    }
}

/// Return the canonical C# method name for an XML element tag.
fn dotnet_method(tag: &str) -> &'static str {
    match tag {
        "document" => "Document",
        "section"  => "Section",
        "layout"   => "Layout",
        "layer"    => "Layer",
        "tokens"   => "Tokens",
        "column"   => "Column",
        "row"      => "Row",
        "grid"     => "Grid",
        "list"     => "List",
        "li"       => "Li",
        "frame"    => "Frame",
        "link"     => "Link",
        "bookmark" => "Bookmark",
        "text"     => "Text",
        "code"     => "Code",
        "img"      => "Img",
        "shape"    => "Shape",
        "divider"  => "Divider",
        "table"    => "Table",
        "thead"    => "Thead",
        "tr"       => "Tr",
        "td"       => "Td",
        "barcode"  => "Barcode",
        "field"    => "Field",
        "region"   => "Region",
        "span"     => "Span",
        "code-span" => "CodeSpan",
        "fn"       => "Fn",
        "rect"     => "Rect",
        "circle"   => "Circle",
        "ellipse"  => "Ellipse",
        "line"     => "Line",
        "path"     => "Path",
        "text-at"    => "TextAt",
        "img-at"     => "ImgAt",
        "barcode-at" => "BarcodeAt",
        _          => "Element",
    }
}

// ── Attribute emission ────────────────────────────────────────────────────────

/// A single-quoted string literal, as JS, PHP and Python write one.
fn single_quoted(val: &str) -> String {
    format!("'{}'", val.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// A double-quoted string literal, as C# writes one. A line break or a tab in it is written as its escape.
fn double_quoted(val: &str) -> String {
    format!("\"{}\"", escape_controls(&val.replace('\\', "\\\\").replace('"', "\\\"")))
}

/// A line break, a carriage return or a tab as its escape, which JS, C# and Python read in a string.
fn escape_controls(val: &str) -> String {
    val.replace('\n', "\\n").replace('\r', "\\r").replace('\t', "\\t")
}

/// A string literal for a piece of the content of a text or a code, in the language of `target`. The content of
/// a code has lines and tabs, which a single-quoted string reads as they are only in PHP, and not in the way a
/// PHP double-quoted string reads them; so PHP gets the double-quoted one when the piece has any.
fn piece_literal(target: Target, text: &str) -> String {
    match target {
        Target::Dotnet => double_quoted(text),
        Target::Js | Target::Python => escape_controls(&single_quoted(text)),
        Target::Php if text.contains(['\n', '\r', '\t']) => {
            let escaped = text.replace('\\', "\\\\").replace('"', "\\\"").replace('$', "\\$");
            format!("\"{}\"", escape_controls(&escaped))
        }
        Target::Php => single_quoted(text),
    }
}

/// Text content as the engine reads it: runs of whitespace are one space and the ends are trimmed, so a
/// string never spans lines.
fn collapse_whitespace(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A piece of the content of a `text`, `text-at`, `code`, `span`, `code-span` or `fn`, as the generators write it.
enum TextPiece<'a, 'i> {
    /// A string, with the space that separates it from its neighbour at its edge.
    Str(String),
    /// A `span` or `code-span` element, or an `fn`.
    Span(Node<'a, 'i>),
}

/// The content of a `code`, split as the engine reads it (`parse_code` in `parse.rs`): the text between the
/// spans as it is written, except the line break after the start tag and the indent of the line the end tag
/// stands on. A string has its lines and tabs, and the code the generators write escapes them.
fn code_pieces<'a, 'i>(node: &Node<'a, 'i>) -> Vec<TextPiece<'a, 'i>> {
    let kids: Vec<Node<'a, 'i>> = node.children().filter(|c| c.is_text() || c.is_element()).collect();
    let last = kids.len().saturating_sub(1);
    let mut pieces = Vec::new();
    for (i, child) in kids.into_iter().enumerate() {
        if child.is_text() {
            let mut text = child.text().unwrap_or("");
            if i == 0    { text = crate::parse::without_first_newline(text); }
            if i == last { text = crate::parse::without_last_indent(text); }
            if !text.is_empty() { pieces.push(TextPiece::Str(text.to_string())); }
        } else if child.tag_name().name() == "span" {
            let raw: String = child.children().filter(|n| n.is_text()).filter_map(|n| n.text()).collect();
            if !raw.is_empty() { pieces.push(TextPiece::Span(child)); }
        }
    }
    pieces
}

/// The content of a `text`, `text-at`, `span` or `fn`, split as the engine reads it (`read_text_runs` in `parse.rs`).
///
/// The words of each stretch of text are one string, and a space falls between two runs where the XML has
/// whitespace there, inside a span's edge or outside it: at the end of a string before a span, at the start of
/// one after a span, or as a string of its own between two spans. The SDKs read the strings and spans of a built
/// text the same way, so the code renders the PDF of the XML. Whitespace at either end of the content is dropped.
fn text_pieces<'a, 'i>(node: &Node<'a, 'i>) -> Vec<TextPiece<'a, 'i>> {
    enum Run<'a, 'i> {
        Plain(String),
        Span(Node<'a, 'i>),
    }

    // The Markdown is one string, which keeps its lines and its indent beyond the one all its lines share.
    if node.has_tag_name("markdown-content") {
        let raw: String = node.children().filter(|n| n.is_text()).filter_map(|n| n.text()).collect();
        // The blank lines around it are the XML's layout, not the Markdown's.
        let dedented = crate::markdown::dedent(&raw);
        let text = dedented.trim_start_matches('\n').trim_end();
        return if text.is_empty() { Vec::new() } else { vec![TextPiece::Str(text.to_string())] };
    }
    // What is written in a code keeps its white space, in the code and in the spans of it.
    if node.has_tag_name("code") {
        return code_pieces(node);
    }
    if node.has_tag_name("span") && node.parent_element().is_some_and(|parent| parent.has_tag_name("code")) {
        let raw: String = node.children().filter(|n| n.is_text()).filter_map(|n| n.text()).collect();
        return if raw.is_empty() { Vec::new() } else { vec![TextPiece::Str(raw)] };
    }

    // Each run, with whether whitespace comes before it.
    let mut runs: Vec<(Run<'a, 'i>, bool)> = Vec::new();
    let mut prev_ended_space = false;
    for child in node.children() {
        match child.node_type() {
            NodeType::Text => {
                let raw = child.text().unwrap_or("");
                let words = collapse_whitespace(raw);
                if !words.is_empty() {
                    let space = raw.starts_with(char::is_whitespace) || prev_ended_space;
                    runs.push((Run::Plain(words), space));
                }
                prev_ended_space = raw.ends_with(char::is_whitespace);
            }
            NodeType::Element if matches!(child.tag_name().name(), "span" | "code-span") => {
                let raw: String = child.children().filter(|n| n.is_text()).filter_map(|n| n.text()).collect();
                if !collapse_whitespace(&raw).is_empty() {
                    runs.push((Run::Span(child), raw.starts_with(char::is_whitespace) || prev_ended_space));
                }
                prev_ended_space = raw.ends_with(char::is_whitespace);
            }
            // A footnote has words of its own, in text and spans, and no whitespace of its own reaches the text.
            NodeType::Element if child.tag_name().name() == "fn" => {
                if child.descendants().any(|n| n.is_text() && !collapse_whitespace(n.text().unwrap_or("")).is_empty()) {
                    runs.push((Run::Span(child), prev_ended_space));
                }
                prev_ended_space = false;
            }
            _ => {}
        }
    }

    let mut pieces: Vec<TextPiece<'a, 'i>> = Vec::new();
    for (i, (run, space)) in runs.into_iter().enumerate() {
        let space = space && i > 0;
        match run {
            Run::Plain(text) => pieces.push(TextPiece::Str(if space { format!(" {text}") } else { text })),
            Run::Span(span) => {
                if space {
                    match pieces.last_mut() {
                        Some(TextPiece::Str(text)) => text.push(' '),
                        _ => pieces.push(TextPiece::Str(" ".into())),
                    }
                }
                pieces.push(TextPiece::Span(span));
            }
        }
    }
    pieces
}

// ── The assets and tokens of a document ───────────────────────────────────────

/// The language a generator writes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    Js,
    Dotnet,
    Php,
    Python,
}

impl Target {
    fn literal(self, value: &str) -> String {
        match self {
            Target::Dotnet => double_quoted(value),
            _              => single_quoted(value),
        }
    }

    /// An XML attribute name as the language names the matching member of an attribute object.
    fn member_name(self, xml_name: &str) -> String {
        match self {
            Target::Js | Target::Php => to_camel_case(xml_name),
            Target::Python           => to_snake_case(xml_name),
            Target::Dotnet           => to_pascal_case(xml_name),
        }
    }

    /// One member of an attribute object: `name: value`, `name=value` or `Name = value`.
    fn member(self, xml_name: &str, value: &str) -> String {
        let name = self.member_name(xml_name);
        match self {
            Target::Js | Target::Php => format!("{name}: {value}"),
            Target::Python           => format!("{name}={value}"),
            Target::Dotnet           => format!("{name} = {value}"),
        }
    }

    /// The value of the attribute `name` of `element`, written as a literal of its type: `true` (`True` in Python) for
    /// a boolean, a number for a number, and a string for the rest, a name for `line-height` included.
    fn attribute(self, element: &Node, name: &str, value: &str) -> String {
        let parent = element.parent_element().map_or("", |p| p.tag_name().name());
        let kind = crate::attr_types::kind_of(parent, element.tag_name().name(), name);
        self.typed(kind, value)
    }

    /// `value` written as a literal of `kind`.
    fn typed(self, kind: Kind, value: &str) -> String {
        if kind == Kind::Boolean {
            return match (self, value) {
                (Target::Python, "true") => "True".to_string(),
                (Target::Python, _) => "False".to_string(),
                _ => value.to_string(),
            };
        }
        crate::attr_types::number_text(kind, value).unwrap_or_else(|| self.literal(value))
    }

    /// A map from names to values written in the language: `{ a: 'b' }`, `{'a': 'b'}`, `['a' => 'b']` or
    /// `new() { ["a"] = "b" }`.
    fn map(self, entries: &[(String, String)]) -> String {
        let items: Vec<String> = entries
            .iter()
            .map(|(key, value)| match self {
                Target::Js     => format!("{}: {value}", js_key(key)),
                Target::Python => format!("{}: {value}", single_quoted(key)),
                Target::Php    => format!("{} => {value}", single_quoted(key)),
                Target::Dotnet => format!("[{}] = {value}", double_quoted(key)),
            })
            .collect();
        let items = items.join(", ");
        match self {
            Target::Js     => format!("{{ {items} }}"),
            Target::Python => format!("{{{items}}}"),
            Target::Php    => format!("[{items}]"),
            Target::Dotnet => format!("new() {{ {items} }}"),
        }
    }

    /// A gradient or one of its stops, with members written in the language: an object in JS, a dictionary in
    /// Python, where a `from` can't be a field, and an object of `class` in PHP and C#.
    fn record(self, class: &str, members: &[(String, String)]) -> String {
        match self {
            Target::Python => self.map(members),
            _ => self.object(class, members, false),
        }
    }

    /// An object of `class` with the given members, each an XML attribute name and the value written for
    /// it. A JS object has no class. In C# a record such as `DocumentTokens` takes its members as arguments
    /// (`constructor`), where an attribute class takes them in an initializer.
    fn object(self, class: &str, members: &[(String, String)], constructor: bool) -> String {
        let written: Vec<String> = members
            .iter()
            .map(|(name, value)| match (self, constructor) {
                (Target::Dotnet, true) => format!("{}: {value}", self.member_name(name)),
                _                      => self.member(name, value),
            })
            .collect();
        let written = written.join(", ");
        match (self, constructor) {
            (Target::Js, _)           => format!("{{ {written} }}"),
            (Target::Python, _)       => format!("{class}({written})"),
            (Target::Php, _)          => format!("new {class}({written})"),
            (Target::Dotnet, true)    => format!("new {class}({written})"),
            (Target::Dotnet, false)   => format!("new {class} {{ {written} }}"),
        }
    }
}

/// The attributes of an element, each written as a literal of its type, leaving out the ones named in `skip`.
fn written_attributes(target: Target, element: &Node, skip: &[&str]) -> Vec<(String, String)> {
    element
        .attributes()
        .filter(|a| !skip.contains(&a.name()))
        .map(|a| (a.name().to_string(), target.attribute(element, a.name(), a.value())))
        .collect()
}

/// The `<gradients>` of the tokens as the map the SDKs take: each gradient's name to its attributes, and its
/// `stops` as a list. PHP and C# have a `Gradient` and a `GradientStop` class, JS and Python plain objects.
fn gradients_expression(target: Target, gradients: &Node) -> String {
    let entries: Vec<(String, String)> = gradients
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "gradient")
        .map(|gradient| {
            let mut members = written_attributes(target, &gradient, &["name"]);
            let stops: Vec<String> = gradient
                .children()
                .filter(|n| n.is_element() && n.tag_name().name() == "stop")
                .map(|stop| target.record("GradientStop", &written_attributes(target, &stop, &[])))
                .collect();
            if !stops.is_empty() {
                members.push(("stops".to_string(), format!("[{}]", stops.join(", "))));
            }
            (gradient.attribute("name").unwrap_or("").to_string(), target.record("Gradient", &members))
        })
        .collect();
    target.map(&entries)
}

/// A key of a JS object literal: bare when it is an identifier, such as `primary`, quoted when it is not,
/// such as `'surface-alt'`.
fn js_key(name: &str) -> String {
    if name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        name.to_string()
    } else {
        single_quoted(name)
    }
}

/// The `assets` of a document as the object the SDK takes: the fonts and images the `<assets>` element
/// declares, each with the attributes of its element. `None` when it declares none.
fn assets_expression(target: Target, assets: &Node) -> Option<String> {
    let mut members: Vec<(String, String)> = Vec::new();
    for (list, tag, class) in [("fonts", "font", "FontAttr"), ("images", "image", "ImageAttr")] {
        let declared: Vec<String> = assets
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == tag)
            .map(|asset| {
                target.object(class, &written_attributes(target, &asset, &[]), false)
            })
            .collect();
        if !declared.is_empty() {
            members.push((list.to_string(), format!("[{}]", declared.join(", "))));
        }
    }
    (!members.is_empty()).then(|| target.object("DocumentAssets", &members, true))
}

/// Whether any font or image of an `<assets>` element has a `src` the SDK has to read.
fn assets_have_src(assets: &Node) -> bool {
    assets
        .children()
        .any(|n| n.is_element() && n.has_attribute("src"))
}

/// The `<file>` elements of a document's `<attachments>`.
fn attached_files<'a, 'i>(document: &Node<'a, 'i>) -> Vec<Node<'a, 'i>> {
    document
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "attachments")
        .flat_map(|attachments| attachments.children())
        .filter(|n| n.is_element() && n.tag_name().name() == "file")
        .collect()
}

/// The `attachments` of a document as the list the SDK takes: a `FileAttr` for each `<file>` of its `<attachments>`,
/// with the attributes of the element. `None` when it carries no file.
fn attachments_expression(target: Target, document: &Node) -> Option<String> {
    let files: Vec<String> = attached_files(document)
        .iter()
        .map(|file| target.object("FileAttr", &written_attributes(target, file, &[]), false))
        .collect();
    (!files.is_empty()).then(|| format!("[{}]", files.join(", ")))
}

/// The `tokens` of a document as the object the SDK takes: the colours, the aliases and the scales of the
/// `<tokens>` element. `None` when it has none.
fn tokens_expression(target: Target, tokens: &Node) -> Option<String> {
    let mut members: Vec<(String, String)> = Vec::new();
    for child in tokens.children().filter(|n| n.is_element()) {
        let tag = child.tag_name().name();
        if tag == "gradients" {
            if child.children().any(|n| n.is_element()) {
                members.push((tag.to_string(), gradients_expression(target, &child)));
            }
            continue;
        }
        // The colours and the aliases are lists of `name` and `value`; the SDK takes each as a map.
        let named = match tag {
            "colors"  => Some("color"),
            "aliases" => Some("alias"),
            _         => None,
        };
        let entries: Vec<(String, String)> = if let Some(entry) = named {
            child
                .children()
                .filter(|n| n.is_element() && n.tag_name().name() == entry)
                .map(|c| {
                    (
                        c.attribute("name").unwrap_or("").to_string(),
                        target.literal(c.attribute("value").unwrap_or("")),
                    )
                })
                .collect()
        } else {
            // A row of slots, each a length, or a factor in the line-height row.
            written_attributes(target, &child, &[])
        };
        if !entries.is_empty() {
            members.push((tag.to_string(), target.map(&entries)));
        }
    }
    (!members.is_empty()).then(|| target.object("DocumentTokens", &members, true))
}

/// The SDK classes that generated code uses, once each and in the order they first appear: the attribute
/// classes and the document's meta, output, assets and tokens.
fn classes_used(code: &str) -> Vec<String> {
    let mut used: Vec<String> = Vec::new();
    for word in code.split(|c: char| !c.is_ascii_alphanumeric()) {
        let is_class = word.starts_with(|c: char| c.is_ascii_uppercase())
            && word != "NoAttr"
            && (word.ends_with("Attr")
                || matches!(word, "DocumentMeta" | "DocumentOutput" | "DocumentAssets" | "DocumentTokens" | "Gradient" | "GradientStop"));
        if is_class && !used.iter().any(|u| u == word) {
            used.push(word.to_string());
        }
    }
    used
}

/// The PHP namespace of an SDK class.
fn php_namespace(class: &str) -> &'static str {
    match class {
        "DocumentAttr" | "DocumentMeta" | "DocumentOutput" | "DocumentAssets" | "DocumentTokens" | "SectionAttr" | "LayoutAttr"
        | "FontAttr" | "ImageAttr" | "FileAttr" | "Gradient" | "GradientStop" => "Lpdf\\Kit",
        "LayerAttr" | "RectAttr" | "CircleAttr" | "EllipseAttr" | "LineAttr" | "PathAttr" | "TextAtAttr"
        | "ImgAtAttr" | "BarcodeAtAttr" => "Lpdf\\Canvas",
        markdown if markdown.starts_with("Markdown") => "Lpdf\\Markdown",
        _ => "Lpdf\\Layout",
    }
}


/// The elements of attributes alone that a document folds into its own attributes, `<meta>` then `<output>`: each
/// one that has attributes, written by `write` from its name and its attributes.
fn folded_elements(document: &Node, write: impl Fn(&str, &[(&str, &str)]) -> String) -> Vec<String> {
    ["meta", "output"]
        .into_iter()
        .filter_map(|name| {
            let element = document.children().find(|n| n.is_element() && n.tag_name().name() == name)?;
            let attributes: Vec<(&str, &str)> = element.attributes().map(|a| (a.name(), a.value())).collect();
            (!attributes.is_empty()).then(|| write(name, &attributes))
        })
        .collect()
}

/// Emit JS object literal for the attributes of a node.
///
/// `extra_attrs` are additional key/value pairs injected by the caller
/// (used for folding `<meta>` into `document`).
///
/// Returns `"NoAttr"` when there are no attributes at all.
fn js_attrs(node: &Node, extra_attrs: Option<&[(&str, String)]>) -> String {
    let mut parts: Vec<String> = node
        .attributes()
        .filter(|a| !is_data_binding(a.name()))
        .map(|a| format!("{}: {}", to_camel_case(a.name()), Target::Js.attribute(node, a.name(), a.value())))
        .collect();

    if let Some(extras) = extra_attrs {
        for (k, v) in extras {
            parts.push(format!("{k}: {v}"));
        }
    }

    if parts.is_empty() {
        "NoAttr".into()
    } else {
        format!("{{ {} }}", parts.join(", "))
    }
}

/// Whether an attribute reads the data (`data-value`, `data-source`, `data-if`, `data-if-not`, `data-href`,
/// `data-color`…). The builders have no such attributes, so codegen leaves them out of the code it writes and
/// notes them in a TODO comment.
fn is_data_binding(name: &str) -> bool {
    name.starts_with("data-")
}

/// The TODO comment for one data-binding attribute, after `marker` (`//` or `#`).
fn data_binding_comment(marker: &str, indent_str: &str, attr: &roxmltree::Attribute) -> String {
    let note = if attr.name() == "data-source" { " — loop" } else { "" };
    format!("{indent_str}{marker} TODO (Lpdf) {}: {}{note}\n", attr.name(), attr.value())
}

/// Collect data-binding attributes for TODO comment generation (// style).
fn data_binding_comments(node: &Node, indent_str: &str) -> String {
    node.attributes()
        .filter(|a| is_data_binding(a.name()))
        .map(|a| data_binding_comment("//", indent_str, &a))
        .collect()
}

/// Collect data-binding attributes for TODO comment generation (# style, Python).
fn data_binding_comments_hash(node: &Node, indent_str: &str) -> String {
    node.attributes()
        .filter(|a| is_data_binding(a.name()))
        .map(|a| data_binding_comment("#", indent_str, &a))
        .collect()
}

/// Emit a PHP named-arg constructor call for the attributes of a node.
///
/// Returns `"NoAttr"` when there are no attributes.
fn php_attrs(node: &Node, tag: &str, extra_attrs: Option<&[(&str, String)]>) -> String {
    let mut parts: Vec<String> = node
        .attributes()
        .filter(|a| !is_data_binding(a.name()))
        .map(|a| format!("{}: {}", to_camel_case(a.name()), Target::Php.attribute(node, a.name(), a.value())))
        .collect();

    if let Some(extras) = extra_attrs {
        for (k, v) in extras {
            parts.push(format!("{k}: {v}"));
        }
    }

    if parts.is_empty() {
        "NoAttr".into()
    } else {
        let class = attr_class(tag);
        format!("new {class}({})", parts.join(", "))
    }
}

/// The attribute class of an element: `{Element}Attr`, as the schema's element names give them (`TextAtAttr`
/// for `text-at`).
fn attr_class(tag: &str) -> String {
    format!("{}Attr", to_pascal_case(tag))
}

/// Emit a Python keyword-arg constructor for the attributes of a node.
///
/// Returns `"NoAttr"` when there are no attributes.
fn python_attrs(node: &Node, tag: &str, extra_attrs: Option<&[(&str, String)]>) -> String {
    let mut parts: Vec<String> = node
        .attributes()
        .filter(|a| !is_data_binding(a.name()))
        .map(|a| format!("{}={}", to_snake_case(a.name()), Target::Python.attribute(node, a.name(), a.value())))
        .collect();

    if let Some(extras) = extra_attrs {
        for (k, v) in extras {
            parts.push(format!("{k}={v}"));
        }
    }

    if parts.is_empty() {
        "NoAttr".into()
    } else {
        let class = attr_class(tag);
        format!("{class}({})", parts.join(", "))
    }
}

/// Emit a C# `new() { ... }` initializer for the attributes of a node.
///
/// Returns `"NoAttr"` when there are no attributes.
fn dotnet_attrs(node: &Node, extra_attrs: Option<&[(&str, String)]>) -> String {
    let mut parts: Vec<String> = node
        .attributes()
        .filter(|a| !is_data_binding(a.name()))
        .map(|a| format!("{} = {}", to_pascal_case(a.name()), Target::Dotnet.attribute(node, a.name(), a.value())))
        .collect();

    if let Some(extras) = extra_attrs {
        for (k, v) in extras {
            parts.push(format!("{k} = {v}"));
        }
    }

    if parts.is_empty() {
        "NoAttr".into()
    } else {
        format!("new() {{ {} }}", parts.join(", "))
    }
}

// ── JS emitter ────────────────────────────────────────────────────────────────

struct JsEmitter {
    indent: u8,
}

impl JsEmitter {
    fn ind(&self, level: usize) -> String {
        " ".repeat(self.indent as usize * level)
    }

    fn emit_document(&self, doc: &Document) -> String {
        let root = doc.root_element(); // <lpdf>

        let mut assets_node:   Option<Node> = None;
        let mut tokens_node:   Option<Node> = None;
        let mut document_node: Option<Node> = None;

        for child in root.children().filter(|n| n.is_element()) {
            match child.tag_name().name() {
                "assets"   => assets_node   = Some(child),
                "tokens"   => tokens_node   = Some(child),
                "document" => document_node = Some(child),
                _ => {}
            }
        }

        // The assets and tokens are attributes of the document. The SDK reads the fonts and images that
        // have a src from there.
        let doc_expr = document_node.map(|doc_node| self.emit_document_node(&doc_node, 0, assets_node, tokens_node));

        let mut out = String::new();

        let markdown = if doc_expr.as_deref().is_some_and(|code| code.contains("LMarkdown.")) { ", LMarkdown" } else { "" };
        out.push_str(&format!("import {{ L{markdown}, NoAttr }} from '@lpdfio/lpdf'\n"));
        out.push('\n');

        out.push_str("const engine = L.engine()\n");
        out.push('\n');

        if let Some(doc_expr) = doc_expr {
            out.push_str(&format!("const doc = {doc_expr}\n"));
        }

        out.push('\n');
        out.push_str("const pdf = await engine.render(doc)\n");

        out
    }

    /// The document an `<lpdf>` element holds, with its assets and tokens as attributes of the document.
    fn emit_lpdf(&self, lpdf: &Node, level: usize) -> String {
        let child = |tag: &str| lpdf.children().find(|n| n.is_element() && n.tag_name().name() == tag);
        match child("document") {
            Some(document) => self.emit_document_node(&document, level, child("assets"), child("tokens")),
            None => String::new(),
        }
    }

    fn emit_node(&self, node: &Node, level: usize) -> String {
        let tag = node.tag_name().name();

        // Special cases
        match tag {
            "lpdf"     => return self.emit_lpdf(node, level),
            "document" => return self.emit_document_node(node, level, None, None),
            "meta" | "output" | "attachments" => return String::new(), // folded into document
            _ => {}
        }
        let family    = family_tag(node);
        let method    = js_callee(tag, &family);
        let ind0      = self.ind(level);
        let ind1      = self.ind(level + 1);

        // Data-binding TODO comments
        let binding_comments = data_binding_comments(node, &ind0);

        // Build attrs
        let data_value = node.attribute("data-value");
        let attrs = js_attrs(node, None);

        // Special: <text> with data-value override
        let text_content_override: Option<String> = data_value.map(|p| format!("{{{p}}}"));

        // Determine children
        let children = self.collect_children(node, tag, level, text_content_override.as_deref());

        let call = if children.is_empty() {
            // Leaf — no children arg
            format!("{binding_comments}{ind0}{method}({attrs})")
        } else if is_text_tag(tag) {
            format!("{binding_comments}{}", self.emit_text_call(&ind0, &ind1, &method, &attrs, &children))
        } else if is_inline_tag(tag) {
            // span always inline: L.span({...}, ['content'])
            format!("{binding_comments}{ind0}{method}({attrs}, [{}])", children.join(", "))
        } else {
            format!("{binding_comments}{}", self.emit_block_call(&ind0, &ind1, &method, &attrs, &children))
        };

        call
    }

    fn emit_document_node(&self, node: &Node, level: usize, assets: Option<Node>, tokens: Option<Node>) -> String {
        let ind0 = self.ind(level);
        let ind1 = self.ind(level + 1);
        let method = "L.document";

        // Fold the attributes of <meta> and <output> in, each as an object under its name
        let folded = folded_elements(node, |element, attributes| {
            let parts: Vec<String> = attributes
                .iter()
                .map(|(name, value)| format!("{}: '{}'", to_camel_case(name), value.replace('\'', "\\'")))
                .collect();
            format!("{element}: {{ {} }}", parts.join(", "))
        });

        // Build document attrs with the folded meta and output
        let mut doc_parts: Vec<String> = node
            .attributes()
            .map(|a| format!("{}: {}", to_camel_case(a.name()), Target::Js.attribute(node, a.name(), a.value())))
            .collect();

        if let Some(assets) = assets.and_then(|a| assets_expression(Target::Js, &a)) {
            doc_parts.push(Target::Js.member("assets", &assets));
        }
        if let Some(tokens) = tokens.and_then(|t| tokens_expression(Target::Js, &t)) {
            doc_parts.push(Target::Js.member("tokens", &tokens));
        }
        doc_parts.extend(folded);
        if let Some(files) = attachments_expression(Target::Js, node) {
            doc_parts.push(Target::Js.member("attachments", &files));
        }

        let attrs = if doc_parts.is_empty() {
            "NoAttr".into()
        } else {
            format!("{{ {} }}", doc_parts.join(", "))
        };

        // Emit children (skip meta and output)
        let mut children: Vec<String> = Vec::new();
        children.extend(
            node.children()
                .filter(|n| n.is_element() && !matches!(n.tag_name().name(), "meta" | "output" | "attachments"))
                .map(|n| self.emit_node(&n, level + 1))
                .filter(|s| !s.is_empty()),
        );

        self.emit_block_call(&ind0, &ind1, &method, &attrs, &children)
    }

    /// Collect children of a node as emitted strings.
    fn collect_children(
        &self,
        node: &Node,
        tag: &str,
        level: usize,
        text_content_override: Option<&str>,
    ) -> Vec<String> {
        let mut children: Vec<String> = Vec::new();

        if is_text_tag(tag) || is_inline_tag(tag) {
            // Mixed text + <span> children
            let override_text = text_content_override;

            if let Some(placeholder) = override_text {
                // data-value replaces all content
                children.push(format!("'{placeholder}'"));
            } else {
                for piece in text_pieces(node) {
                    match piece {
                        TextPiece::Str(text) => children.push(piece_literal(Target::Js, &text)),
                        // Emit span at level 0 — parent handles indentation
                        TextPiece::Span(span) => children.push(self.emit_node(&span, 0)),
                    }
                }
            }
        } else {
            for child in node.children().filter(|n| n.is_element()) {
                let s = self.emit_node(&child, level + 1);
                if !s.is_empty() {
                    children.push(s);
                }
            }
        }

        children
    }

    fn emit_text_call(
        &self,
        ind0: &str,
        ind1: &str,
        method: &str,
        attrs: &str,
        children: &[String],
    ) -> String {
        if children.len() == 1 {
            // Inline form
            format!("{ind0}{method}({attrs}, [{}])", children[0])
        } else {
            // Multi-line form — children are plain values (strings/inline spans), add indent
            let items: Vec<String> = children.iter().map(|c| format!("{ind1}{c},")).collect();
            format!("{ind0}{method}({attrs}, [\n{}\n{ind0}])", items.join("\n"))
        }
    }

    /// Emit a generic block element with children on separate lines.
    fn emit_block_call(
        &self,
        ind0: &str,
        _ind1: &str,
        method: &str,
        attrs: &str,
        children: &[String],
    ) -> String {
        if children.is_empty() {
            return format!("{ind0}{method}({attrs})");
        }
        // Children already carry their own leading indentation from emit_node.
        let items: Vec<String> = children.iter().map(|c| format!("{c},")).collect();
        format!("{ind0}{method}({attrs}, [\n{}\n{ind0}])", items.join("\n"))
    }
}

// ── C# emitter ────────────────────────────────────────────────────────────────

struct DotnetEmitter {
    indent: u8,
}

impl DotnetEmitter {
    fn ind(&self, level: usize) -> String {
        " ".repeat(self.indent as usize * level)
    }

    fn emit_document(&self, doc: &Document) -> String {
        let root = doc.root_element(); // <lpdf>

        let mut assets_node:   Option<Node> = None;
        let mut tokens_node:   Option<Node> = None;
        let mut document_node: Option<Node> = None;

        for child in root.children().filter(|n| n.is_element()) {
            match child.tag_name().name() {
                "assets"   => assets_node   = Some(child),
                "tokens"   => tokens_node   = Some(child),
                "document" => document_node = Some(child),
                _ => {}
            }
        }

        let body = document_node
            .map(|doc_node| {
                let doc_expr = self.emit_document_node(&doc_node, 0, assets_node, tokens_node);
                format!("var doc = {doc_expr};\n")
            })
            .unwrap_or_default();

        let mut out = String::new();

        out.push_str("using Lpdf;\n");
        // C# writes the attribute classes as `new()`; the ones named are those of the assets and tokens.
        if !classes_used(&body).is_empty() {
            out.push_str("using Lpdf.Kit;\n");
        }
        // `NoAttr` is a static member of `L`, written bare as the other targets write it.
        out.push_str("using static Lpdf.L;\n");
        out.push('\n');

        // .NET reads the src of a font, an image or a file through the engine's SrcFallback.
        let files_have_src = document_node.is_some_and(|d| attached_files(&d).iter().any(|f| f.has_attribute("src")));
        if assets_node.is_some_and(|assets| assets_have_src(&assets)) || files_have_src {
            out.push_str("var engine = L.Engine(new() { SrcFallback = File.ReadAllBytes });\n");
        } else {
            out.push_str("var engine = L.Engine();\n");
        }
        out.push('\n');

        out.push_str(&body);
        out.push('\n');
        out.push_str("var pdf = engine.Render(doc);\n");

        out
    }

    /// The document an `<lpdf>` element holds, with its assets and tokens as attributes of the document.
    fn emit_lpdf(&self, lpdf: &Node, level: usize) -> String {
        let child = |tag: &str| lpdf.children().find(|n| n.is_element() && n.tag_name().name() == tag);
        match child("document") {
            Some(document) => self.emit_document_node(&document, level, child("assets"), child("tokens")),
            None => String::new(),
        }
    }

    fn emit_node(&self, node: &Node, level: usize) -> String {
        let tag = node.tag_name().name();

        match tag {
            "lpdf"     => return self.emit_lpdf(node, level),
            "document" => return self.emit_document_node(node, level, None, None),
            "meta" | "output" | "attachments" => return String::new(),
            _ => {}
        }
        let family    = family_tag(node);
        let method    = dotnet_callee(tag, &family);
        let ind0      = self.ind(level);
        let ind1      = self.ind(level + 1);

        let binding_comments = data_binding_comments(node, &ind0);
        let data_value = node.attribute("data-value");
        let attrs = dotnet_attrs(node, None);
        let text_content_override: Option<String> = data_value.map(|p| format!("{{{p}}}"));

        let children = self.collect_children(node, tag, level, text_content_override.as_deref());

        if children.is_empty() {
            format!("{binding_comments}{ind0}{method}({attrs})")
        } else if is_text_tag(tag) {
            format!("{binding_comments}{}", self.emit_text_call(&ind0, &ind1, &method, &attrs, &children))
        } else if is_inline_tag(tag) {
            format!("{binding_comments}{ind0}{method}({attrs}, [{}])", children.join(", "))
        } else {
            format!("{binding_comments}{}", self.emit_block_call(&ind0, &method, &attrs, &children))
        }
    }

    fn emit_document_node(&self, node: &Node, level: usize, assets: Option<Node>, tokens: Option<Node>) -> String {
        let ind0   = self.ind(level);
        let method = "L.Document";

        let folded = folded_elements(node, |element, attributes| {
            let parts: Vec<String> = attributes
                .iter()
                .map(|(name, value)| format!("{} = \"{}\"", to_pascal_case(name), value.replace('"', "\\\"")))
                .collect();
            format!("{} = new() {{ {} }}", to_pascal_case(element), parts.join(", "))
        });

        let mut doc_parts: Vec<String> = node
            .attributes()
            .map(|a| format!("{} = {}", to_pascal_case(a.name()), Target::Dotnet.attribute(node, a.name(), a.value())))
            .collect();

        if let Some(assets) = assets.and_then(|a| assets_expression(Target::Dotnet, &a)) {
            doc_parts.push(Target::Dotnet.member("assets", &assets));
        }
        if let Some(tokens) = tokens.and_then(|t| tokens_expression(Target::Dotnet, &t)) {
            doc_parts.push(Target::Dotnet.member("tokens", &tokens));
        }
        doc_parts.extend(folded);
        if let Some(files) = attachments_expression(Target::Dotnet, node) {
            doc_parts.push(Target::Dotnet.member("attachments", &files));
        }

        let attrs = if doc_parts.is_empty() {
            "NoAttr".into()
        } else {
            format!("new() {{ {} }}", doc_parts.join(", "))
        };

        let mut children: Vec<String> = Vec::new();
        children.extend(
            node.children()
                .filter(|n| n.is_element() && !matches!(n.tag_name().name(), "meta" | "output" | "attachments"))
                .map(|n| self.emit_node(&n, level + 1))
                .filter(|s| !s.is_empty()),
        );

        self.emit_block_call(&ind0, &method, &attrs, &children)
    }

    fn collect_children(
        &self,
        node: &Node,
        tag: &str,
        level: usize,
        text_content_override: Option<&str>,
    ) -> Vec<String> {
        let mut children: Vec<String> = Vec::new();

        if is_text_tag(tag) || is_inline_tag(tag) {
            if let Some(placeholder) = text_content_override {
                children.push(format!("\"{placeholder}\""));
            } else {
                for piece in text_pieces(node) {
                    match piece {
                        TextPiece::Str(text) => children.push(double_quoted(&text)),
                        // Emit span at level 0 — parent handles indentation
                        TextPiece::Span(span) => children.push(self.emit_node(&span, 0)),
                    }
                }
            }
        } else {
            for child in node.children().filter(|n| n.is_element()) {
                let s = self.emit_node(&child, level + 1);
                if !s.is_empty() {
                    children.push(s);
                }
            }
        }

        children
    }

    fn emit_text_call(
        &self,
        ind0: &str,
        ind1: &str,
        method: &str,
        attrs: &str,
        children: &[String],
    ) -> String {
        if children.len() == 1 {
            format!("{ind0}{method}({attrs}, [{}])", children[0])
        } else {
            let items: Vec<String> = children.iter().map(|c| format!("{ind1}{c},")).collect();
            format!("{ind0}{method}({attrs}, [\n{}\n{ind0}])", items.join("\n"))
        }
    }

    fn emit_block_call(
        &self,
        ind0: &str,
        method: &str,
        attrs: &str,
        children: &[String],
    ) -> String {
        if children.is_empty() {
            return format!("{ind0}{method}({attrs})");
        }
        let items: Vec<String> = children.iter().map(|c| format!("{c},")).collect();
        format!("{ind0}{method}({attrs}, [\n{}\n{ind0}])", items.join("\n"))
    }
}

// ── PHP emitter ───────────────────────────────────────────────────────────────

struct PhpEmitter {
    indent: u8,
}

impl PhpEmitter {
    fn ind(&self, level: usize) -> String {
        " ".repeat(self.indent as usize * level)
    }

    fn emit_document(&self, doc: &Document) -> String {
        let root = doc.root_element();

        let mut assets_node:   Option<Node> = None;
        let mut tokens_node:   Option<Node> = None;
        let mut document_node: Option<Node> = None;

        for child in root.children().filter(|n| n.is_element()) {
            match child.tag_name().name() {
                "assets"   => assets_node   = Some(child),
                "tokens"   => tokens_node   = Some(child),
                "document" => document_node = Some(child),
                _ => {}
            }
        }

        let body = document_node
            .map(|doc_node| {
                let doc_expr = self.emit_document_node(&doc_node, 0, assets_node, tokens_node);
                format!("$doc = {doc_expr};\n")
            })
            .unwrap_or_default();

        let mut out = String::new();

        out.push_str("<?php\n\n");
        out.push_str("require_once 'vendor/autoload.php';\n\n");
        out.push_str("use Lpdf\\L;\n");
        if body.contains("LMarkdown::") {
            out.push_str("use Lpdf\\LMarkdown;\n");
        }
        out.push_str("use const Lpdf\\NoAttr;\n");
        let mut classes: Vec<String> = classes_used(&body)
            .iter()
            .map(|class| format!("{}\\{class}", php_namespace(class)))
            .collect();
        classes.sort();
        for class in classes {
            out.push_str(&format!("use {class};\n"));
        }
        out.push('\n');

        out.push_str("$engine = L::engine();\n");
        out.push('\n');

        out.push_str(&body);
        out.push('\n');
        out.push_str("$pdf = $engine->render($doc);\n");

        out
    }

    /// The document an `<lpdf>` element holds, with its assets and tokens as attributes of the document.
    fn emit_lpdf(&self, lpdf: &Node, level: usize) -> String {
        let child = |tag: &str| lpdf.children().find(|n| n.is_element() && n.tag_name().name() == tag);
        match child("document") {
            Some(document) => self.emit_document_node(&document, level, child("assets"), child("tokens")),
            None => String::new(),
        }
    }

    fn emit_node(&self, node: &Node, level: usize) -> String {
        let tag = node.tag_name().name();

        match tag {
            "lpdf"     => return self.emit_lpdf(node, level),
            "document" => return self.emit_document_node(node, level, None, None),
            "meta" | "output" | "attachments" => return String::new(),
            _ => {}
        }
        // PHP uses same method names as JS (camelCase)
        let family    = family_tag(node);
        let method    = php_callee(tag, &family);
        let ind0      = self.ind(level);
        let ind1      = self.ind(level + 1);

        let binding_comments = data_binding_comments(node, &ind0);
        let data_value = node.attribute("data-value");
        let attrs = php_attrs(node, &family, None);
        let text_content_override: Option<String> = data_value.map(|p| format!("{{{p}}}"));

        let children = self.collect_children(node, tag, level, text_content_override.as_deref());

        if children.is_empty() {
            format!("{binding_comments}{ind0}{method}({attrs})")
        } else if is_text_tag(tag) {
            format!("{binding_comments}{}", self.emit_text_call(&ind0, &ind1, &method, &attrs, &children))
        } else if is_inline_tag(tag) {
            format!("{binding_comments}{ind0}{method}({attrs}, [{}])", children.join(", "))
        } else {
            format!("{binding_comments}{}", self.emit_block_call(&ind0, &method, &attrs, &children))
        }
    }

    fn emit_document_node(&self, node: &Node, level: usize, assets: Option<Node>, tokens: Option<Node>) -> String {
        let ind0   = self.ind(level);
        let method = "L::document";

        let folded = folded_elements(node, |element, attributes| {
            let parts: Vec<String> = attributes
                .iter()
                .map(|(name, value)| format!("{}: '{}'", to_camel_case(name), value.replace('\'', "\\'")))
                .collect();
            format!("{element}: new Document{}({})", to_pascal_case(element), parts.join(", "))
        });

        let mut doc_parts: Vec<String> = node
            .attributes()
            .map(|a| format!("{}: {}", to_camel_case(a.name()), Target::Php.attribute(node, a.name(), a.value())))
            .collect();

        if let Some(assets) = assets.and_then(|a| assets_expression(Target::Php, &a)) {
            doc_parts.push(Target::Php.member("assets", &assets));
        }
        if let Some(tokens) = tokens.and_then(|t| tokens_expression(Target::Php, &t)) {
            doc_parts.push(Target::Php.member("tokens", &tokens));
        }
        doc_parts.extend(folded);
        if let Some(files) = attachments_expression(Target::Php, node) {
            doc_parts.push(Target::Php.member("attachments", &files));
        }

        let attrs = if doc_parts.is_empty() {
            "NoAttr".into()
        } else {
            format!("new DocumentAttr({})", doc_parts.join(", "))
        };

        let mut children: Vec<String> = Vec::new();
        children.extend(
            node.children()
                .filter(|n| n.is_element() && !matches!(n.tag_name().name(), "meta" | "output" | "attachments"))
                .map(|n| self.emit_node(&n, level + 1))
                .filter(|s| !s.is_empty()),
        );

        self.emit_block_call(&ind0, &method, &attrs, &children)
    }

    fn collect_children(
        &self,
        node: &Node,
        tag: &str,
        level: usize,
        text_content_override: Option<&str>,
    ) -> Vec<String> {
        let mut children: Vec<String> = Vec::new();

        if is_text_tag(tag) || is_inline_tag(tag) {
            if let Some(placeholder) = text_content_override {
                children.push(format!("'{placeholder}'"));
            } else {
                for piece in text_pieces(node) {
                    match piece {
                        TextPiece::Str(text) => children.push(piece_literal(Target::Php, &text)),
                        // Emit span at level 0 — parent handles indentation
                        TextPiece::Span(span) => children.push(self.emit_node(&span, 0)),
                    }
                }
            }
        } else {
            for child in node.children().filter(|n| n.is_element()) {
                let s = self.emit_node(&child, level + 1);
                if !s.is_empty() {
                    children.push(s);
                }
            }
        }

        children
    }

    fn emit_text_call(
        &self,
        ind0: &str,
        ind1: &str,
        method: &str,
        attrs: &str,
        children: &[String],
    ) -> String {
        if children.len() == 1 {
            format!("{ind0}{method}({attrs}, [{}])", children[0])
        } else {
            let items: Vec<String> = children.iter().map(|c| format!("{ind1}{c},")).collect();
            format!("{ind0}{method}({attrs}, [\n{}\n{ind0}])", items.join("\n"))
        }
    }

    fn emit_block_call(
        &self,
        ind0: &str,
        method: &str,
        attrs: &str,
        children: &[String],
    ) -> String {
        if children.is_empty() {
            return format!("{ind0}{method}({attrs})");
        }
        let items: Vec<String> = children.iter().map(|c| format!("{c},")).collect();
        format!("{ind0}{method}({attrs}, [\n{}\n{ind0}])", items.join("\n"))
    }
}

// ── Python emitter ────────────────────────────────────────────────────────────

struct PythonEmitter {
    indent: u8,
}

impl PythonEmitter {
    fn ind(&self, level: usize) -> String {
        " ".repeat(self.indent as usize * level)
    }

    fn emit_document(&self, doc: &Document) -> String {
        let root = doc.root_element();

        let mut assets_node:   Option<Node> = None;
        let mut tokens_node:   Option<Node> = None;
        let mut document_node: Option<Node> = None;

        for child in root.children().filter(|n| n.is_element()) {
            match child.tag_name().name() {
                "assets"   => assets_node   = Some(child),
                "tokens"   => tokens_node   = Some(child),
                "document" => document_node = Some(child),
                _ => {}
            }
        }

        let body = document_node
            .map(|doc_node| {
                let doc_expr = self.emit_document_node(&doc_node, 0, assets_node, tokens_node);
                format!("doc = {doc_expr}\n")
            })
            .unwrap_or_default();

        let mut out = String::new();

        let mut imports = vec!["L".to_string(), "NoAttr".to_string()];
        if body.contains("LMarkdown.") {
            imports.insert(1, "LMarkdown".to_string());
        }
        let mut classes = classes_used(&body);
        classes.sort();
        imports.extend(classes);
        out.push_str(&format!("from lpdf import {}\n", imports.join(", ")));
        out.push('\n');

        out.push_str("engine = L.engine()\n");
        out.push('\n');

        out.push_str(&body);
        out.push('\n');
        out.push_str("pdf = engine.render(doc)\n");

        out
    }

    /// The document an `<lpdf>` element holds, with its assets and tokens as attributes of the document.
    fn emit_lpdf(&self, lpdf: &Node, level: usize) -> String {
        let child = |tag: &str| lpdf.children().find(|n| n.is_element() && n.tag_name().name() == tag);
        match child("document") {
            Some(document) => self.emit_document_node(&document, level, child("assets"), child("tokens")),
            None => String::new(),
        }
    }

    fn emit_node(&self, node: &Node, level: usize) -> String {
        let tag = node.tag_name().name();

        match tag {
            "lpdf"     => return self.emit_lpdf(node, level),
            "document" => return self.emit_document_node(node, level, None, None),
            "meta" | "output" | "attachments" => return String::new(),
            _ => {}
        }
        // Python uses same method names as JS (snake_case only for multi-word: textAt → text_at)
        let family    = family_tag(node);
        let method    = python_callee(tag, &family);
        let ind0      = self.ind(level);
        let ind1      = self.ind(level + 1);

        let binding_comments = data_binding_comments_hash(node, &ind0);
        let data_value = node.attribute("data-value");
        let attrs = python_attrs(node, &family, None);
        let text_content_override: Option<String> = data_value.map(|p| format!("{{{p}}}"));

        let children = self.collect_children(node, tag, level, text_content_override.as_deref());

        if children.is_empty() {
            format!("{binding_comments}{ind0}{method}({attrs})")
        } else if is_text_tag(tag) {
            format!("{binding_comments}{}", self.emit_text_call(&ind0, &ind1, &method, &attrs, &children))
        } else if is_inline_tag(tag) {
            format!("{binding_comments}{ind0}{method}({attrs}, [{}])", children.join(", "))
        } else {
            format!("{binding_comments}{}", self.emit_block_call(&ind0, &method, &attrs, &children))
        }
    }

    fn emit_document_node(&self, node: &Node, level: usize, assets: Option<Node>, tokens: Option<Node>) -> String {
        let ind0   = self.ind(level);
        let method = "L.document";

        let folded = folded_elements(node, |element, attributes| {
            let parts: Vec<String> = attributes
                .iter()
                .map(|(name, value)| format!("{}='{}'", to_snake_case(name), value.replace('\'', "\\'")))
                .collect();
            format!("{element}=Document{}({})", to_pascal_case(element), parts.join(", "))
        });

        let mut doc_parts: Vec<String> = node
            .attributes()
            .map(|a| format!("{}={}", to_snake_case(a.name()), Target::Python.attribute(node, a.name(), a.value())))
            .collect();

        if let Some(assets) = assets.and_then(|a| assets_expression(Target::Python, &a)) {
            doc_parts.push(Target::Python.member("assets", &assets));
        }
        if let Some(tokens) = tokens.and_then(|t| tokens_expression(Target::Python, &t)) {
            doc_parts.push(Target::Python.member("tokens", &tokens));
        }
        doc_parts.extend(folded);
        if let Some(files) = attachments_expression(Target::Python, node) {
            doc_parts.push(Target::Python.member("attachments", &files));
        }

        let attrs = if doc_parts.is_empty() {
            "NoAttr".into()
        } else {
            format!("DocumentAttr({})", doc_parts.join(", "))
        };

        let mut children: Vec<String> = Vec::new();
        children.extend(
            node.children()
                .filter(|n| n.is_element() && !matches!(n.tag_name().name(), "meta" | "output" | "attachments"))
                .map(|n| self.emit_node(&n, level + 1))
                .filter(|s| !s.is_empty()),
        );

        self.emit_block_call(&ind0, &method, &attrs, &children)
    }

    fn collect_children(
        &self,
        node: &Node,
        tag: &str,
        level: usize,
        text_content_override: Option<&str>,
    ) -> Vec<String> {
        let mut children: Vec<String> = Vec::new();

        if is_text_tag(tag) || is_inline_tag(tag) {
            if let Some(placeholder) = text_content_override {
                children.push(format!("'{placeholder}'"));
            } else {
                for piece in text_pieces(node) {
                    match piece {
                        TextPiece::Str(text) => children.push(piece_literal(Target::Python, &text)),
                        // Emit span at level 0 — parent handles indentation
                        TextPiece::Span(span) => children.push(self.emit_node(&span, 0)),
                    }
                }
            }
        } else {
            for child in node.children().filter(|n| n.is_element()) {
                let s = self.emit_node(&child, level + 1);
                if !s.is_empty() {
                    children.push(s);
                }
            }
        }

        children
    }

    fn emit_text_call(
        &self,
        ind0: &str,
        ind1: &str,
        method: &str,
        attrs: &str,
        children: &[String],
    ) -> String {
        if children.len() == 1 {
            format!("{ind0}{method}({attrs}, [{}])", children[0])
        } else {
            // Python: no trailing comma on last item
            let last = children.len() - 1;
            let items: Vec<String> = children
                .iter()
                .enumerate()
                .map(|(i, c)| if i < last { format!("{ind1}{c},") } else { format!("{ind1}{c}") })
                .collect();
            format!("{ind0}{method}({attrs}, [\n{}\n{ind0}])", items.join("\n"))
        }
    }

    fn emit_block_call(
        &self,
        ind0: &str,
        method: &str,
        attrs: &str,
        children: &[String],
    ) -> String {
        if children.is_empty() {
            return format!("{ind0}{method}({attrs})");
        }
        // Python: no trailing comma on last item
        let last = children.len() - 1;
        let items: Vec<String> = children
            .iter()
            .enumerate()
            .map(|(i, c)| if i < last { format!("{c},") } else { c.clone() })
            .collect();
        format!("{ind0}{method}({attrs}, [\n{}\n{ind0}])", items.join("\n"))
    }
}

/// Return the Python method name for an XML element tag.
fn python_method(tag: &str) -> &'static str {
    match tag {
        "document" => "document",
        "section"  => "section",
        "layout"   => "layout",
        "layer"    => "layer",
        "tokens"   => "tokens",
        "column"   => "column",
        "row"      => "row",
        "grid"     => "grid",
        "list"     => "list",
        "li"       => "li",
        "frame"    => "frame",
        "link"     => "link",
        "bookmark" => "bookmark",
        "text"     => "text",
        "code"     => "code",
        "img"      => "img",
        "shape"    => "shape",
        "divider"  => "divider",
        "table"    => "table",
        "thead"    => "thead",
        "tr"       => "tr",
        "td"       => "td",
        "barcode"  => "barcode",
        "field"    => "field",
        "region"   => "region",
        "span"     => "span",
        "code-span" => "code_span",
        "fn"       => "fn",
        "rect"     => "rect",
        "circle"   => "circle",
        "ellipse"  => "ellipse",
        "line"     => "line",
        "path"     => "path",
        "text-at"    => "text_at",
        "img-at"     => "img_at",
        "barcode-at" => "barcode_at",
        _          => "element",
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camel_case() {
        assert_eq!(to_camel_case("font-size"),    "fontSize");
        assert_eq!(to_camel_case("stroke-width"), "strokeWidth");
        assert_eq!(to_camel_case("data-value"),   "dataValue");
        assert_eq!(to_camel_case("hrt"),           "hrt");
        assert_eq!(to_camel_case("col-width"),    "colWidth");
    }

    const TYPED: &str = r#"<lpdf version="1"><document debug="false"><section><layout>
<grid cols="3" opacity=".5"><text bold="true" line-height="1.5">a</text><text line-height="m">b</text></grid>
</layout></section></document></lpdf>"#;

    #[test]
    fn test_typed_attributes_are_written_as_literals_of_their_type() {
        let code = |target: &str| codegen(TYPED, &CodegenOptions { target: target.into(), indent: 4 }).unwrap();
        let js = code("js");
        for written in ["{ debug: false }", "L.grid({ cols: 3, opacity: 0.5 }", "L.text({ bold: true, lineHeight: 1.5 }", "{ lineHeight: 'm' }"] {
            assert!(js.contains(written), "{written}: {js}");
        }
        let python = code("python");
        for written in ["debug=False", "GridAttr(cols=3, opacity=0.5)", "TextAttr(bold=True, line_height=1.5)", "line_height='m'"] {
            assert!(python.contains(written), "{written}: {python}");
        }
        let php = code("php");
        for written in ["debug: false", "new GridAttr(cols: 3, opacity: 0.5)", "new TextAttr(bold: true, lineHeight: 1.5)", "lineHeight: 'm'"] {
            assert!(php.contains(written), "{written}: {php}");
        }
        let dotnet = code("dotnet");
        for written in ["Debug = false", "new() { Cols = 3, Opacity = 0.5 }", "new() { Bold = true, LineHeight = 1.5 }", "LineHeight = \"m\""] {
            assert!(dotnet.contains(written), "{written}: {dotnet}");
        }
    }

    #[test]
    fn test_a_value_the_engine_refuses_is_refused_with_the_engine_s_error() {
        let xml = TYPED.replace(r#"bold="true""#, r#"bold="1""#);
        let error = codegen(&xml, &CodegenOptions { target: "js".into(), indent: 4 }).unwrap_err();
        assert!(error.contains("bold must be true or false, not \"1\""), "{error}");
        let error = codegen_fragment(r#"<grid cols="x"/>"#, &CodegenOptions { target: "js".into(), indent: 4 }).unwrap_err();
        assert!(error.contains("cols must be a whole number from 1 to 12"), "{error}");
    }

    #[test]
    fn test_string_literals_escape_backslashes_and_quotes() {
        assert_eq!(single_quoted(r"a\b'c"), r"'a\\b\'c'");
        assert_eq!(double_quoted(r#"a\b"c"#), r#""a\\b\"c""#);
    }

    #[test]
    fn test_minimal_document() {
        let xml = r#"<?xml version="1.0"?>
<lpdf version="1">
  <document size="a4">
    <section>
      <layout margin="48pt">
        <text font-size="12pt">Hello</text>
      </layout>
    </section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "js".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("L.document("));
        assert!(out.contains("L.section("));
        assert!(out.contains("L.text({ fontSize: '12pt' }, ['Hello'])"));
        assert!(out.contains("const pdf = await engine.render(doc)"));
    }

    #[test]
    fn test_no_attr() {
        let xml = r#"<?xml version="1.0"?>
<lpdf version="1">
  <document>
    <section>
      <layout>
        <column>
          <text>Hi</text>
        </column>
      </layout>
    </section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "js".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("L.column(NoAttr, ["));
    }

    #[test]
    fn test_tokens_are_an_attribute_of_the_document() {
        let xml = r##"<?xml version="1.0"?>
<lpdf version="1">
  <tokens>
    <text-size xs="7pt" m="11pt"/>
    <colors>
      <color name="primary" value="#1763cf"/>
    </colors>
  </tokens>
  <document>
    <section><layout><text>x</text></layout></section>
  </document>
</lpdf>"##;
        let opts = CodegenOptions { target: "js".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("const doc = L.document({ tokens: { textSize: { xs: '7pt', m: '11pt' }, colors: { primary: '#1763cf' } } }, ["), "{out}");
        assert!(!out.contains("const tokens"), "{out}");
    }

    #[test]
    fn attributes_bound_to_data_become_todo_comments_in_every_target() {
        let xml = r##"<?xml version="1.0"?>
<lpdf version="1">
  <assets><image name="logo"/></assets>
  <document>
    <section><layout>
      <link href="https://x.example" data-href="url"><img name="logo" data-name="photo" width="40pt"/></link>
      <shape type="rect" width="10pt" height="10pt" fill="#999999" data-fill="status.color"/>
      <text color="#000000" data-color="tone">Late</text>
    </layout></section>
  </document>
</lpdf>"##;
        for (target, marker, bound, builder_name) in [
            ("js", "//", "dataHref", "dataFill"),
            ("dotnet", "//", "DataHref", "DataColor"),
            ("php", "//", "dataColor", "dataName"),
            ("python", "#", "data_href", "data_fill"),
        ] {
            let out = codegen(xml, &CodegenOptions { target: target.into(), indent: 4 }).unwrap();
            for line in ["data-href: url", "data-name: photo", "data-fill: status.color", "data-color: tone"] {
                assert!(out.contains(&format!("{marker} TODO (Lpdf) {line}")), "{target}: {line}\n{out}");
            }
            assert!(!out.contains(bound) && !out.contains(builder_name), "{target} writes a bound attribute as code\n{out}");
        }
    }

    #[test]
    fn test_data_binding_comment() {
        let xml = r#"<?xml version="1.0"?>
<lpdf version="1">
  <document>
    <section><layout>
      <text data-value="invoice.number"/>
    </layout></section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "js".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("// TODO (Lpdf) data-value: invoice.number"));
        assert!(out.contains("'{invoice.number}'"));
    }

    #[test]
    fn test_assets_are_attributes_of_the_document() {
        let xml = r#"<?xml version="1.0"?>
<lpdf version="1">
  <assets>
    <font name="heading" core="Times-Bold"/>
    <font name="body" ref="body-font" src="./fonts/Body.ttf"/>
    <image name="logo" src="./assets/logo.png"/>
  </assets>
  <document>
    <section><layout><text>x</text></layout></section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "js".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("assets: { fonts: [{ name: 'heading', core: 'Times-Bold' }, { name: 'body', ref: 'body-font', src: './fonts/Body.ttf' }], images: [{ name: 'logo', src: './assets/logo.png' }] }"), "{out}");
        assert!(!out.contains("loadFont") && !out.contains("loadImage"), "{out}");
    }

    #[test]
    fn test_gradients_are_a_map_of_tokens_in_every_language() {
        let xml = r##"<?xml version="1.0"?>
<lpdf version="1">
  <tokens>
    <colors><color name="brand" value="#336699"/></colors>
    <gradients>
      <gradient name="fade" type="linear" from="brand" to="#ffffff" angle="90"/>
      <gradient name="sunset" type="radial">
        <stop offset="0%" color="#f97316"/>
        <stop offset="100%" color="#8b5cf6"/>
      </gradient>
    </gradients>
  </tokens>
  <document>
    <section><layout><column background="gradient:fade"><text>x</text></column></layout></section>
  </document>
</lpdf>"##;
        let code = |target: &str| codegen(xml, &CodegenOptions { target: target.into(), indent: 4 }).unwrap();

        assert!(code("js").contains(
            "gradients: { fade: { type: 'linear', from: 'brand', to: '#ffffff', angle: 90 }, sunset: { type: 'radial', stops: [{ offset: '0%', color: '#f97316' }, { offset: '100%', color: '#8b5cf6' }] } }"
        ), "{}", code("js"));
        assert!(code("python").contains(
            "gradients={'fade': {'type': 'linear', 'from': 'brand', 'to': '#ffffff', 'angle': 90}, 'sunset': {'type': 'radial', 'stops': [{'offset': '0%', 'color': '#f97316'}, {'offset': '100%', 'color': '#8b5cf6'}]}}"
        ), "{}", code("python"));
        assert!(code("php").contains(
            "gradients: ['fade' => new Gradient(type: 'linear', from: 'brand', to: '#ffffff', angle: 90), 'sunset' => new Gradient(type: 'radial', stops: [new GradientStop(offset: '0%', color: '#f97316'), new GradientStop(offset: '100%', color: '#8b5cf6')])]"
        ), "{}", code("php"));
        assert!(code("php").contains("use Lpdf\\Kit\\Gradient;") && code("php").contains("use Lpdf\\Kit\\GradientStop;"), "{}", code("php"));
        assert!(code("dotnet").contains(
            "Gradients: new() { [\"fade\"] = new Gradient { Type = \"linear\", From = \"brand\", To = \"#ffffff\", Angle = 90 }, [\"sunset\"] = new Gradient { Type = \"radial\", Stops = [new GradientStop { Offset = \"0%\", Color = \"#f97316\" }, new GradientStop { Offset = \"100%\", Color = \"#8b5cf6\" }] } }"
        ), "{}", code("dotnet"));
    }

    const FULL_DOCUMENT: &str = r##"<lpdf version="1">
  <assets><image name="logo" src="logo.png"/></assets>
  <tokens><colors><color name="brand" value="#336699"/></colors></tokens>
  <document size="a4">
    <section><layout><text color="brand">Hi</text></layout></section>
  </document>
</lpdf>"##;

    fn fragment(target: &str) -> String {
        codegen_fragment(FULL_DOCUMENT, &CodegenOptions { target: target.into(), indent: 4 }).unwrap()
    }

    #[test]
    fn test_fragment_of_a_whole_document_is_the_document_with_its_assets_and_tokens() {
        let js = fragment("js");
        assert!(js.starts_with("L.document({ size: 'a4', assets: { images: [{ name: 'logo', src: 'logo.png' }] }, tokens: { colors: { brand: '#336699' } } }, ["), "{js}");
        assert!(!js.contains("L.element"), "{js}");

        let python = fragment("python");
        assert!(python.starts_with("L.document(DocumentAttr(size='a4', assets=DocumentAssets(images=[ImageAttr(name='logo', src='logo.png')]), tokens=DocumentTokens(colors={'brand': '#336699'})), ["), "{python}");

        let php = fragment("php");
        assert!(php.starts_with("L::document(new DocumentAttr(size: 'a4', assets: new DocumentAssets(images: [new ImageAttr(name: 'logo', src: 'logo.png')]), tokens: new DocumentTokens(colors: ['brand' => '#336699'])), ["), "{php}");

        let dotnet = fragment("dotnet");
        assert!(dotnet.starts_with("L.Document(new() { Size = \"a4\", Assets = new DocumentAssets(Images: [new ImageAttr { Name = \"logo\", Src = \"logo.png\" }]), Tokens = new DocumentTokens(Colors: new() { [\"brand\"] = \"#336699\" }) }, ["), "{dotnet}");
    }

    #[test]
    fn test_program_imports_the_classes_it_uses() {
        let opts = |target: &str| CodegenOptions { target: target.into(), indent: 4 };
        let python = codegen(FULL_DOCUMENT, &opts("python")).unwrap();
        assert!(python.contains("from lpdf import L, NoAttr, DocumentAssets, DocumentAttr, DocumentTokens, ImageAttr, TextAttr\n"), "{python}");

        let php = codegen(FULL_DOCUMENT, &opts("php")).unwrap();
        for class in ["DocumentAssets", "DocumentAttr", "DocumentTokens", "ImageAttr"] {
            assert!(php.contains(&format!("use Lpdf\\Kit\\{class};\n")), "{class}: {php}");
        }
        assert!(php.contains("use Lpdf\\Layout\\TextAttr;\n"), "{php}");
    }

    #[test]
    fn test_unknown_target() {
        let xml = "<lpdf version=\"1\"><document><section><layout></layout></section></document></lpdf>";
        let opts = CodegenOptions { target: "ruby".into(), indent: 4 };
        assert!(codegen(xml, &opts).is_err());
    }

    #[test]
    fn test_meta_folded() {
        let xml = r#"<?xml version="1.0"?>
<lpdf version="1">
  <document size="a4">
    <meta title="My Doc" author="Alice"/>
    <section><layout><text>x</text></layout></section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "js".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("meta: { title: 'My Doc', author: 'Alice' }"));
        // meta should NOT appear as its own L.meta() call
        assert!(!out.contains("L.meta("));
    }

    #[test]
    fn pdfa_is_written_in_each_language_s_case() {
        let xml = r#"<lpdf version="1"><document><output pdfa="2u"/><section><layout><text>x</text></layout></section></document></lpdf>"#;
        for (target, expected) in [
            ("js", "output: { pdfa: '2u' }"),
            ("dotnet", "Output = new() { PdfA = \"2u\" }"),
            ("php", "output: new DocumentOutput(pdfa: '2u')"),
            ("python", "output=DocumentOutput(pdfa='2u')"),
        ] {
            let out = codegen(xml, &CodegenOptions { target: target.into(), indent: 4 }).unwrap();
            assert!(out.contains(expected), "{target}: {out}");
        }
    }

    #[test]
    fn attachments_are_a_list_of_files_in_each_language() {
        let xml = r#"<lpdf version="1"><document><output encrypt="aes-256"/><attachments>
            <file name="invoice.xml" src="invoice.xml" mime="text/xml" relationship="data"/><file name="terms.txt" ref="terms"/>
        </attachments><section><layout><text>x</text></layout></section></document></lpdf>"#;
        for (target, expected) in [
            ("js", "output: { encrypt: 'aes-256' }, attachments: [{ name: 'invoice.xml', src: 'invoice.xml', mime: 'text/xml', relationship: 'data' }, { name: 'terms.txt', ref: 'terms' }] }"),
            ("dotnet", "Attachments = [new FileAttr { Name = \"invoice.xml\", Src = \"invoice.xml\", Mime = \"text/xml\", Relationship = \"data\" }, new FileAttr { Name = \"terms.txt\", Ref = \"terms\" }] }"),
            ("php", "attachments: [new FileAttr(name: 'invoice.xml', src: 'invoice.xml', mime: 'text/xml', relationship: 'data'), new FileAttr(name: 'terms.txt', ref: 'terms')])"),
            ("python", "attachments=[FileAttr(name='invoice.xml', src='invoice.xml', mime='text/xml', relationship='data'), FileAttr(name='terms.txt', ref='terms')])"),
        ] {
            let out = codegen(xml, &CodegenOptions { target: target.into(), indent: 4 }).unwrap();
            assert!(out.contains(expected), "{target}: {out}");
            assert!(!out.contains("L.attachments") && !out.contains("L.file"), "{target}: {out}");
        }
        let dotnet = codegen(xml, &CodegenOptions { target: "dotnet".into(), indent: 4 }).unwrap();
        assert!(dotnet.contains("SrcFallback = File.ReadAllBytes"), "the file's src is read as an image's is: {dotnet}");
        let php = codegen(xml, &CodegenOptions { target: "php".into(), indent: 4 }).unwrap();
        assert!(php.contains("use Lpdf\\Kit\\FileAttr;"), "{php}");
        let python = codegen(xml, &CodegenOptions { target: "python".into(), indent: 4 }).unwrap();
        assert!(python.contains("FileAttr"), "{python}");
    }

    #[test]
    fn pdfua_lang_alt_and_role_are_written_in_each_language_s_case() {
        let xml = r#"<lpdf version="1"><assets><image name="logo" src="logo.png"/></assets><document><meta title="T" lang="en"/><output pdfua="1"/><section><layout><text role="h1">x</text><img name="logo" alt="Logo"/></layout></section></document></lpdf>"#;
        for (target, expected) in [
            ("js", ["pdfua: '1'", "lang: 'en'", "role: 'h1'", "alt: 'Logo'"]),
            ("dotnet", ["PdfUa = \"1\"", "Lang = \"en\"", "Role = \"h1\"", "Alt = \"Logo\""]),
            ("php", ["pdfua: '1'", "lang: 'en'", "role: 'h1'", "alt: 'Logo'"]),
            ("python", ["pdfua='1'", "lang='en'", "role='h1'", "alt='Logo'"]),
        ] {
            let out = codegen(xml, &CodegenOptions { target: target.into(), indent: 4 }).unwrap();
            for piece in expected {
                assert!(out.contains(piece), "{target}: {piece}\n{out}");
            }
        }
    }

    #[test]
    fn test_output_folded_after_meta() {
        let xml = r#"<?xml version="1.0"?>
<lpdf version="1">
  <document size="a4">
    <meta title="My Doc"/>
    <output encrypt="aes-256" allow="print fill-forms"/>
    <section><layout><text>x</text></layout></section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "js".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("meta: { title: 'My Doc' }, output: { encrypt: 'aes-256', allow: 'print fill-forms' }"), "{out}");
        assert!(!out.contains("L.output("));
    }

    #[test]
    fn test_span_mixed_content() {
        let xml = r##"<?xml version="1.0"?>
<lpdf version="1">
  <document>
    <section><layout>
      <text font-size="13pt">Hello <span color="#f00">world</span></text>
    </layout></section>
  </document>
</lpdf>"##;
        let opts = CodegenOptions { target: "js".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("L.span({ color: '#f00' }, ['world'])"));
        // Multiple children → multi-line text (children on separate indented lines)
        assert!(out.contains("L.text({ fontSize: '13pt' }, ["));
        assert!(out.contains("'Hello ',"));
    }

    /// The code a text of this XML content makes in a target.
    fn text_code(target: &str, tag: &str, content: &str) -> String {
        let xml = format!(
            "<lpdf version=\"1\"><document><section>{}</section></document></lpdf>",
            if tag == "text-at" {
                format!("<layer><text-at x=\"0pt\" y=\"0pt\">{content}</text-at></layer>")
            } else {
                format!("<layout><text>{content}</text></layout>")
            }
        );
        codegen(&xml, &CodegenOptions { target: target.into(), indent: 4 }).unwrap()
    }

    /// The code a `code` of this XML content makes in a target.
    fn code_code(target: &str, content: &str) -> String {
        let xml = format!(
            "<lpdf version=\"1\"><document><section><layout><code tab-size=\"2\">{content}</code></layout></section></document></lpdf>"
        );
        codegen(&xml, &CodegenOptions { target: target.into(), indent: 4 }).unwrap()
    }

    #[test]
    fn test_a_code_is_written_with_its_lines_and_tabs_escaped_in_every_language() {
        let content = "\nfn a() {\n\tb(\"x\", '$y');\n}\n";
        let js = code_code("js", content);
        assert!(js.contains(r#"L.code({ tabSize: 2 }, ['fn a() {\n\tb("x", \'$y\');\n}\n'])"#), "{js}");
        let python = code_code("python", content);
        assert!(python.contains(r#"L.code(CodeAttr(tab_size=2), ['fn a() {\n\tb("x", \'$y\');\n}\n'])"#), "{python}");
        let dotnet = code_code("dotnet", content);
        assert!(dotnet.contains(r#"L.Code("#) && dotnet.contains(r#""fn a() {\n\tb(\"x\", '$y');\n}\n""#), "{dotnet}");
        let php = code_code("php", content);
        assert!(php.contains("L::code(") && php.contains(r#""fn a() {\n\tb(\"x\", '\$y');\n}\n""#), "{php}");
    }

    #[test]
    fn test_a_code_with_no_line_or_tab_is_a_plain_string_in_php() {
        let php = code_code("php", "x = 1");
        assert!(php.contains("['x = 1']"), "{php}");
    }

    #[test]
    fn test_a_span_in_a_code_keeps_its_white_space_and_a_code_span_is_written_inline() {
        let js = code_code("js", "let <span color=\"#c0392b\">  x  </span> = 1;");
        assert!(js.contains("L.span({ color: '#c0392b' }, ['  x  '])"), "{js}");
        let xml = "<lpdf version=\"1\"><document><section><layout><text>Run <code-span background=\"#ffff00\">cargo test</code-span> now</text></layout></section></document></lpdf>";
        let js = codegen(xml, &CodegenOptions { target: "js".into(), indent: 4 }).unwrap();
        assert!(js.contains("L.codeSpan({ background: '#ffff00' }, ['cargo test'])"), "{js}");
        assert!(js.contains("'Run ',") && js.contains("' now'"), "{js}");
        for (target, call) in [("dotnet", "L.CodeSpan("), ("php", "L::codeSpan("), ("python", "L.code_span(")] {
            let out = codegen(xml, &CodegenOptions { target: target.into(), indent: 4 }).unwrap();
            assert!(out.contains(call), "{target}: {out}");
        }
    }

    /// An attribute as the SDKs put it in a tree: a boolean as `true` or `false`, the rest as text, as the spans have
    /// no number.
    fn tree_value(element: &Node, attribute: roxmltree::Attribute) -> serde_json::Value {
        let parent = element.parent_element().map_or("", |p| p.tag_name().name());
        match crate::attr_types::kind_of(parent, element.tag_name().name(), attribute.name()) {
            Kind::Boolean => serde_json::Value::Bool(attribute.value() == "true"),
            _ => serde_json::Value::String(attribute.value().to_string()),
        }
    }

    /// The code's XML, and the tree the SDKs would build from the strings and spans the generator writes for it.
    fn code_xml_and_tree_pdfs(content: &str) -> (Vec<u8>, Vec<u8>) {
        let xml = format!(
            "<lpdf version=\"1\"><document><section><layout><code tab-size=\"3\">{content}</code></layout></section></document></lpdf>"
        );
        let parsed = Document::parse(&xml).unwrap();
        let code = parsed.descendants().find(|n| n.has_tag_name("code")).unwrap();
        let nodes: Vec<serde_json::Value> = text_pieces(&code)
            .into_iter()
            .map(|piece| match piece {
                TextPiece::Str(s) => serde_json::Value::String(s),
                TextPiece::Span(span) => {
                    let attrs: serde_json::Map<String, serde_json::Value> =
                        span.attributes().map(|a| (a.name().to_string(), tree_value(&span, a))).collect();
                    let inner: Vec<serde_json::Value> = text_pieces(&span)
                        .into_iter()
                        .filter_map(|p| if let TextPiece::Str(s) = p { Some(serde_json::Value::String(s)) } else { None })
                        .collect();
                    serde_json::json!({ "type": "span", "attrs": attrs, "nodes": inner })
                }
            })
            .collect();
        let tree = serde_json::json!({
            "version": 1, "type": "document", "attrs": {},
            "nodes": [{ "type": "section", "attrs": {}, "nodes": [{
                "type": "layout", "nodes": [{ "type": "code", "attrs": { "tab-size": 3 }, "nodes": nodes }]
            }]}]
        });
        (
            pdf_of(crate::parse::parse(&xml).unwrap()),
            pdf_of(crate::parse::parse_tree(&tree.to_string()).unwrap()),
        )
    }

    #[test]
    fn test_the_strings_and_spans_written_for_a_code_render_the_pdf_of_the_xml() {
        for content in [
            "fn a() {\n\tb();\n}",
            "\nfn a() {\n    b();\n}\n",
            "\n    indented\n      more\n    ",
            "let <span color=\"#c0392b\">  x  </span> = 1;\n  // done",
            "<span bold=\"true\">a</span> b\n\n\nc",
        ] {
            let (from_xml, from_tree) = code_xml_and_tree_pdfs(content);
            assert_eq!(from_xml, from_tree, "{content:?}");
        }
    }

    #[test]
    fn test_a_footnote_is_written_as_an_inline_call_holding_its_words() {
        let content = r#"Total<fn>The <span bold="true">note</span>.</fn> here"#;
        let js = text_code("js", "text", content);
        assert!(js.contains("L.fn(NoAttr, ['The ', L.span({ bold: true }, ['note']), '.'])"), "{js}");
        let dotnet = text_code("dotnet", "text", content);
        assert!(dotnet.contains("L.Fn(") && dotnet.contains("\"The \""), "{dotnet}");
        let php = text_code("php", "text", content);
        assert!(php.contains("L::fn(") && php.contains("'The '"), "{php}");
        let python = text_code("python", "text", content);
        assert!(python.contains("L.fn(") && python.contains("'The '"), "{python}");
    }

    #[test]
    fn test_space_before_a_span_stays_on_its_string() {
        let out = text_code("js", "text", r#"See the <span bold="true">docs</span>"#);
        assert!(out.contains("'See the ',\n"), "{out}");
        assert!(out.contains("L.span({ bold: true }, ['docs'])"), "{out}");
    }

    #[test]
    fn test_space_after_a_span_stays_on_its_string() {
        let out = text_code("js", "text", r#"<span bold="true">5</span> units"#);
        assert!(out.contains("' units',"), "{out}");
    }

    #[test]
    fn test_space_on_both_sides_of_a_span() {
        let out = text_code("js", "text", r#"Read <span bold="true">this</span> twice"#);
        assert!(out.contains("'Read ',"), "{out}");
        assert!(out.contains("' twice',"), "{out}");
    }

    #[test]
    fn test_no_space_beside_a_span_where_the_xml_has_none() {
        let out = text_code("js", "text", r#"Tight<span bold="true">er</span>run"#);
        assert!(out.contains("'Tight',"), "{out}");
        assert!(out.contains("'run',"), "{out}");
    }

    #[test]
    fn test_space_between_two_spans_is_a_string_of_its_own() {
        let out = text_code("js", "text", r#"<span bold="true">a</span> <span underline="true">b</span>"#);
        assert!(out.contains("' ',"), "{out}");
        let out = text_code("js", "text", r#"<span bold="true">a</span><span underline="true">b</span>"#);
        assert!(!out.contains("' '"), "{out}");
    }

    #[test]
    fn test_the_whitespace_at_a_spans_edges_is_written_beside_it() {
        // The engine reads whitespace at either edge of a span as a space between the span and its neighbour.
        let out = text_code("js", "text", r#"Total:<span bold="true"> 5</span>"#);
        assert!(out.contains("'Total: ',"), "{out}");
        assert!(out.contains("['5']"), "{out}");
        let out = text_code("js", "text", r#"<span bold="true">a </span>b"#);
        assert!(out.contains("['a']"), "{out}");
        assert!(out.contains("' b',"), "{out}");
    }

    #[test]
    fn test_text_edges_and_line_breaks_make_no_spaces() {
        let out = text_code("js", "text", "\n      Read the\n      <span bold=\"true\">docs</span>\n      first.\n    ");
        assert!(out.contains("'Read the ',"), "{out}");
        assert!(out.contains("' first.',"), "{out}");
        let out = text_code("js", "text", r#"  <span bold="true">docs</span>  "#);
        assert!(!out.contains("' '"), "{out}");
    }

    #[test]
    fn test_text_at_keeps_the_space_beside_a_span() {
        let out = text_code("js", "text-at", r#"Invoice <span bold="true">#1042</span>"#);
        assert!(out.contains("'Invoice ',"), "{out}");
    }

    #[test]
    fn test_every_target_keeps_the_space_beside_a_span() {
        let content = r#"See the <span bold="true">docs</span> now"#;
        let out = text_code("dotnet", "text", content);
        assert!(out.contains("\"See the \","), "{out}");
        assert!(out.contains("\" now\","), "{out}");
        for target in ["php", "python"] {
            let out = text_code(target, "text", content);
            assert!(out.contains("'See the ',"), "{target}: {out}");
            assert!(out.contains("' now'"), "{target}: {out}");
        }
    }

    /// The PDF the engine makes of a parsed document.
    fn pdf_of(mut doc: crate::parse::Document) -> Vec<u8> {
        let layouts = doc.section_layouts();
        let pages: Vec<crate::render::RenderPage> = crate::layout::layout_document(&layouts);
        let fonts = crate::pdf::FontRegistry::new();
        let images = crate::pdf::ImageRegistry::new();
        crate::pdf::render_pdf(&pages, &doc.fonts, &fonts, &images, &doc.meta, &doc.gradients, None, false, &doc.output, &[]).unwrap()
    }

    /// The text's XML, and the tree the SDKs would build from the strings and spans the generator writes for it.
    fn xml_and_tree_pdfs(content: &str) -> (Vec<u8>, Vec<u8>) {
        let xml = format!(
            "<lpdf version=\"1\"><document><section><layout><text>{content}</text></layout></section></document></lpdf>"
        );
        let parsed = Document::parse(&xml).unwrap();
        let text = parsed.descendants().find(|n| n.has_tag_name("text")).unwrap();

        fn inline_nodes(parent: &Node) -> Vec<serde_json::Value> {
            text_pieces(parent)
                .into_iter()
                .map(|piece| match piece {
                    TextPiece::Str(s) => serde_json::Value::String(s),
                    TextPiece::Span(inline) => {
                        let attrs: serde_json::Map<String, serde_json::Value> =
                            inline.attributes().map(|a| (a.name().to_string(), tree_value(&inline, a))).collect();
                        serde_json::json!({ "type": inline.tag_name().name(), "attrs": attrs, "nodes": inline_nodes(&inline) })
                    }
                })
                .collect()
        }
        let nodes = inline_nodes(&text);
        let tree = serde_json::json!({
            "version": 1, "type": "document", "attrs": {},
            "nodes": [{ "type": "section", "attrs": {}, "nodes": [{
                "type": "layout", "nodes": [{ "type": "text", "attrs": {}, "nodes": nodes }]
            }]}]
        });
        (
            pdf_of(crate::parse::parse(&xml).unwrap()),
            pdf_of(crate::parse::parse_tree(&tree.to_string()).unwrap()),
        )
    }

    #[test]
    fn test_the_strings_and_spans_written_render_the_pdf_of_the_xml() {
        for content in [
            r#"Total: <span bold="true">5</span>"#,
            r#"<span bold="true">5</span> units"#,
            r#"Read <span bold="true">this</span> twice"#,
            r#"Tight<span bold="true">er</span>run"#,
            r#"<span bold="true">a</span> <span underline="true">b</span>"#,
            r#"<span bold="true">a</span><span underline="true">b</span>"#,
            r#"<span bold="true">a </span><span underline="true">b</span>"#,
            r#"<span bold="true">a </span>b"#,
            r#"Total:<span bold="true"> 5</span>"#,
            r#"<span bold="true">a</span><span underline="true"> b</span>"#,
            r#"<span bold="true">per </span><code-span>x</code-span><span bold="true"> call</span>"#,
            r#"Total<fn>The note.</fn> is five."#,
            r#"Total <fn>The <span bold="true">note</span> here.</fn>and more"#,
            r#"A<fn>One.</fn><fn>Two.</fn> b"#,
            "\n      Read the\n      <span bold=\"true\">docs</span>\n      first.\n    ",
            r#"  <span bold="true">docs</span> and <span underline="true">more</span>  "#,
        ] {
            let (from_xml, from_tree) = xml_and_tree_pdfs(content);
            assert!(from_xml == from_tree, "the tree of {content:?} renders another PDF than its XML");
        }
    }

    #[test]
    fn test_the_pdf_shows_the_space_the_xml_has_beside_a_span() {
        // The check above proves nothing unless the space changes the PDF.
        let (spaced, _) = xml_and_tree_pdfs(r#"Total: <span bold="true">5</span>"#);
        let (tight, _) = xml_and_tree_pdfs(r#"Total:<span bold="true">5</span>"#);
        assert!(spaced != tight);
    }

    #[test]
    fn test_text_over_several_lines_is_one_string() {
        let xml = "<lpdf version=\"1\"><document><section><layout>\
            <text>First line\n          second line</text>\
            </layout></section></document></lpdf>";
        for (target, quote) in [("js", '\''), ("python", '\''), ("php", '\''), ("dotnet", '"')] {
            let opts = CodegenOptions { target: target.into(), indent: 4 };
            let out  = codegen(xml, &opts).unwrap();
            let expected = format!("{quote}First line second line{quote}");
            assert!(out.contains(&expected), "{target}: {out}");
        }
    }

    #[test]
    fn test_layers_are_section_children_before_and_after_the_layout() {
        let xml = r##"<lpdf version="1"><document><section>
            <layer page="first"><rect width="10pt" height="10pt"/></layer>
            <layout><text>Body</text></layout>
            <layer page="last"><rect width="10pt" height="10pt"/></layer>
        </section></document></lpdf>"##;
        for (target, layer, layout, section, canvas) in [
            ("js", "L.layer(", "L.layout(", "L.section(", "L.canvas("),
            ("python", "L.layer(", "L.layout(", "L.section(", "L.canvas("),
            ("php", "L::layer(", "L::layout(", "L::section(", "L::canvas("),
            ("dotnet", "L.Layer(", "L.Layout(", "L.Section(", "L.Canvas("),
        ] {
            let opts = CodegenOptions { target: target.into(), indent: 4 };
            let out  = codegen(xml, &opts).unwrap();
            let at = |call: &str| out.find(call).unwrap_or_else(|| panic!("{target}: no {call} in {out}"));
            let first_layer = at(layer);
            assert!(at(section) < first_layer, "{target}: {out}");
            assert!(first_layer < at(layout), "{target}: {out}");
            assert!(at(layout) < out.rfind(layer).unwrap(), "{target}: {out}");
            assert!(!out.contains(canvas), "{target}: {out}");
        }
    }

    #[test]
    fn test_the_margin_is_an_attribute_of_the_layout() {
        let xml = r#"<lpdf version="1"><document size="a4"><section>
            <layout margin="48pt"><text>Body</text></layout>
        </section></document></lpdf>"#;
        for (target, layout, import) in [
            ("js", "L.layout({ margin: '48pt' }, [", None),
            ("python", "L.layout(LayoutAttr(margin='48pt'), [", Some("from lpdf import L, NoAttr, DocumentAttr, LayoutAttr\n")),
            ("php", "L::layout(new LayoutAttr(margin: '48pt'), [", Some("use Lpdf\\Kit\\LayoutAttr;\n")),
            ("dotnet", "L.Layout(new() { Margin = \"48pt\" }, [", None),
        ] {
            let opts = CodegenOptions { target: target.into(), indent: 4 };
            let out  = codegen(xml, &opts).unwrap();
            assert!(out.contains(layout), "{target}: {out}");
            if let Some(import) = import {
                assert!(out.contains(import), "{target}: {out}");
            }
        }
    }

    #[test]
    fn test_layer_text_image_and_barcode_have_their_own_elements_and_attribute_classes() {
        let xml = r##"<lpdf version="1"><document><section><layer>
            <text-at x="10pt" y="20pt">Hi</text-at>
            <img-at name="logo" x="0pt" y="0pt" width="10pt" height="10pt"/>
            <barcode-at type="qr" data="x" x="0pt" y="0pt" width="40pt"/>
        </layer></section></document></lpdf>"##;
        for (target, builders, classes) in [
            ("js",     ["L.textAt(", "L.imgAt(", "L.barcodeAt("],   [None, None, None]),
            ("python", ["L.text_at(", "L.img_at(", "L.barcode_at("], [Some("TextAtAttr("), Some("ImgAtAttr("), Some("BarcodeAtAttr(")]),
            ("php",    ["L::textAt(", "L::imgAt(", "L::barcodeAt("], [Some("TextAtAttr("), Some("ImgAtAttr("), Some("BarcodeAtAttr(")]),
            ("dotnet", ["L.TextAt(", "L.ImgAt(", "L.BarcodeAt("],   [None, None, None]),
        ] {
            let opts = CodegenOptions { target: target.into(), indent: 4 };
            let out  = codegen(xml, &opts).unwrap();
            for builder in builders {
                assert!(out.contains(builder), "{target}: no {builder} in {out}");
            }
            for class in classes.into_iter().flatten() {
                assert!(out.contains(class), "{target}: no {class} in {out}");
            }
            assert!(out.contains("['Hi']") || out.contains("[\"Hi\"]"), "{target}: the text is lost in {out}");
            assert!(!out.contains("CanvasText") && !out.contains("CanvasImg"), "{target}: {out}");
        }
        // PHP imports the classes from the namespace of the layer's elements.
        let opts = CodegenOptions { target: "php".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("use Lpdf\\Canvas\\TextAtAttr;"), "{out}");
        assert!(out.contains("use Lpdf\\Canvas\\BarcodeAtAttr;"), "{out}");
    }

    #[test]
    fn test_a_shape_of_the_layout_is_a_shape_with_a_shape_attribute_class() {
        let xml = r##"<lpdf version="1"><document><section><layout>
            <row><shape type="circle" r="5pt" fill="primary"/><text>Active</text></row>
        </layout></section></document></lpdf>"##;
        for (target, builder, class) in [
            ("js",     "L.shape(",  None),
            ("python", "L.shape(",  Some("ShapeAttr(type='circle', r='5pt', fill='primary')")),
            ("php",    "L::shape(", Some("new ShapeAttr(type: 'circle', r: '5pt', fill: 'primary')")),
            ("dotnet", "L.Shape(",  Some("new() { Type = \"circle\", R = \"5pt\", Fill = \"primary\" }")),
        ] {
            let opts = CodegenOptions { target: target.into(), indent: 4 };
            let out  = codegen(xml, &opts).unwrap();
            assert!(out.contains(builder), "{target}: no {builder} in {out}");
            if let Some(class) = class {
                assert!(out.contains(class), "{target}: no {class} in {out}");
            }
            assert!(!out.contains("element("), "{target}: {out}");
        }
        let opts = CodegenOptions { target: "js".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("L.shape({ type: 'circle', r: '5pt', fill: 'primary' })"), "{out}");
        // PHP imports the class from the namespace of the layout's elements.
        let opts = CodegenOptions { target: "php".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("use Lpdf\\Layout\\ShapeAttr;"), "{out}");
    }

    #[test]
    fn test_a_list_is_a_list_of_items_with_list_and_li_attribute_classes() {
        let xml = r##"<lpdf version="1"><document><section><layout>
            <list type="number" format="lower-alpha" suffix=")"><li><text>One</text><list bullet="dash"><li><text>Two</text></li></list></li></list>
        </layout></section></document></lpdf>"##;
        for (target, list, item, class) in [
            ("js",     "L.list(",  "L.li(",  "L.list({ type: 'number', format: 'lower-alpha', suffix: ')' }, ["),
            ("python", "L.list(",  "L.li(",  "ListAttr(type='number', format='lower-alpha', suffix=')')"),
            ("php",    "L::list(", "L::li(", "new ListAttr(type: 'number', format: 'lower-alpha', suffix: ')')"),
            ("dotnet", "L.List(",  "L.Li(",  "new() { Type = \"number\", Format = \"lower-alpha\", Suffix = \")\" }"),
        ] {
            let opts = CodegenOptions { target: target.into(), indent: 4 };
            let out  = codegen(xml, &opts).unwrap();
            for expected in [list, item, class] {
                assert!(out.contains(expected), "{target}: no {expected} in {out}");
            }
            assert!(!out.contains("element("), "{target}: {out}");
        }
        // PHP imports the classes from the namespace of the layout's elements.
        let opts = CodegenOptions { target: "php".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("use Lpdf\\Layout\\ListAttr;"), "{out}");
    }

    #[test]
    fn test_layout_text_and_image_keep_their_attribute_classes() {
        let xml = r##"<lpdf version="1"><document><section><layout>
            <text align="right">Hi</text>
            <img name="logo"/>
        </layout></section></document></lpdf>"##;
        let opts = CodegenOptions { target: "python".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("TextAttr(align='right')"), "{out}");
        assert!(out.contains("ImgAttr(name='logo')"), "{out}");
        assert!(!out.contains("Canvas"), "{out}");
    }

    // ── C# (.NET) tests ───────────────────────────────────────────────────────

    #[test]
    fn test_dotnet_minimal_document() {
        let xml = r#"<?xml version="1.0"?>
<lpdf version="1">
  <document size="a4">
    <section>
      <layout margin="48pt">
        <text font-size="12pt">Hello</text>
      </layout>
    </section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "dotnet".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("using Lpdf;"));
        assert!(out.contains("L.Document("));
        assert!(out.contains("L.Section("));
        assert!(out.contains("L.Text(new() { FontSize = \"12pt\" }, [\"Hello\"])"));
        assert!(out.contains("var pdf = engine.Render(doc);"));
        assert!(!out.contains("await"), "{out}");
    }

    #[test]
    fn test_dotnet_no_attr() {
        let xml = r#"<?xml version="1.0"?>
<lpdf version="1">
  <document>
    <section>
      <layout>
        <column>
          <text>Hi</text>
        </column>
      </layout>
    </section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "dotnet".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("L.Column(NoAttr, ["));
        assert!(out.contains("using static Lpdf.L;\n"), "{out}");
    }

    #[test]
    fn test_dotnet_meta_folded() {
        let xml = r#"<?xml version="1.0"?>
<lpdf version="1">
  <document size="a4">
    <meta title="My Doc" author="Alice"/>
    <section><layout><text>x</text></layout></section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "dotnet".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("Meta = new() { Title = \"My Doc\", Author = \"Alice\" }"));
        assert!(!out.contains("L.Meta("));
    }

    #[test]
    fn test_dotnet_output_folded_after_meta() {
        let xml = r#"<?xml version="1.0"?>
<lpdf version="1">
  <document size="a4">
    <meta title="My Doc"/>
    <output encrypt="aes-256" allow="print fill-forms"/>
    <section><layout><text>x</text></layout></section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "dotnet".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("Meta = new() { Title = \"My Doc\" }, Output = new() { Encrypt = \"aes-256\", Allow = \"print fill-forms\" }"), "{out}");
        assert!(!out.contains("L.Output("));
    }

    #[test]
    fn test_dotnet_assets_are_attributes_of_the_document() {
        let xml = r#"<?xml version="1.0"?>
<lpdf version="1">
  <assets>
    <font name="heading" core="Times-Bold"/>
    <font name="body" ref="body-font" src="./fonts/Body.ttf"/>
    <image name="logo" src="./assets/logo.png"/>
  </assets>
  <document>
    <section><layout><text>x</text></layout></section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "dotnet".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("Assets = new DocumentAssets(Fonts: [new FontAttr { Name = \"heading\", Core = \"Times-Bold\" }, new FontAttr { Name = \"body\", Ref = \"body-font\", Src = \"./fonts/Body.ttf\" }], Images: [new ImageAttr { Name = \"logo\", Src = \"./assets/logo.png\" }])"), "{out}");
        assert!(out.contains("using Lpdf.Kit;"), "{out}");
        assert!(out.contains("L.Engine(new() { SrcFallback = File.ReadAllBytes })"), "{out}");
        assert!(!out.contains("LoadFont") && !out.contains("LoadImage"), "{out}");
    }

    #[test]
    fn test_dotnet_data_binding_comment() {
        let xml = r#"<?xml version="1.0"?>
<lpdf version="1">
  <document>
    <section><layout>
      <text data-value="invoice.number"/>
    </layout></section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "dotnet".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("// TODO (Lpdf) data-value: invoice.number"));
        assert!(out.contains("\"{invoice.number}\""));
    }

    #[test]
    fn test_dotnet_tokens_are_an_attribute_of_the_document() {
        let xml = r##"<?xml version="1.0"?>
<lpdf version="1">
  <tokens>
    <text-size xs="7pt" m="11pt"/>
    <colors>
      <color name="primary" value="#1763cf"/>
    </colors>
  </tokens>
  <document>
    <section><layout><text>x</text></layout></section>
  </document>
</lpdf>"##;
        let opts = CodegenOptions { target: "dotnet".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("Tokens = new DocumentTokens(TextSize: new() { [\"xs\"] = \"7pt\", [\"m\"] = \"11pt\" }, Colors: new() { [\"primary\"] = \"#1763cf\" })"), "{out}");
        assert!(!out.contains("var tokens"), "{out}");
    }

    #[test]
    fn a_line_height_row_and_a_line_height_token_are_written_in_every_target() {
        let xml = r##"<?xml version="1.0"?>
<lpdf version="1">
  <tokens>
    <line-height xs="1" m="1.2"/>
  </tokens>
  <document>
    <section><layout><text line-height="l">x</text></layout></section>
  </document>
</lpdf>"##;
        for (target, row, attr) in [
            ("js",     "lineHeight: { xs: 1, m: 1.2 }",                       "lineHeight: 'l'"),
            ("dotnet", "LineHeight: new() { [\"xs\"] = 1, [\"m\"] = 1.2 }", "LineHeight = \"l\""),
            ("php",    "lineHeight: ['xs' => 1, 'm' => 1.2]",                  "lineHeight: 'l'"),
            ("python", "line_height={'xs': 1, 'm': 1.2}",                      "line_height='l'"),
        ] {
            let out = codegen(xml, &CodegenOptions { target: target.into(), indent: 4 }).unwrap();
            assert!(out.contains(row), "{target}: the row is not written as {row}\n{out}");
            assert!(out.contains(attr), "{target}: the attribute is not written as {attr}\n{out}");
        }
    }

    #[test]
    fn aliases_and_a_text_size_that_uses_one_are_written_in_every_target() {
        let xml = r##"<?xml version="1.0"?>
<lpdf version="1">
  <tokens>
    <aliases>
      <alias name="caption" value="xs"/>
      <alias name="sub-title" value="l"/>
    </aliases>
  </tokens>
  <document>
    <section><layout><text font-size="caption">x</text></layout></section>
  </document>
</lpdf>"##;
        for (target, aliases, attr) in [
            ("js",     "aliases: { caption: 'xs', 'sub-title': 'l' }",                          "fontSize: 'caption'"),
            ("dotnet", "Aliases: new() { [\"caption\"] = \"xs\", [\"sub-title\"] = \"l\" }",    "FontSize = \"caption\""),
            ("php",    "aliases: ['caption' => 'xs', 'sub-title' => 'l']",                      "fontSize: 'caption'"),
            ("python", "aliases={'caption': 'xs', 'sub-title': 'l'}",                           "font_size='caption'"),
        ] {
            let out = codegen(xml, &CodegenOptions { target: target.into(), indent: 4 }).unwrap();
            assert!(out.contains(aliases), "{target}: the aliases are not written as {aliases}\n{out}");
            assert!(out.contains(attr), "{target}: the attribute is not written as {attr}\n{out}");
        }
    }

    #[test]
    fn a_height_row_and_a_height_that_uses_a_slot_are_written_in_every_target() {
        let xml = r##"<?xml version="1.0"?>
<lpdf version="1">
  <tokens>
    <height xs="10pt" s="20pt" m="30pt" l="40pt" xl="50pt" xxl="60pt"/>
  </tokens>
  <document>
    <section><layout><column height="m"/></layout></section>
  </document>
</lpdf>"##;
        for (target, row, attr) in [
            ("js",     "height: { xs: '10pt', s: '20pt', m: '30pt'", "height: 'm'"),
            ("dotnet", "Height: new() { [\"xs\"] = \"10pt\"",       "Height = \"m\""),
            ("php",    "height: ['xs' => '10pt'",                    "height: 'm'"),
            ("python", "height={'xs': '10pt'",                       "height='m'"),
        ] {
            let out = codegen(xml, &CodegenOptions { target: target.into(), indent: 4 }).unwrap();
            assert!(out.contains(row), "{target}: the row is not written as {row}\n{out}");
            assert!(out.contains(attr), "{target}: the attribute is not written as {attr}\n{out}");
        }
    }

    #[test]
    fn test_dotnet_pascal_case() {
        assert_eq!(to_pascal_case("font-size"),    "FontSize");
        assert_eq!(to_pascal_case("stroke-width"), "StrokeWidth");
        assert_eq!(to_pascal_case("hrt"),           "Hrt");
        assert_eq!(to_pascal_case("xs"),            "Xs");
        assert_eq!(to_pascal_case("text-size"),    "TextSize");
        assert_eq!(to_pascal_case("pdfa"),          "PdfA");
        assert_eq!(to_pascal_case("pdfua"),         "PdfUa");
    }

    // ── PHP tests ─────────────────────────────────────────────────────────────

    #[test]
    fn test_php_minimal_document() {
        let xml = r#"<?xml version="1.0"?>
<lpdf version="1">
  <document size="a4">
    <section>
      <layout margin="48pt">
        <text font-size="12pt">Hello</text>
      </layout>
    </section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "php".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("<?php"));
        assert!(out.contains("use Lpdf\\L;"));
        assert!(out.contains("L::document("));
        assert!(out.contains("L::section("));
        assert!(out.contains("L::text(new TextAttr(fontSize: '12pt'), ['Hello'])"));
        assert!(out.contains("$pdf = $engine->render($doc);"));
    }

    #[test]
    fn test_php_no_attr() {
        let xml = r#"<?xml version="1.0"?>
<lpdf version="1">
  <document>
    <section>
      <layout>
        <column>
          <text>Hi</text>
        </column>
      </layout>
    </section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "php".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("L::column(NoAttr, ["));
    }

    #[test]
    fn test_php_meta_folded() {
        let xml = r#"<?xml version="1.0"?>
<lpdf version="1">
  <document size="a4">
    <meta title="My Doc" author="Alice"/>
    <section><layout><text>x</text></layout></section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "php".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("meta: new DocumentMeta(title: 'My Doc', author: 'Alice')"));
        assert!(!out.contains("L::meta("));
    }

    #[test]
    fn test_php_output_folded_after_meta() {
        let xml = r#"<?xml version="1.0"?>
<lpdf version="1">
  <document size="a4">
    <meta title="My Doc"/>
    <output encrypt="aes-256" allow="print fill-forms"/>
    <section><layout><text>x</text></layout></section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "php".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("output: new DocumentOutput(encrypt: 'aes-256', allow: 'print fill-forms')"), "{out}");
        assert!(out.contains("use Lpdf\\Kit\\DocumentOutput;"), "{out}");
        assert!(!out.contains("L::output("));
    }

    #[test]
    fn test_php_assets_are_attributes_of_the_document() {
        let xml = r#"<?xml version="1.0"?>
<lpdf version="1">
  <assets>
    <font name="heading" core="Times-Bold"/>
    <font name="body" ref="body-font" src="./fonts/Body.ttf"/>
    <image name="logo" src="./assets/logo.png"/>
  </assets>
  <document>
    <section><layout><text>x</text></layout></section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "php".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("assets: new DocumentAssets(fonts: [new FontAttr(name: 'heading', core: 'Times-Bold'), new FontAttr(name: 'body', ref: 'body-font', src: './fonts/Body.ttf')], images: [new ImageAttr(name: 'logo', src: './assets/logo.png')])"), "{out}");
        assert!(out.contains("use Lpdf\\Kit\\DocumentAssets;") && out.contains("use Lpdf\\Kit\\FontAttr;"), "{out}");
        assert!(!out.contains("loadFont") && !out.contains("loadImage"), "{out}");
    }

    #[test]
    fn test_php_data_binding_comment() {
        let xml = r#"<?xml version="1.0"?>
<lpdf version="1">
  <document>
    <section><layout>
      <text data-value="invoice.number"/>
    </layout></section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "php".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("// TODO (Lpdf) data-value: invoice.number"));
        assert!(out.contains("'{invoice.number}'"));
    }

    #[test]
    fn test_php_tokens_are_an_attribute_of_the_document() {
        let xml = r##"<?xml version="1.0"?>
<lpdf version="1">
  <tokens>
    <text-size xs="7pt" m="11pt"/>
    <colors>
      <color name="primary" value="#1763cf"/>
    </colors>
  </tokens>
  <document>
    <section><layout><text>x</text></layout></section>
  </document>
</lpdf>"##;
        let opts = CodegenOptions { target: "php".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("tokens: new DocumentTokens(textSize: ['xs' => '7pt', 'm' => '11pt'], colors: ['primary' => '#1763cf'])"), "{out}");
        assert!(!out.contains("$tokens"), "{out}");
    }

    // ── Python tests ──────────────────────────────────────────────────────────

    #[test]
    fn test_python_minimal_document() {
        let xml = r#"<?xml version="1.0"?>
<lpdf version="1">
  <document size="a4">
    <section>
      <layout margin="48pt">
        <text font-size="12pt">Hello</text>
      </layout>
    </section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "python".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("from lpdf import L, NoAttr"));
        assert!(out.contains("L.document("));
        assert!(out.contains("L.section("));
        assert!(out.contains("L.text(TextAttr(font_size='12pt'), ['Hello'])"));
        assert!(out.contains("pdf = engine.render(doc)"));
    }

    #[test]
    fn test_python_no_attr() {
        let xml = r#"<?xml version="1.0"?>
<lpdf version="1">
  <document>
    <section>
      <layout>
        <column>
          <text>Hi</text>
        </column>
      </layout>
    </section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "python".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("L.column(NoAttr, ["));
    }

    #[test]
    fn test_python_meta_folded() {
        let xml = r#"<?xml version="1.0"?>
<lpdf version="1">
  <document size="a4">
    <meta title="My Doc" author="Alice"/>
    <section><layout><text>x</text></layout></section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "python".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("meta=DocumentMeta(title='My Doc', author='Alice')"));
        assert!(!out.contains("L.meta("));
    }

    #[test]
    fn test_python_output_folded_after_meta() {
        let xml = r#"<?xml version="1.0"?>
<lpdf version="1">
  <document size="a4">
    <meta title="My Doc"/>
    <output encrypt="aes-256" allow="print fill-forms"/>
    <section><layout><text>x</text></layout></section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "python".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("output=DocumentOutput(encrypt='aes-256', allow='print fill-forms')"), "{out}");
        assert!(out.contains("from lpdf import L, NoAttr, DocumentAttr, DocumentMeta, DocumentOutput\n"), "{out}");
        assert!(!out.contains("L.output("));
    }

    #[test]
    fn test_python_assets_are_attributes_of_the_document() {
        let xml = r#"<?xml version="1.0"?>
<lpdf version="1">
  <assets>
    <font name="heading" core="Times-Bold"/>
    <font name="body" ref="body-font" src="./fonts/Body.ttf"/>
    <image name="logo" src="./assets/logo.png"/>
  </assets>
  <document>
    <section><layout><text>x</text></layout></section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "python".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("assets=DocumentAssets(fonts=[FontAttr(name='heading', core='Times-Bold'), FontAttr(name='body', ref='body-font', src='./fonts/Body.ttf')], images=[ImageAttr(name='logo', src='./assets/logo.png')])"), "{out}");
        assert!(out.contains("from lpdf import L, NoAttr, DocumentAssets, DocumentAttr, FontAttr, ImageAttr"), "{out}");
        assert!(!out.contains("load_font") && !out.contains("load_image"), "{out}");
    }

    #[test]
    fn test_python_data_binding_comment() {
        let xml = r#"<?xml version="1.0"?>
<lpdf version="1">
  <document>
    <section><layout>
      <text data-value="invoice.number"/>
    </layout></section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "python".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("# TODO (Lpdf) data-value: invoice.number"));
        assert!(out.contains("'{invoice.number}'"));
    }

    #[test]
    fn test_python_bool_attr_is_a_python_boolean() {
        let xml = r#"<?xml version="1.0"?>
<lpdf version="1">
  <document>
    <section><layout>
      <text bold="true">Hi</text>
    </layout></section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "python".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("bold=True"), "{out}");
    }

    #[test]
    fn test_python_tokens_are_an_attribute_of_the_document() {
        let xml = r##"<?xml version="1.0"?>
<lpdf version="1">
  <tokens>
    <text-size xs="7pt" m="11pt"/>
    <colors>
      <color name="primary" value="#1763cf"/>
    </colors>
  </tokens>
  <document>
    <section><layout><text>x</text></layout></section>
  </document>
</lpdf>"##;
        let opts = CodegenOptions { target: "python".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("tokens=DocumentTokens(text_size={'xs': '7pt', 'm': '11pt'}, colors={'primary': '#1763cf'})"), "{out}");
        assert!(!out.contains("tokens = "), "{out}");
    }

    #[test]
    fn test_python_no_trailing_comma() {
        let xml = r#"<?xml version="1.0"?>
<lpdf version="1">
  <document>
    <section>
      <layout>
        <column>
          <text>A</text>
          <text>B</text>
        </column>
      </layout>
    </section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "python".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        // Last child of column must not have trailing comma
        assert!(!out.contains("L.text(NoAttr, ['B']),"));
        assert!(out.contains("L.text(NoAttr, ['B'])"));
    }

    #[test]
    fn test_snake_case() {
        assert_eq!(to_snake_case("font-size"),    "font_size");
        assert_eq!(to_snake_case("stroke-width"), "stroke_width");
        assert_eq!(to_snake_case("hrt"),           "hrt");
        assert_eq!(to_snake_case("data-if-not"),  "data_if_not");
    }

    // ── codegen_fragment ─────────────────────────────────────────────────────

    #[test]
    fn test_fragment_js_single_node() {
        let xml = r#"<text font-size="12pt">Hello</text>"#;
        let opts = CodegenOptions { target: "js".into(), indent: 4 };
        let out  = codegen_fragment(xml, &opts).unwrap();
        assert_eq!(out.trim(), "L.text({ fontSize: '12pt' }, ['Hello'])");
        // No boilerplate
        assert!(!out.contains("import"));
        assert!(!out.contains("engine"));
    }

    #[test]
    fn test_fragment_js_multiple_nodes() {
        let xml = r#"<text>A</text><text>B</text>"#;
        let opts = CodegenOptions { target: "js".into(), indent: 4 };
        let out  = codegen_fragment(xml, &opts).unwrap();
        assert!(out.contains("L.text(NoAttr, ['A'])"));
        assert!(out.contains("L.text(NoAttr, ['B'])"));
    }

    #[test]
    fn test_fragment_dotnet_single_node() {
        let xml = r#"<text font-size="12pt">Hello</text>"#;
        let opts = CodegenOptions { target: "dotnet".into(), indent: 4 };
        let out  = codegen_fragment(xml, &opts).unwrap();
        assert_eq!(out.trim(), r#"L.Text(new() { FontSize = "12pt" }, ["Hello"])"#);
        assert!(!out.contains("using Lpdf"));
    }

    #[test]
    fn test_fragment_php_single_node() {
        let xml = r#"<text font-size="12pt">Hello</text>"#;
        let opts = CodegenOptions { target: "php".into(), indent: 4 };
        let out  = codegen_fragment(xml, &opts).unwrap();
        assert_eq!(out.trim(), "L::text(new TextAttr(fontSize: '12pt'), ['Hello'])");
        assert!(!out.contains("<?php"));
    }

    #[test]
    fn test_fragment_python_single_node() {
        let xml = r#"<text font-size="12pt">Hello</text>"#;
        let opts = CodegenOptions { target: "python".into(), indent: 4 };
        let out  = codegen_fragment(xml, &opts).unwrap();
        assert_eq!(out.trim(), "L.text(TextAttr(font_size='12pt'), ['Hello'])");
        assert!(!out.contains("import"));
    }

    #[test]
    fn test_fragment_nested() {
        let xml = r#"<column><text>A</text><text>B</text></column>"#;
        let opts = CodegenOptions { target: "js".into(), indent: 4 };
        let out  = codegen_fragment(xml, &opts).unwrap();
        assert!(out.contains("L.column("));
        assert!(out.contains("L.text(NoAttr, ['A'])"));
        assert!(out.contains("L.text(NoAttr, ['B'])"));
    }

    #[test]
    fn test_fragment_empty_errors() {
        let opts = CodegenOptions { target: "js".into(), indent: 4 };
        assert!(codegen_fragment("", &opts).is_err());
        assert!(codegen_fragment("   ", &opts).is_err());
    }

    #[test]
    fn test_fragment_invalid_xml_errors() {
        let opts = CodegenOptions { target: "js".into(), indent: 4 };
        assert!(codegen_fragment("<text>unclosed", &opts).is_err());
    }

    #[test]
    fn test_fragment_unknown_target_errors() {
        let xml = r#"<text>Hello</text>"#;
        let opts = CodegenOptions { target: "ruby".into(), indent: 4 };
        assert!(codegen_fragment(xml, &opts).is_err());
    }

    #[test]
    fn test_bookmark_and_toc_section_in_every_language() {
        let xml = r#"<lpdf version="1"><document>
            <section type="toc"><bookmark label="Contents"/><layout><text>Contents</text></layout></section>
            <section><layout><bookmark label="Methods" level="2"/><text>Methods</text></layout></section>
        </document></lpdf>"#;
        for (target, bookmark, section) in [
            ("js", "L.bookmark({ label: 'Methods', level: 2 })", "L.section({ type: 'toc' }"),
            ("php", "L::bookmark(new BookmarkAttr(label: 'Methods', level: 2))", "L::section(new SectionAttr(type: 'toc')"),
            ("python", "L.bookmark(BookmarkAttr(label='Methods', level=2))", "L.section(SectionAttr(type='toc')"),
            ("dotnet", "L.Bookmark(new() { Label = \"Methods\", Level = 2 })", "L.Section(new() { Type = \"toc\" }"),
        ] {
            let out = codegen(xml, &CodegenOptions { target: target.into(), indent: 4 }).unwrap();
            assert!(out.contains(bookmark), "{target}: {out}");
            assert!(out.contains(section), "{target}: {out}");
        }
    }
    /// A section whose content is a markdown with a rule that is named like an element of the layout.
    const MARKDOWN: &str = r#"<lpdf version="1"><document><section>
        <markdown margin="54pt">
            <markdown-styles src="styles.xml"><h1 color="muted"/><code font="Courier"/></markdown-styles>
            <markdown-content><![CDATA[
                # Title

                Some `code` and a <b> sign.
            ]]></markdown-content>
        </markdown>
    </section></document></lpdf>"#;

    #[test]
    fn test_a_markdown_is_l_markdown_and_its_parts_are_lmarkdown_in_every_language() {
        let text = r#"'# Title\n\nSome `code` and a <b> sign.'"#;
        let js = codegen(MARKDOWN, &CodegenOptions { target: "js".into(), indent: 4 }).unwrap();
        assert!(js.contains("import { L, LMarkdown, NoAttr } from '@lpdfio/lpdf'"), "{js}");
        assert!(js.contains("L.markdown({ margin: '54pt' }, ["), "{js}");
        assert!(js.contains("LMarkdown.styles({ src: 'styles.xml' }, ["), "{js}");
        assert!(js.contains("LMarkdown.h1({ color: 'muted' }),") && js.contains("LMarkdown.code({ font: 'Courier' }),"), "{js}");
        assert!(js.contains(&format!("LMarkdown.content(NoAttr, [{text}])")), "{js}");

        let python = codegen(MARKDOWN, &CodegenOptions { target: "python".into(), indent: 4 }).unwrap();
        assert!(python.contains("from lpdf import L, LMarkdown, NoAttr, MarkdownAttr, MarkdownCodeAttr, MarkdownH1Attr, MarkdownStylesAttr"), "{python}");
        assert!(python.contains("L.markdown(MarkdownAttr(margin='54pt'), [") && python.contains("LMarkdown.h1(MarkdownH1Attr(color='muted'))"), "{python}");

        let php = codegen(MARKDOWN, &CodegenOptions { target: "php".into(), indent: 4 }).unwrap();
        assert!(php.contains("use Lpdf\\LMarkdown;") && php.contains("use Lpdf\\Markdown\\MarkdownCodeAttr;"), "{php}");
        assert!(php.contains("L::markdown(new MarkdownAttr(margin: '54pt'), [") && php.contains("LMarkdown::code(new MarkdownCodeAttr(font: 'Courier'))"), "{php}");

        let dotnet = codegen(MARKDOWN, &CodegenOptions { target: "dotnet".into(), indent: 4 }).unwrap();
        assert!(dotnet.contains("L.Markdown(new() { Margin = \"54pt\" }, [") && dotnet.contains("LMarkdown.Styles(new() { Src = \"styles.xml\" }, ["), "{dotnet}");
        assert!(dotnet.contains("LMarkdown.H1(new() { Color = \"muted\" })") && dotnet.contains("LMarkdown.Content(NoAttr, ["), "{dotnet}");
    }

    #[test]
    fn test_a_document_without_a_markdown_imports_no_lmarkdown() {
        let xml = r#"<lpdf version="1"><document><section><layout><text>Hi</text></layout></section></document></lpdf>"#;
        for target in ["js", "php", "python"] {
            let out = codegen(xml, &CodegenOptions { target: target.into(), indent: 4 }).unwrap();
            assert!(!out.contains("LMarkdown"), "{target}: {out}");
        }
    }
}
