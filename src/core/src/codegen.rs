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

    let elements: Vec<Node> = root.children().filter(|n| n.is_element()).collect();
    if elements.is_empty() {
        return Err("no elements found in fragment".into());
    }

    let lines: Vec<String> = match options.target.as_str() {
        "js" => {
            let emitter = JsEmitter { indent: options.indent };
            elements.iter().map(|n| emitter.emit_node(n, 0, false)).collect()
        }
        "dotnet" => {
            let emitter = DotnetEmitter { indent: options.indent };
            elements.iter().map(|n| emitter.emit_node(n, 0, false)).collect()
        }
        "php" => {
            let emitter = PhpEmitter { indent: options.indent };
            elements.iter().map(|n| emitter.emit_node(n, 0, false)).collect()
        }
        "python" => {
            let emitter = PythonEmitter { indent: options.indent };
            elements.iter().map(|n| emitter.emit_node(n, 0, false)).collect()
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

/// Convert a kebab-case XML attribute name to PascalCase (C# style).
fn to_pascal_case(s: &str) -> String {
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

/// Return the canonical JS method name for an XML element tag, given context.
fn js_method(tag: &str, in_canvas: bool) -> &'static str {
    if in_canvas {
        return match tag {
            "text" => "textAt",
            "img"  => "imgAt",
            other  => js_layout_method(other),
        };
    }
    js_layout_method(tag)
}

fn js_layout_method(tag: &str) -> &'static str {
    match tag {
        "document" => "document",
        "section"  => "section",
        "layout"   => "layout",
        "canvas"   => "canvas",
        "layer"    => "layer",
        "tokens"   => "tokens",
        "stack"    => "stack",
        "flank"    => "flank",
        "split"    => "split",
        "cluster"  => "cluster",
        "grid"     => "grid",
        "frame"    => "frame",
        "link"     => "link",
        "text"     => "text",
        "img"      => "img",
        "divider"  => "divider",
        "table"    => "table",
        "thead"    => "thead",
        "tr"       => "tr",
        "td"       => "td",
        "barcode"  => "barcode",
        "field"    => "field",
        "region"   => "region",
        "span"     => "span",
        "rect"     => "rect",
        "circle"   => "circle",
        "ellipse"  => "ellipse",
        "line"     => "line",
        "path"     => "path",
        _          => "element",
    }
}

/// Return the canonical C# method name for an XML element tag, given context.
fn dotnet_method(tag: &str, in_canvas: bool) -> &'static str {
    if in_canvas {
        return match tag {
            "text" => "TextAt",
            "img"  => "ImgAt",
            other  => dotnet_layout_method(other),
        };
    }
    dotnet_layout_method(tag)
}

fn dotnet_layout_method(tag: &str) -> &'static str {
    match tag {
        "document" => "Document",
        "section"  => "Section",
        "layout"   => "Layout",
        "canvas"   => "Canvas",
        "layer"    => "Layer",
        "tokens"   => "Tokens",
        "stack"    => "Stack",
        "flank"    => "Flank",
        "split"    => "Split",
        "cluster"  => "Cluster",
        "grid"     => "Grid",
        "frame"    => "Frame",
        "link"     => "Link",
        "text"     => "Text",
        "img"      => "Img",
        "divider"  => "Divider",
        "table"    => "Table",
        "thead"    => "Thead",
        "tr"       => "Tr",
        "td"       => "Td",
        "barcode"  => "Barcode",
        "field"    => "Field",
        "region"   => "Region",
        "span"     => "Span",
        "rect"     => "Rect",
        "circle"   => "Circle",
        "ellipse"  => "Ellipse",
        "line"     => "Line",
        "path"     => "Path",
        _          => "Element",
    }
}

// ── Attribute emission ────────────────────────────────────────────────────────

