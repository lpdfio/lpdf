//! # codegen
//!
//! XML → SDK code generator. Walks an LPDF XML document and emits idiomatic
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

/// Generate SDK source code from an LPDF XML string.
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

/// Emit a boolean-aware attribute value string.
fn js_attr_value(val: &str) -> String {
    match val.to_ascii_lowercase().as_str() {
        "true"  => "true".into(),
        "false" => "false".into(),
        _       => format!("'{}'", val.replace('\'', "\\'")),
    }
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

/// Emit a PHP single-quoted string or boolean literal for an attribute value.
fn php_attr_value(val: &str) -> String {
    match val.to_ascii_lowercase().as_str() {
        "true"  => "true".into(),
        "false" => "false".into(),
        _       => format!("'{}'", val.replace('\\', "\\\\").replace('\'', "\\'")),
    }
}

/// Emit a PHP named-arg constructor call for the attributes of a node.
///
/// Returns `"NoAttr"` when there are no attributes.
fn php_attrs(node: &Node, tag: &str, extra_attrs: Option<&[(&str, String)]>) -> String {
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
        let class = php_attr_class(tag);
        format!("new {class}({})", parts.join(", "))
    }
}

/// Return the `{Element}Attr` PHP class name for a given XML element tag.
fn php_attr_class(tag: &str) -> String {
    let pascal = to_pascal_case(tag);
    format!("{pascal}Attr")
}

/// Emit a Python keyword-arg constructor for the attributes of a node.
///
/// Returns `"NoAttr"` when there are no attributes.
fn python_attrs(node: &Node, tag: &str, extra_attrs: Option<&[(&str, String)]>) -> String {
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
        let class = python_attr_class(tag);
        format!("{class}({})", parts.join(", "))
    }
}

/// Return the `{Element}Attr` Python class name for a given XML element tag.
fn python_attr_class(tag: &str) -> String {
    let pascal = to_pascal_case(tag);
    format!("{pascal}Attr")
}

/// Emit a Python string or boolean literal for an attribute value.
fn python_attr_value(val: &str) -> String {
    match val.to_ascii_lowercase().as_str() {
        "true"  => "True".into(),
        "false" => "False".into(),
        _       => format!("'{}'", val.replace('\\', "\\\\").replace('\'', "\\'")),
    }
}