/// A single-quoted string literal, as JS, PHP and Python write one.
fn single_quoted(val: &str) -> String {
    format!("'{}'", val.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// A double-quoted string literal, as C# writes one.
fn double_quoted(val: &str) -> String {
    format!("\"{}\"", val.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Text content as the engine reads it: runs of whitespace are one space and the ends are trimmed, so a
/// string never spans lines.
fn collapse_whitespace(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
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

    /// A map from names to strings: `{ a: 'b' }`, `{'a': 'b'}`, `['a' => 'b']` or `new() { ["a"] = "b" }`.
    fn map(self, entries: &[(String, String)]) -> String {
        let items: Vec<String> = entries
            .iter()
            .map(|(key, value)| match self {
                Target::Js     => format!("{}: {}", js_key(key), single_quoted(value)),
                Target::Python => format!("{}: {}", single_quoted(key), single_quoted(value)),
                Target::Php    => format!("{} => {}", single_quoted(key), single_quoted(value)),
                Target::Dotnet => format!("[{}] = {}", double_quoted(key), double_quoted(value)),
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
                let attributes: Vec<(String, String)> = asset
                    .attributes()
                    .map(|a| (a.name().to_string(), target.literal(a.value())))
                    .collect();
                target.object(class, &attributes, false)
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

/// The `tokens` of a document as the object the SDK takes: the colours and the scales of the `<tokens>`
/// element. `None` when it has none.
fn tokens_expression(target: Target, tokens: &Node) -> Option<String> {
    let mut members: Vec<(String, String)> = Vec::new();
    for child in tokens.children().filter(|n| n.is_element()) {
        let tag = child.tag_name().name();
        let entries: Vec<(String, String)> = if tag == "colors" {
            child
                .children()
                .filter(|n| n.is_element() && n.tag_name().name() == "color")
                .map(|c| {
                    (
                        c.attribute("name").unwrap_or("").to_string(),
                        c.attribute("value").unwrap_or("").to_string(),
                    )
                })
                .collect()
        } else {
            child
                .attributes()
                .map(|a| (a.name().to_string(), a.value().to_string()))
                .collect()
        };
        if !entries.is_empty() {
            members.push((tag.to_string(), target.map(&entries)));
        }
    }
    (!members.is_empty()).then(|| target.object("DocumentTokens", &members, true))
}

/// The SDK classes that generated code uses, once each and in the order they first appear: the attribute
/// classes and the document's meta, assets and tokens.
fn classes_used(code: &str) -> Vec<String> {
    let mut used: Vec<String> = Vec::new();
    for word in code.split(|c: char| !c.is_ascii_alphanumeric()) {
        let is_class = word.starts_with(|c: char| c.is_ascii_uppercase())
            && word != "NoAttr"
            && (word.ends_with("Attr") || matches!(word, "DocumentMeta" | "DocumentAssets" | "DocumentTokens"));
        if is_class && !used.iter().any(|u| u == word) {
            used.push(word.to_string());
        }
    }
    used
}

/// The PHP namespace of an SDK class.
fn php_namespace(class: &str) -> &'static str {
    match class {
        "DocumentAttr" | "DocumentMeta" | "DocumentAssets" | "DocumentTokens" | "SectionAttr" | "FontAttr"
        | "ImageAttr" => "Lpdf\\Kit",
        "LayerAttr" | "RectAttr" | "CircleAttr" | "EllipseAttr" | "LineAttr" | "PathAttr" | "CanvasTextAttr"
        | "CanvasImgAttr" => "Lpdf\\Canvas",
        _ => "Lpdf\\Layout",
    }
}


/// An attribute value as a JS string literal. Every attribute is a string in the SDKs, `true` included.
fn js_attr_value(val: &str) -> String {
    single_quoted(val)
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
        .filter(|a| !matches!(a.name(), "data-value" | "data-source" | "data-if" | "data-if-not"))
        .map(|a| format!("{}: {}", to_camel_case(a.name()), js_attr_value(a.value())))
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

/// Collect data-binding attributes for TODO comment generation (// style).
fn data_binding_comments(node: &Node, indent_str: &str) -> String {
    let mut lines = String::new();
    for attr in node.attributes() {
        let comment = match attr.name() {
            "data-value"  => format!("{indent_str}// TODO (Lpdf) data-value: {}\n", attr.value()),
            "data-source" => format!("{indent_str}// TODO (Lpdf) data-source: {} — loop\n", attr.value()),
            "data-if"     => format!("{indent_str}// TODO (Lpdf) data-if: {}\n", attr.value()),
            "data-if-not" => format!("{indent_str}// TODO (Lpdf) data-if-not: {}\n", attr.value()),
            _ => continue,
        };
        lines.push_str(&comment);
    }
    lines
}

/// Collect data-binding attributes for TODO comment generation (# style, Python).
fn data_binding_comments_hash(node: &Node, indent_str: &str) -> String {
    let mut lines = String::new();
    for attr in node.attributes() {
        let comment = match attr.name() {
            "data-value"  => format!("{indent_str}# TODO (Lpdf) data-value: {}\n", attr.value()),
            "data-source" => format!("{indent_str}# TODO (Lpdf) data-source: {} — loop\n", attr.value()),
            "data-if"     => format!("{indent_str}# TODO (Lpdf) data-if: {}\n", attr.value()),
            "data-if-not" => format!("{indent_str}# TODO (Lpdf) data-if-not: {}\n", attr.value()),
            _ => continue,
        };
        lines.push_str(&comment);
    }
    lines
}

/// An attribute value as a PHP string literal. Every attribute is a string in the SDKs, `true` included.
fn php_attr_value(val: &str) -> String {
    single_quoted(val)
}

/// Emit a PHP named-arg constructor call for the attributes of a node.
///
/// Returns `"NoAttr"` when there are no attributes.
fn php_attrs(node: &Node, tag: &str, in_canvas: bool, extra_attrs: Option<&[(&str, String)]>) -> String {
    let mut parts: Vec<String> = node
        .attributes()
        .filter(|a| !matches!(a.name(), "data-value" | "data-source" | "data-if" | "data-if-not"))
        .map(|a| format!("{}: {}", to_camel_case(a.name()), php_attr_value(a.value())))
        .collect();

    if let Some(extras) = extra_attrs {
        for (k, v) in extras {
            parts.push(format!("{k}: {v}"));
        }
    }

    if parts.is_empty() {
        "NoAttr".into()
    } else {
        let class = attr_class(tag, in_canvas);
        format!("new {class}({})", parts.join(", "))
    }
}

/// The attribute class of an element: `{Element}Attr`, except that text and images on the canvas, which
/// have other attributes than their layout namesakes, are `CanvasTextAttr` and `CanvasImgAttr`.
fn attr_class(tag: &str, in_canvas: bool) -> String {
    match (tag, in_canvas) {
        ("text", true) => "CanvasTextAttr".to_string(),
        ("img", true)  => "CanvasImgAttr".to_string(),
        _              => format!("{}Attr", to_pascal_case(tag)),
    }
}

/// Emit a Python keyword-arg constructor for the attributes of a node.
///
/// Returns `"NoAttr"` when there are no attributes.
fn python_attrs(node: &Node, tag: &str, in_canvas: bool, extra_attrs: Option<&[(&str, String)]>) -> String {
    let mut parts: Vec<String> = node
        .attributes()
        .filter(|a| !matches!(a.name(), "data-value" | "data-source" | "data-if" | "data-if-not"))
        .map(|a| format!("{}={}", to_snake_case(a.name()), python_attr_value(a.value())))
        .collect();

    if let Some(extras) = extra_attrs {
        for (k, v) in extras {
            parts.push(format!("{k}={v}"));
        }
    }

    if parts.is_empty() {
        "NoAttr".into()
    } else {
        let class = attr_class(tag, in_canvas);
        format!("{class}({})", parts.join(", "))
    }
}

/// An attribute value as a Python string literal. Every attribute is a string in the SDKs, `true` included.
fn python_attr_value(val: &str) -> String {
    single_quoted(val)
}

/// An attribute value as a C# string literal. Every attribute is a string in the SDKs, `true` included.
fn dotnet_attr_value(val: &str) -> String {
    double_quoted(val)
}

/// Emit a C# `new() { ... }` initializer for the attributes of a node.
///
/// Returns `"NoAttr"` when there are no attributes.
fn dotnet_attrs(node: &Node, extra_attrs: Option<&[(&str, String)]>) -> String {
    let mut parts: Vec<String> = node
        .attributes()
        .filter(|a| !matches!(a.name(), "data-value" | "data-source" | "data-if" | "data-if-not"))
        .map(|a| format!("{} = {}", to_pascal_case(a.name()), dotnet_attr_value(a.value())))
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

        let mut out = String::new();

        out.push_str("import { L, NoAttr } from '@lpdfio/lpdf'\n");
        out.push('\n');

        out.push_str("const engine = L.engine()\n");
        out.push('\n');

        // The assets and tokens are attributes of the document. The SDK reads the fonts and images that
        // have a src from there.
        if let Some(doc_node) = document_node {
            let doc_expr = self.emit_document_node(&doc_node, 0, assets_node, tokens_node);
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

    fn emit_node(&self, node: &Node, level: usize, in_canvas: bool) -> String {
        let tag = node.tag_name().name();

        // Special cases
        match tag {
            "lpdf"     => return self.emit_lpdf(node, level),
            "document" => return self.emit_document_node(node, level, None, None),
            "meta"     => return String::new(), // folded into document
            _ => {}
        }

        let in_canvas = in_canvas || tag == "layer";
        let method    = js_method(tag, in_canvas);
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
        let children = self.collect_children(node, tag, level, in_canvas, text_content_override.as_deref());

        let call = if children.is_empty() {
            // Leaf — no children arg
            format!("{binding_comments}{ind0}L.{method}({attrs})")
        } else if tag == "text" {
            format!("{binding_comments}{}", self.emit_text_call(&ind0, &ind1, method, &attrs, &children))
        } else if tag == "span" {
            // span always inline: L.span({...}, ['content'])
            format!("{binding_comments}{ind0}L.{method}({attrs}, [{}])", children.join(", "))
        } else {
            format!("{binding_comments}{}", self.emit_block_call(&ind0, &ind1, method, &attrs, &children))
        };

        call
    }

    fn emit_document_node(&self, node: &Node, level: usize, assets: Option<Node>, tokens: Option<Node>) -> String {
        let ind0 = self.ind(level);
        let ind1 = self.ind(level + 1);
        let method = "document";

        // Find <meta> child and fold its attributes in
        let meta_node = node.children().find(|n| n.is_element() && n.tag_name().name() == "meta");
        let meta_inline = meta_node.map(|m| {
            let meta_parts: Vec<String> = m
                .attributes()
                .map(|a| format!("{}: '{}'", to_camel_case(a.name()), a.value().replace('\'', "\\'")))
                .collect();
            if meta_parts.is_empty() {
                String::new()
            } else {
                format!("{{ {} }}", meta_parts.join(", "))
            }
        });

        // Build document attrs with optional meta key
        let mut doc_parts: Vec<String> = node
            .attributes()
            .map(|a| format!("{}: {}", to_camel_case(a.name()), js_attr_value(a.value())))
            .collect();

        if let Some(assets) = assets.and_then(|a| assets_expression(Target::Js, &a)) {
            doc_parts.push(Target::Js.member("assets", &assets));
        }
        if let Some(tokens) = tokens.and_then(|t| tokens_expression(Target::Js, &t)) {
            doc_parts.push(Target::Js.member("tokens", &tokens));
        }
        if let Some(meta_str) = meta_inline {
            if !meta_str.is_empty() {
                doc_parts.push(format!("meta: {meta_str}"));
            }
        }

        let attrs = if doc_parts.is_empty() {
            "NoAttr".into()
        } else {
            format!("{{ {} }}", doc_parts.join(", "))
        };

        // Emit children (skip meta)
        let mut children: Vec<String> = Vec::new();
        children.extend(
            node.children()
                .filter(|n| n.is_element() && n.tag_name().name() != "meta")
                .map(|n| self.emit_node(&n, level + 1, false))
                .filter(|s| !s.is_empty()),
        );

        self.emit_block_call(&ind0, &ind1, method, &attrs, &children)
    }

    /// Collect children of a node as emitted strings.
    fn collect_children(
        &self,
        node: &Node,
        tag: &str,
        level: usize,
        in_canvas: bool,
        text_content_override: Option<&str>,
    ) -> Vec<String> {
        let mut children: Vec<String> = Vec::new();

        if tag == "text" || tag == "span" {
            // Mixed text + <span> children
            let override_text = text_content_override;

            if let Some(placeholder) = override_text {
                // data-value replaces all content
                children.push(format!("'{placeholder}'"));
            } else {
                for child in node.children() {
                    match child.node_type() {
                        NodeType::Text => {
                            let text = collapse_whitespace(child.text().unwrap_or(""));
                            if !text.is_empty() {
                                children.push(single_quoted(&text));
                            }
                        }
                        NodeType::Element if child.tag_name().name() == "span" => {
                            // Emit span at level 0 — parent handles indentation
                            children.push(self.emit_node(&child, 0, in_canvas));
                        }
                        _ => {}
                    }
                }
            }
        } else {
            for child in node.children().filter(|n| n.is_element()) {
                let s = self.emit_node(&child, level + 1, in_canvas);
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
            format!("{ind0}L.{method}({attrs}, [{}])", children[0])
        } else {
            // Multi-line form — children are plain values (strings/inline spans), add indent
            let items: Vec<String> = children.iter().map(|c| format!("{ind1}{c},")).collect();
            format!("{ind0}L.{method}({attrs}, [\n{}\n{ind0}])", items.join("\n"))
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
            return format!("{ind0}L.{method}({attrs})");
        }
        // Children already carry their own leading indentation from emit_node.
        let items: Vec<String> = children.iter().map(|c| format!("{c},")).collect();
        format!("{ind0}L.{method}({attrs}, [\n{}\n{ind0}])", items.join("\n"))
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
        out.push('\n');

        // .NET reads the src of a font or image through the engine's SrcFallback.
        if assets_node.is_some_and(|assets| assets_have_src(&assets)) {
            out.push_str("var engine = L.Engine(new() { SrcFallback = File.ReadAllBytes });\n");
        } else {
            out.push_str("var engine = L.Engine();\n");
        }
        out.push('\n');

        out.push_str(&body);
        out.push('\n');
        out.push_str("var pdf = await engine.Render(doc);\n");

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

    fn emit_node(&self, node: &Node, level: usize, in_canvas: bool) -> String {
        let tag = node.tag_name().name();

        match tag {
            "lpdf"     => return self.emit_lpdf(node, level),
            "document" => return self.emit_document_node(node, level, None, None),
            "meta"     => return String::new(),
            _ => {}
        }

        let in_canvas = in_canvas || tag == "layer";
        let method    = dotnet_method(tag, in_canvas);
        let ind0      = self.ind(level);
        let ind1      = self.ind(level + 1);

        let binding_comments = data_binding_comments(node, &ind0);
        let data_value = node.attribute("data-value");
        let attrs = dotnet_attrs(node, None);
        let text_content_override: Option<String> = data_value.map(|p| format!("{{{p}}}"));

        let children = self.collect_children(node, tag, level, in_canvas, text_content_override.as_deref());

        if children.is_empty() {
            format!("{binding_comments}{ind0}L.{method}({attrs})")
        } else if tag == "text" {
            format!("{binding_comments}{}", self.emit_text_call(&ind0, &ind1, method, &attrs, &children))
        } else if tag == "span" {
            format!("{binding_comments}{ind0}L.{method}({attrs}, [{}])", children.join(", "))
        } else {
            format!("{binding_comments}{}", self.emit_block_call(&ind0, method, &attrs, &children))
        }
    }

    fn emit_document_node(&self, node: &Node, level: usize, assets: Option<Node>, tokens: Option<Node>) -> String {
        let ind0   = self.ind(level);
        let method = "Document";

        let meta_node = node.children().find(|n| n.is_element() && n.tag_name().name() == "meta");
        let meta_inline = meta_node.map(|m| {
            let meta_parts: Vec<String> = m
                .attributes()
                .map(|a| format!("{} = \"{}\"", to_pascal_case(a.name()), a.value().replace('"', "\\\"")))
                .collect();
            if meta_parts.is_empty() {
                String::new()
            } else {
                format!("new() {{ {} }}", meta_parts.join(", "))
            }
        });

        let mut doc_parts: Vec<String> = node
            .attributes()
            .map(|a| format!("{} = {}", to_pascal_case(a.name()), dotnet_attr_value(a.value())))
            .collect();

        if let Some(assets) = assets.and_then(|a| assets_expression(Target::Dotnet, &a)) {
            doc_parts.push(Target::Dotnet.member("assets", &assets));
        }
        if let Some(tokens) = tokens.and_then(|t| tokens_expression(Target::Dotnet, &t)) {
            doc_parts.push(Target::Dotnet.member("tokens", &tokens));
        }
        if let Some(meta_str) = meta_inline {
            if !meta_str.is_empty() {
                doc_parts.push(format!("Meta = {meta_str}"));
            }
        }

        let attrs = if doc_parts.is_empty() {
            "NoAttr".into()
        } else {
            format!("new() {{ {} }}", doc_parts.join(", "))
        };

        let mut children: Vec<String> = Vec::new();
        children.extend(
            node.children()
                .filter(|n| n.is_element() && n.tag_name().name() != "meta")
                .map(|n| self.emit_node(&n, level + 1, false))
                .filter(|s| !s.is_empty()),
        );

        self.emit_block_call(&ind0, method, &attrs, &children)
    }

    fn collect_children(
        &self,
        node: &Node,
        tag: &str,
        level: usize,
        in_canvas: bool,
        text_content_override: Option<&str>,
    ) -> Vec<String> {
        let mut children: Vec<String> = Vec::new();

        if tag == "text" || tag == "span" {
            if let Some(placeholder) = text_content_override {
                children.push(format!("\"{placeholder}\""));
            } else {
                for child in node.children() {
                    match child.node_type() {
                        NodeType::Text => {
                            let text = collapse_whitespace(child.text().unwrap_or(""));
                            if !text.is_empty() {
                                children.push(double_quoted(&text));
                            }
                        }
                        NodeType::Element if child.tag_name().name() == "span" => {
                            children.push(self.emit_node(&child, 0, in_canvas));
                        }
                        _ => {}
                    }
                }
            }
        } else {
            for child in node.children().filter(|n| n.is_element()) {
                let s = self.emit_node(&child, level + 1, in_canvas);
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
            format!("{ind0}L.{method}({attrs}, [{}])", children[0])
        } else {
            let items: Vec<String> = children.iter().map(|c| format!("{ind1}{c},")).collect();
            format!("{ind0}L.{method}({attrs}, [\n{}\n{ind0}])", items.join("\n"))
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
            return format!("{ind0}L.{method}({attrs})");
        }
        let items: Vec<String> = children.iter().map(|c| format!("{c},")).collect();
        format!("{ind0}L.{method}({attrs}, [\n{}\n{ind0}])", items.join("\n"))
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

    fn emit_node(&self, node: &Node, level: usize, in_canvas: bool) -> String {
        let tag = node.tag_name().name();

        match tag {
            "lpdf"     => return self.emit_lpdf(node, level),
            "document" => return self.emit_document_node(node, level, None, None),
            "meta"     => return String::new(),
            _ => {}
        }

        let in_canvas = in_canvas || tag == "layer";
        // PHP uses same method names as JS (camelCase)
        let method    = js_method(tag, in_canvas);
        let ind0      = self.ind(level);
        let ind1      = self.ind(level + 1);

        let binding_comments = data_binding_comments(node, &ind0);
        let data_value = node.attribute("data-value");
        let attrs = php_attrs(node, tag, in_canvas, None);
        let text_content_override: Option<String> = data_value.map(|p| format!("{{{p}}}"));

        let children = self.collect_children(node, tag, level, in_canvas, text_content_override.as_deref());

        if children.is_empty() {
            format!("{binding_comments}{ind0}L::{method}({attrs})")
        } else if tag == "text" {
            format!("{binding_comments}{}", self.emit_text_call(&ind0, &ind1, method, &attrs, &children))
        } else if tag == "span" {
            format!("{binding_comments}{ind0}L::{method}({attrs}, [{}])", children.join(", "))
        } else {
            format!("{binding_comments}{}", self.emit_block_call(&ind0, method, &attrs, &children))
        }
    }

    fn emit_document_node(&self, node: &Node, level: usize, assets: Option<Node>, tokens: Option<Node>) -> String {
        let ind0   = self.ind(level);
        let method = "document";

        let meta_node = node.children().find(|n| n.is_element() && n.tag_name().name() == "meta");
        let meta_inline = meta_node.map(|m| {
            let meta_parts: Vec<String> = m
                .attributes()
                .map(|a| format!("{}: '{}'", to_camel_case(a.name()), a.value().replace('\'', "\\'")))
                .collect();
            if meta_parts.is_empty() {
                String::new()
            } else {
                format!("new DocumentMeta({})", meta_parts.join(", "))
            }
        });

        let mut doc_parts: Vec<String> = node
            .attributes()
            .map(|a| format!("{}: {}", to_camel_case(a.name()), php_attr_value(a.value())))
            .collect();

        if let Some(assets) = assets.and_then(|a| assets_expression(Target::Php, &a)) {
            doc_parts.push(Target::Php.member("assets", &assets));
        }
        if let Some(tokens) = tokens.and_then(|t| tokens_expression(Target::Php, &t)) {
            doc_parts.push(Target::Php.member("tokens", &tokens));
        }
        if let Some(meta_str) = meta_inline {
            if !meta_str.is_empty() {
                doc_parts.push(format!("meta: {meta_str}"));
            }
        }

        let attrs = if doc_parts.is_empty() {
            "NoAttr".into()
        } else {
            format!("new DocumentAttr({})", doc_parts.join(", "))
        };

        let mut children: Vec<String> = Vec::new();
        children.extend(
            node.children()
                .filter(|n| n.is_element() && n.tag_name().name() != "meta")
                .map(|n| self.emit_node(&n, level + 1, false))
                .filter(|s| !s.is_empty()),
        );

        self.emit_block_call(&ind0, method, &attrs, &children)
    }

    fn collect_children(
        &self,
        node: &Node,
        tag: &str,
        level: usize,
        in_canvas: bool,
        text_content_override: Option<&str>,
    ) -> Vec<String> {
        let mut children: Vec<String> = Vec::new();

        if tag == "text" || tag == "span" {
            if let Some(placeholder) = text_content_override {
                children.push(format!("'{placeholder}'"));
            } else {
                for child in node.children() {
                    match child.node_type() {
                        NodeType::Text => {
                            let text = collapse_whitespace(child.text().unwrap_or(""));
                            if !text.is_empty() {
                                children.push(single_quoted(&text));
                            }
                        }
                        NodeType::Element if child.tag_name().name() == "span" => {
                            children.push(self.emit_node(&child, 0, in_canvas));
                        }
                        _ => {}
                    }
                }
            }
        } else {
            for child in node.children().filter(|n| n.is_element()) {
                let s = self.emit_node(&child, level + 1, in_canvas);
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
            format!("{ind0}L::{method}({attrs}, [{}])", children[0])
        } else {
            let items: Vec<String> = children.iter().map(|c| format!("{ind1}{c},")).collect();
            format!("{ind0}L::{method}({attrs}, [\n{}\n{ind0}])", items.join("\n"))
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
            return format!("{ind0}L::{method}({attrs})");
        }
        let items: Vec<String> = children.iter().map(|c| format!("{c},")).collect();
        format!("{ind0}L::{method}({attrs}, [\n{}\n{ind0}])", items.join("\n"))
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

    fn emit_node(&self, node: &Node, level: usize, in_canvas: bool) -> String {
        let tag = node.tag_name().name();

        match tag {
            "lpdf"     => return self.emit_lpdf(node, level),
            "document" => return self.emit_document_node(node, level, None, None),
            "meta"     => return String::new(),
            _ => {}
        }

        let in_canvas = in_canvas || tag == "layer";
        // Python uses same method names as JS (snake_case only for multi-word: textAt → text_at)
        let method    = python_method(tag, in_canvas);
        let ind0      = self.ind(level);
        let ind1      = self.ind(level + 1);

        let binding_comments = data_binding_comments_hash(node, &ind0);
        let data_value = node.attribute("data-value");
        let attrs = python_attrs(node, tag, in_canvas, None);
        let text_content_override: Option<String> = data_value.map(|p| format!("{{{p}}}"));

        let children = self.collect_children(node, tag, level, in_canvas, text_content_override.as_deref());

        if children.is_empty() {
            format!("{binding_comments}{ind0}L.{method}({attrs})")
        } else if tag == "text" {
            format!("{binding_comments}{}", self.emit_text_call(&ind0, &ind1, method, &attrs, &children))
        } else if tag == "span" {
            format!("{binding_comments}{ind0}L.{method}({attrs}, [{}])", children.join(", "))
        } else {
            format!("{binding_comments}{}", self.emit_block_call(&ind0, method, &attrs, &children))
        }
    }

    fn emit_document_node(&self, node: &Node, level: usize, assets: Option<Node>, tokens: Option<Node>) -> String {
        let ind0   = self.ind(level);
        let method = "document";

        let meta_node = node.children().find(|n| n.is_element() && n.tag_name().name() == "meta");
        let meta_inline = meta_node.map(|m| {
            let meta_parts: Vec<String> = m
                .attributes()
                .map(|a| format!("{}='{}'", to_snake_case(a.name()), a.value().replace('\'', "\\'")))
                .collect();
            if meta_parts.is_empty() {
                String::new()
            } else {
                format!("DocumentMeta({})", meta_parts.join(", "))
            }
        });

        let mut doc_parts: Vec<String> = node
            .attributes()
            .map(|a| format!("{}={}", to_snake_case(a.name()), python_attr_value(a.value())))
            .collect();

        if let Some(assets) = assets.and_then(|a| assets_expression(Target::Python, &a)) {
            doc_parts.push(Target::Python.member("assets", &assets));
        }
        if let Some(tokens) = tokens.and_then(|t| tokens_expression(Target::Python, &t)) {
            doc_parts.push(Target::Python.member("tokens", &tokens));
        }
        if let Some(meta_str) = meta_inline {
            if !meta_str.is_empty() {
                doc_parts.push(format!("meta={meta_str}"));
            }
        }

        let attrs = if doc_parts.is_empty() {
            "NoAttr".into()
        } else {
            format!("DocumentAttr({})", doc_parts.join(", "))
        };

        let mut children: Vec<String> = Vec::new();
        children.extend(
            node.children()
                .filter(|n| n.is_element() && n.tag_name().name() != "meta")
                .map(|n| self.emit_node(&n, level + 1, false))
                .filter(|s| !s.is_empty()),
        );

        self.emit_block_call(&ind0, method, &attrs, &children)
    }

    fn collect_children(
        &self,
        node: &Node,
        tag: &str,
        level: usize,
        in_canvas: bool,
        text_content_override: Option<&str>,
    ) -> Vec<String> {
        let mut children: Vec<String> = Vec::new();

        if tag == "text" || tag == "span" {
            if let Some(placeholder) = text_content_override {
                children.push(format!("'{placeholder}'"));
            } else {
                for child in node.children() {
                    match child.node_type() {
                        NodeType::Text => {
                            let text = collapse_whitespace(child.text().unwrap_or(""));
                            if !text.is_empty() {
                                children.push(single_quoted(&text));
                            }
                        }
                        NodeType::Element if child.tag_name().name() == "span" => {
                            children.push(self.emit_node(&child, 0, in_canvas));
                        }
                        _ => {}
                    }
                }
            }
        } else {
            for child in node.children().filter(|n| n.is_element()) {
                let s = self.emit_node(&child, level + 1, in_canvas);
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
            format!("{ind0}L.{method}({attrs}, [{}])", children[0])
        } else {
            // Python: no trailing comma on last item
            let last = children.len() - 1;
            let items: Vec<String> = children
                .iter()
                .enumerate()
                .map(|(i, c)| if i < last { format!("{ind1}{c},") } else { format!("{ind1}{c}") })
                .collect();
            format!("{ind0}L.{method}({attrs}, [\n{}\n{ind0}])", items.join("\n"))
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
            return format!("{ind0}L.{method}({attrs})");
        }
        // Python: no trailing comma on last item
        let last = children.len() - 1;
        let items: Vec<String> = children
            .iter()
            .enumerate()
            .map(|(i, c)| if i < last { format!("{c},") } else { c.clone() })
            .collect();
        format!("{ind0}L.{method}({attrs}, [\n{}\n{ind0}])", items.join("\n"))
    }
}

/// Return the Python method name for an XML element tag, given context.
fn python_method(tag: &str, in_canvas: bool) -> &'static str {
    if in_canvas {
        return match tag {
            "text" => "text_at",
            "img"  => "img_at",
            other  => python_layout_method(other),
        };
    }
    python_layout_method(tag)
}

fn python_layout_method(tag: &str) -> &'static str {
    match tag {
        "document" => "document",
        "section"  => "section",
        "layout"   => "layout",
        "canvas"   => "canvas",
        "layer"    => "layer",
        "tokens"   => "tokens",
        "stack"    => "stack",
        "flank"    => "flank",
        "split"    => "split",
        "cluster"  => "cluster",
        "grid"     => "grid",
        "frame"    => "frame",
        "link"     => "link",
        "text"     => "text",
        "img"      => "img",
        "divider"  => "divider",
        "table"    => "table",
        "thead"    => "thead",
        "tr"       => "tr",
        "td"       => "td",
        "barcode"  => "barcode",
        "field"    => "field",
        "region"   => "region",
        "span"     => "span",
        "rect"     => "rect",
        "circle"   => "circle",
        "ellipse"  => "ellipse",
        "line"     => "line",
        "path"     => "path",
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

    #[test]
    fn test_attr_value_is_always_a_string() {
        // The SDKs take every attribute as a string, so a boolean is the string "true".
        assert_eq!(js_attr_value("true"),     "'true'");
        assert_eq!(js_attr_value("false"),    "'false'");
        assert_eq!(js_attr_value("a4"),       "'a4'");
        assert_eq!(php_attr_value("true"),    "'true'");
        assert_eq!(python_attr_value("true"), "'true'");
        assert_eq!(dotnet_attr_value("true"), "\"true\"");
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
  <document size="a4" margin="48pt">
    <section>
      <layout>
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
        <stack>
          <text>Hi</text>
        </stack>
      </layout>
    </section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "js".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("L.stack(NoAttr, ["));
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
        assert!(out.contains("'Hello'"));
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
    fn test_canvas_text_and_image_have_their_own_attribute_classes() {
        let xml = r##"<lpdf version="1"><document><section><canvas><layer>
            <text x="10pt" y="20pt">Hi</text>
            <img name="logo" x="0pt" y="0pt" w="10pt" h="10pt"/>
        </layer></canvas></section></document></lpdf>"##;
        for target in ["php", "python"] {
            let opts = CodegenOptions { target: target.into(), indent: 4 };
            let out  = codegen(xml, &opts).unwrap();
            assert!(out.contains("CanvasTextAttr("), "{target}: {out}");
            assert!(out.contains("CanvasImgAttr("), "{target}: {out}");
        }
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
  <document size="a4" margin="48pt">
    <section>
      <layout>
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
        assert!(out.contains("var pdf = await engine.Render(doc);"));
    }

    #[test]
    fn test_dotnet_no_attr() {
        let xml = r#"<?xml version="1.0"?>
<lpdf version="1">
  <document>
    <section>
      <layout>
        <stack>
          <text>Hi</text>
        </stack>
      </layout>
    </section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "dotnet".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("L.Stack(NoAttr, ["));
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
    fn test_dotnet_pascal_case() {
        assert_eq!(to_pascal_case("font-size"),    "FontSize");
        assert_eq!(to_pascal_case("stroke-width"), "StrokeWidth");
        assert_eq!(to_pascal_case("hrt"),           "Hrt");
        assert_eq!(to_pascal_case("xs"),            "Xs");
        assert_eq!(to_pascal_case("text-size"),    "TextSize");
    }

    // ── PHP tests ─────────────────────────────────────────────────────────────

    #[test]
    fn test_php_minimal_document() {
        let xml = r#"<?xml version="1.0"?>
<lpdf version="1">
  <document size="a4" margin="48pt">
    <section>
      <layout>
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
        <stack>
          <text>Hi</text>
        </stack>
      </layout>
    </section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "php".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("L::stack(NoAttr, ["));
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
  <document size="a4" margin="48pt">
    <section>
      <layout>
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
        <stack>
          <text>Hi</text>
        </stack>
      </layout>
    </section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "python".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(out.contains("L.stack(NoAttr, ["));
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
    fn test_python_bool_attr() {
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
        assert!(out.contains("bold='true'"));
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
        <stack>
          <text>A</text>
          <text>B</text>
        </stack>
      </layout>
    </section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "python".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        // Last child of stack must not have trailing comma
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
        let xml = r#"<stack><text>A</text><text>B</text></stack>"#;
        let opts = CodegenOptions { target: "js".into(), indent: 4 };
        let out  = codegen_fragment(xml, &opts).unwrap();
        assert!(out.contains("L.stack("));
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
}