/// Emit a C# double-quoted string or boolean literal for an attribute value.
fn dotnet_attr_value(val: &str) -> String {
    match val.to_ascii_lowercase().as_str() {
        "true"  => "true".into(),
        "false" => "false".into(),
        _       => format!("\"{}\"", val.replace('"', "\\\"")),
    }
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

        // Collect top-level children
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

        // Imports
        out.push_str("import { L, NoAttr } from '@lpdfio/lpdf'\n");
        out.push('\n');

        // Engine
        out.push_str("const engine = L.engine()\n");
        out.push('\n');

        // Assets
        if let Some(assets) = assets_node {
            for child in assets.children().filter(|n| n.is_element()) {
                match child.tag_name().name() {
                    "font" => {
                        // Skip built-in fonts (have `core` attribute)
                        if child.has_attribute("core") {
                            continue;
                        }
                        let name = child.attribute("name").unwrap_or("");
                        let src  = child.attribute("src").unwrap_or("");
                        out.push_str(&format!(
                            "await engine.loadFont('{name}', readFileSync('{src}'))\n"
                        ));
                    }
                    "image" => {
                        let name = child.attribute("name").unwrap_or("");
                        let src  = child.attribute("src").unwrap_or("");
                        out.push_str(&format!(
                            "await engine.loadImage('{name}', readFileSync('{src}'))\n"
                        ));
                    }
                    _ => {}
                }
            }
            out.push('\n');
        }

        // Tokens variable
        if let Some(tok) = tokens_node {
            let tok_expr = self.emit_tokens_call(&tok, 0);
            out.push_str(&format!("const tokens = {tok_expr}\n"));
            out.push('\n');
        }

        // Document
        if let Some(doc_node) = document_node {
            let tokens_var = if tokens_node.is_some() { Some("tokens") } else { None };
            let doc_expr = self.emit_document_node(&doc_node, 0, tokens_var);
            out.push_str(&format!("const doc = {doc_expr}\n"));
        }

        out.push('\n');
        out.push_str("const pdf = await engine.render(doc)\n");

        out
    }

    // ── Tokens ────────────────────────────────────────────────────────────────

    fn emit_tokens_call(&self, node: &Node, level: usize) -> String {
        let ind0 = self.ind(level);

        // Build attrs object from child scale elements and colors
        let mut parts: Vec<String> = Vec::new();

        for child in node.children().filter(|n| n.is_element()) {
            let tag = child.tag_name().name();
            if tag == "colors" {
                // Collect <color> children into a plain map
                let color_parts: Vec<String> = child
                    .children()
                    .filter(|n| n.is_element() && n.tag_name().name() == "color")
                    .map(|c| {
                        let name  = c.attribute("name").unwrap_or("");
                        let value = c.attribute("value").unwrap_or("");
                        // Quote color names that are not valid JS identifiers (e.g. "surface-alt")
                        let key = if name.chars().all(|ch| ch.is_alphanumeric() || ch == '_') {
                            name.to_string()
                        } else {
                            format!("'{name}'")
                        };
                        format!("{key}: '{value}'")
                    })
                    .collect();
                if !color_parts.is_empty() {
                    parts.push(format!("colors: {{ {} }}", color_parts.join(", ")));
                }
            } else {
                // Scale element — emit as nested object with all scale attrs
                let attr_parts: Vec<String> = child
                    .attributes()
                    .map(|a| format!("{}: '{}'", a.name(), a.value()))
                    .collect();
                if !attr_parts.is_empty() {
                    let key = to_camel_case(tag); // "text-size" → "textSize"
                    parts.push(format!("{key}: {{ {} }}", attr_parts.join(", ")));
                }
            }
        }

        let attrs = if parts.is_empty() {
            "NoAttr".into()
        } else {
            format!("{{ {} }}", parts.join(", "))
        };

        format!("{ind0}L.tokens({attrs})")
    }

    // ── Generic node emitter ──────────────────────────────────────────────────

    fn emit_node(&self, node: &Node, level: usize, in_canvas: bool) -> String {
        let tag = node.tag_name().name();

        // Special cases
        match tag {
            "document" => return self.emit_document_node(node, level, None),
            "meta"     => return String::new(), // folded into document
            "tokens"   => return self.emit_tokens_call(node, level),
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

    fn emit_document_node(&self, node: &Node, level: usize, tokens_var: Option<&str>) -> String {
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

        // Emit children (skip meta); prepend tokens variable reference if present
        let mut children: Vec<String> = Vec::new();
        if let Some(var) = tokens_var {
            children.push(format!("{}{var}", self.ind(level + 1)));
        }
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
                            let raw = child.text().unwrap_or("");
                            // Keep internal spaces (e.g. "Hello ") but skip
                            // pure-whitespace-only nodes (formatting indentation).
                            if !raw.trim().is_empty() {
                                children.push(format!("'{}'", raw.replace('\'', "\\'")));
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

        let mut out = String::new();

        // Using
        out.push_str("using Lpdf;\n");
        out.push('\n');

        // Engine
        out.push_str("var engine = L.Engine();\n");
        out.push('\n');

        // Assets
        if let Some(assets) = assets_node {
            for child in assets.children().filter(|n| n.is_element()) {
                match child.tag_name().name() {
                    "font" => {
                        if child.has_attribute("core") {
                            continue;
                        }
                        let name = child.attribute("name").unwrap_or("");
                        let src  = child.attribute("src").unwrap_or("");
                        out.push_str(&format!(
                            "await engine.LoadFont(\"{name}\", File.ReadAllBytes(\"{src}\"));\n"
                        ));
                    }
                    "image" => {
                        let name = child.attribute("name").unwrap_or("");
                        let src  = child.attribute("src").unwrap_or("");
                        out.push_str(&format!(
                            "await engine.LoadImage(\"{name}\", File.ReadAllBytes(\"{src}\"));\n"
                        ));
                    }
                    _ => {}
                }
            }
            out.push('\n');
        }

        // Tokens variable
        if let Some(tok) = tokens_node {
            let tok_expr = self.emit_tokens_call(&tok, 0);
            out.push_str(&format!("var tokens = {tok_expr};\n"));
            out.push('\n');
        }

        // Document
        if let Some(doc_node) = document_node {
            let tokens_var = if tokens_node.is_some() { Some("tokens") } else { None };
            let doc_expr = self.emit_document_node(&doc_node, 0, tokens_var);
            out.push_str(&format!("var doc = {doc_expr};\n"));
        }

        out.push('\n');
        out.push_str("var pdf = await engine.Render(doc);\n");

        out
    }

    fn emit_tokens_call(&self, node: &Node, level: usize) -> String {
        let ind0 = self.ind(level);
        let mut parts: Vec<String> = Vec::new();

        for child in node.children().filter(|n| n.is_element()) {
            let tag = child.tag_name().name();
            if tag == "colors" {
                let color_parts: Vec<String> = child
                    .children()
                    .filter(|n| n.is_element() && n.tag_name().name() == "color")
                    .map(|c| {
                        let name  = c.attribute("name").unwrap_or("");
                        let value = c.attribute("value").unwrap_or("");
                        format!("[\"{name}\"] = \"{value}\"")
                    })
                    .collect();
                if !color_parts.is_empty() {
                    parts.push(format!("Colors = new() {{ {} }}", color_parts.join(", ")));
                }
            } else {
                let attr_parts: Vec<String> = child
                    .attributes()
                    .map(|a| format!("{} = \"{}\"", to_pascal_case(a.name()), a.value()))
                    .collect();
                if !attr_parts.is_empty() {
                    let key = to_pascal_case(tag);
                    parts.push(format!("{key} = new() {{ {} }}", attr_parts.join(", ")));
                }
            }
        }

        let attrs = if parts.is_empty() {
            "NoAttr".into()
        } else {
            format!("new() {{ {} }}", parts.join(", "))
        };

        format!("{ind0}L.Tokens({attrs})")
    }

    fn emit_node(&self, node: &Node, level: usize, in_canvas: bool) -> String {
        let tag = node.tag_name().name();

        match tag {
            "document" => return self.emit_document_node(node, level, None),
            "meta"     => return String::new(),
            "tokens"   => return self.emit_tokens_call(node, level),
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

    fn emit_document_node(&self, node: &Node, level: usize, tokens_var: Option<&str>) -> String {
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
        if let Some(var) = tokens_var {
            children.push(format!("{}{var}", self.ind(level + 1)));
        }
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
                            let raw = child.text().unwrap_or("");
                            if !raw.trim().is_empty() {
                                children.push(format!("\"{}\"", raw.replace('"', "\\\"")));
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

        let mut out = String::new();

        out.push_str("<?php\n\n");
        out.push_str("require_once 'vendor/autoload.php';\n\n");
        out.push_str("use Lpdf\\L;\n");
        out.push_str("use const Lpdf\\NoAttr;\n");
        out.push('\n');

        out.push_str("$engine = L::engine();\n");
        out.push('\n');

        if let Some(assets) = assets_node {
            for child in assets.children().filter(|n| n.is_element()) {
                match child.tag_name().name() {
                    "font" => {
                        if child.has_attribute("core") {
                            continue;
                        }
                        let name = child.attribute("name").unwrap_or("");
                        let src  = child.attribute("src").unwrap_or("");
                        out.push_str(&format!(
                            "$engine->loadFont('{name}', file_get_contents('{src}'));\n"
                        ));
                    }
                    "image" => {
                        let name = child.attribute("name").unwrap_or("");
                        let src  = child.attribute("src").unwrap_or("");
                        out.push_str(&format!(
                            "$engine->loadImage('{name}', file_get_contents('{src}'));\n"
                        ));
                    }
                    _ => {}
                }
            }
            out.push('\n');
        }

        if let Some(tok) = tokens_node {
            let tok_expr = self.emit_tokens_call(&tok, 0);
            out.push_str(&format!("$tokens = {tok_expr};\n"));
            out.push('\n');
        }

        if let Some(doc_node) = document_node {
            let tokens_var = if tokens_node.is_some() { Some("tokens") } else { None };
            let doc_expr = self.emit_document_node(&doc_node, 0, tokens_var);
            out.push_str(&format!("$doc = {doc_expr};\n"));
        }

        out.push('\n');
        out.push_str("$pdf = $engine->render($doc);\n");

        out
    }

    fn emit_tokens_call(&self, node: &Node, level: usize) -> String {
        let ind0 = self.ind(level);
        let mut parts: Vec<String> = Vec::new();

        for child in node.children().filter(|n| n.is_element()) {
            let tag = child.tag_name().name();
            if tag == "colors" {
                let color_parts: Vec<String> = child
                    .children()
                    .filter(|n| n.is_element() && n.tag_name().name() == "color")
                    .map(|c| {
                        let name  = c.attribute("name").unwrap_or("");
                        let value = c.attribute("value").unwrap_or("");
                        format!("'{name}' => '{value}'")
                    })
                    .collect();
                if !color_parts.is_empty() {
                    parts.push(format!("colors: [{}]", color_parts.join(", ")));
                }
            } else {
                let attr_parts: Vec<String> = child
                    .attributes()
                    .map(|a| format!("{}: '{}'", a.name(), a.value()))
                    .collect();
                if !attr_parts.is_empty() {
                    let key = to_camel_case(tag);
                    // PHP uses arrays for scale objects
                    parts.push(format!("{key}: [{}]", attr_parts.join(", ")));
                }
            }
        }

        let attrs = if parts.is_empty() {
            "NoAttr".into()
        } else {
            format!("new TokensAttr({})", parts.join(", "))
        };

        format!("{ind0}L::tokens({attrs})")
    }

    fn emit_node(&self, node: &Node, level: usize, in_canvas: bool) -> String {
        let tag = node.tag_name().name();

        match tag {
            "document" => return self.emit_document_node(node, level, None),
            "meta"     => return String::new(),
            "tokens"   => return self.emit_tokens_call(node, level),
            _ => {}
        }

        let in_canvas = in_canvas || tag == "layer";
        // PHP uses same method names as JS (camelCase)
        let method    = js_method(tag, in_canvas);
        let ind0      = self.ind(level);
        let ind1      = self.ind(level + 1);

        let binding_comments = data_binding_comments(node, &ind0);
        let data_value = node.attribute("data-value");
        let attrs = php_attrs(node, tag, None);
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

    fn emit_document_node(&self, node: &Node, level: usize, tokens_var: Option<&str>) -> String {
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
                format!("new LpdfMeta({})", meta_parts.join(", "))
            }
        });

        let mut doc_parts: Vec<String> = node
            .attributes()
            .map(|a| format!("{}: {}", to_camel_case(a.name()), php_attr_value(a.value())))
            .collect();

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
        if let Some(var) = tokens_var {
            children.push(format!("{}${var}", self.ind(level + 1)));
        }
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
                            let raw = child.text().unwrap_or("");
                            if !raw.trim().is_empty() {
                                children.push(format!("'{}'", raw.replace('\'', "\\'")));
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

        let mut out = String::new();

        out.push_str("from lpdf import L, NoAttr\n");
        out.push('\n');

        out.push_str("engine = L.engine()\n");
        out.push('\n');

        if let Some(assets) = assets_node {
            for child in assets.children().filter(|n| n.is_element()) {
                match child.tag_name().name() {
                    "font" => {
                        if child.has_attribute("core") {
                            continue;
                        }
                        let name = child.attribute("name").unwrap_or("");
                        let src  = child.attribute("src").unwrap_or("");
                        out.push_str(&format!(
                            "engine.load_font('{name}', open('{src}', 'rb').read())\n"
                        ));
                    }
                    "image" => {
                        let name = child.attribute("name").unwrap_or("");
                        let src  = child.attribute("src").unwrap_or("");
                        out.push_str(&format!(
                            "engine.load_image('{name}', open('{src}', 'rb').read())\n"
                        ));
                    }
                    _ => {}
                }
            }
            out.push('\n');
        }

        if let Some(tok) = tokens_node {
            let tok_expr = self.emit_tokens_call(&tok, 0);
            out.push_str(&format!("tokens = {tok_expr}\n"));
            out.push('\n');
        }

        if let Some(doc_node) = document_node {
            let tokens_var = if tokens_node.is_some() { Some("tokens") } else { None };
            let doc_expr = self.emit_document_node(&doc_node, 0, tokens_var);
            out.push_str(&format!("doc = {doc_expr}\n"));
        }

        out.push('\n');
        out.push_str("pdf = engine.render(doc)\n");

        out
    }

    fn emit_tokens_call(&self, node: &Node, level: usize) -> String {
        let ind0 = self.ind(level);
        let mut parts: Vec<String> = Vec::new();

        for child in node.children().filter(|n| n.is_element()) {
            let tag = child.tag_name().name();
            if tag == "colors" {
                let color_parts: Vec<String> = child
                    .children()
                    .filter(|n| n.is_element() && n.tag_name().name() == "color")
                    .map(|c| {
                        let name  = c.attribute("name").unwrap_or("");
                        let value = c.attribute("value").unwrap_or("");
                        format!("'{name}': '{value}'")
                    })
                    .collect();
                if !color_parts.is_empty() {
                    parts.push(format!("colors={{{}}}", color_parts.join(", ")));
                }
            } else {
                let attr_parts: Vec<String> = child
                    .attributes()
                    .map(|a| format!("{}: '{}'", a.name(), a.value()))
                    .collect();
                if !attr_parts.is_empty() {
                    let key = to_snake_case(tag);
                    parts.push(format!("{key}={{{}}}", attr_parts.join(", ")));
                }
            }
        }

        let attrs = if parts.is_empty() {
            "NoAttr".into()
        } else {
            format!("TokensAttr({})", parts.join(", "))
        };

        format!("{ind0}L.tokens({attrs})")
    }

    fn emit_node(&self, node: &Node, level: usize, in_canvas: bool) -> String {
        let tag = node.tag_name().name();

        match tag {
            "document" => return self.emit_document_node(node, level, None),
            "meta"     => return String::new(),
            "tokens"   => return self.emit_tokens_call(node, level),
            _ => {}
        }

        let in_canvas = in_canvas || tag == "layer";
        // Python uses same method names as JS (snake_case only for multi-word: textAt → text_at)
        let method    = python_method(tag, in_canvas);
        let ind0      = self.ind(level);
        let ind1      = self.ind(level + 1);

        let binding_comments = data_binding_comments_hash(node, &ind0);
        let data_value = node.attribute("data-value");
        let attrs = python_attrs(node, tag, None);
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

    fn emit_document_node(&self, node: &Node, level: usize, tokens_var: Option<&str>) -> String {
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
                format!("LpdfMeta({})", meta_parts.join(", "))
            }
        });

        let mut doc_parts: Vec<String> = node
            .attributes()
            .map(|a| format!("{}={}", to_snake_case(a.name()), python_attr_value(a.value())))
            .collect();

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
        if let Some(var) = tokens_var {
            children.push(format!("{}{var}", self.ind(level + 1)));
        }
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
                            let raw = child.text().unwrap_or("");
                            if !raw.trim().is_empty() {
                                children.push(format!("'{}'", raw.replace('\'', "\\'")));
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
    fn test_attr_value_bool() {
        assert_eq!(js_attr_value("true"),  "true");
        assert_eq!(js_attr_value("false"), "false");
        assert_eq!(js_attr_value("True"),  "true");
        assert_eq!(js_attr_value("a4"),    "'a4'");
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
    fn test_tokens() {
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
        assert!(out.contains("const tokens = L.tokens("));
        assert!(out.contains("colors: { primary: '#1763cf' }"));
        // tokens variable must be referenced as first child of document
        assert!(out.contains("const doc = L.document("));
        assert!(out.contains("    tokens,"));
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
    fn test_assets_load_font_skip_core() {
        let xml = r#"<?xml version="1.0"?>
<lpdf version="1">
  <assets>
    <font name="heading" core="true"/>
    <font name="body" src="./fonts/Body.ttf"/>
    <image name="logo" src="./assets/logo.png"/>
  </assets>
  <document>
    <section><layout><text>x</text></layout></section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "js".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        // core font must be skipped
        assert!(!out.contains("loadFont('heading'"));
        assert!(out.contains("loadFont('body'"));
        assert!(out.contains("loadImage('logo'"));
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
        assert!(out.contains("'Hello '"));
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
    fn test_dotnet_assets() {
        let xml = r#"<?xml version="1.0"?>
<lpdf version="1">
  <assets>
    <font name="heading" core="true"/>
    <font name="body" src="./fonts/Body.ttf"/>
    <image name="logo" src="./assets/logo.png"/>
  </assets>
  <document>
    <section><layout><text>x</text></layout></section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "dotnet".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(!out.contains("LoadFont(\"heading\""));
        assert!(out.contains("await engine.LoadFont(\"body\", File.ReadAllBytes(\"./fonts/Body.ttf\"));"));
        assert!(out.contains("await engine.LoadImage(\"logo\", File.ReadAllBytes(\"./assets/logo.png\"));"));
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
    fn test_dotnet_tokens() {
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
        assert!(out.contains("var tokens = L.Tokens("));
        assert!(out.contains("Colors = new() { [\"primary\"] = \"#1763cf\" }"));
        assert!(out.contains("var doc = L.Document("));
        assert!(out.contains("    tokens,"));
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
        assert!(out.contains("meta: new LpdfMeta(title: 'My Doc', author: 'Alice')"));
        assert!(!out.contains("L::meta("));
    }

    #[test]
    fn test_php_assets() {
        let xml = r#"<?xml version="1.0"?>
<lpdf version="1">
  <assets>
    <font name="heading" core="true"/>
    <font name="body" src="./fonts/Body.ttf"/>
    <image name="logo" src="./assets/logo.png"/>
  </assets>
  <document>
    <section><layout><text>x</text></layout></section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "php".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(!out.contains("loadFont('heading'"));
        assert!(out.contains("$engine->loadFont('body', file_get_contents('./fonts/Body.ttf'));"));
        assert!(out.contains("$engine->loadImage('logo', file_get_contents('./assets/logo.png'));"));
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
    fn test_php_tokens() {
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
        assert!(out.contains("$tokens = L::tokens("));
        assert!(out.contains("'primary' => '#1763cf'"));
        assert!(out.contains("$doc = L::document("));
        assert!(out.contains("    $tokens,"));
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
        assert!(out.contains("meta=LpdfMeta(title='My Doc', author='Alice')"));
        assert!(!out.contains("L.meta("));
    }

    #[test]
    fn test_python_assets() {
        let xml = r#"<?xml version="1.0"?>
<lpdf version="1">
  <assets>
    <font name="heading" core="true"/>
    <font name="body" src="./fonts/Body.ttf"/>
    <image name="logo" src="./assets/logo.png"/>
  </assets>
  <document>
    <section><layout><text>x</text></layout></section>
  </document>
</lpdf>"#;
        let opts = CodegenOptions { target: "python".into(), indent: 4 };
        let out  = codegen(xml, &opts).unwrap();
        assert!(!out.contains("load_font('heading'"));
        assert!(out.contains("engine.load_font('body', open('./fonts/Body.ttf', 'rb').read())"));
        assert!(out.contains("engine.load_image('logo', open('./assets/logo.png', 'rb').read())"));
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
        assert!(out.contains("bold=True"));
    }

    #[test]
    fn test_python_tokens() {
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
        assert!(out.contains("tokens = L.tokens("));
        assert!(out.contains("'primary': '#1763cf'"));
        assert!(out.contains("doc = L.document("));
        assert!(out.contains("    tokens"));
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
}
