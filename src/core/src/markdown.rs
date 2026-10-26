//! Markdown as the content of a section.
//!
//! A `<markdown>` is read into one `NodeKind::Markdown` node that holds a `MarkdownSpec`: the rules that say how
//! each part looks, the text, and what is needed to read it. `expand` turns the node into the nodes of the layout
//! once the text is known: in `parse` for a text written inline, in `data::apply` for one that comes from the data.
//!
//! The expansion writes the Markdown as Lpdf XML, one element per block, with the attributes of the rules, and reads
//! that XML with the parser every document goes through. A rule so takes the attributes of the element it becomes,
//! and tokens, fonts and errors work as they do anywhere else.

use std::collections::{BTreeMap, HashMap};
use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, MetadataBlockKind, Options, Parser, Tag, TagEnd};
use serde_json::Value;

use crate::parse::{
    elems, layout_margin, node_loc, parse_node, resolve_font_alias, resolve_parsed_node, Attrs, JsonAttrs, LayoutChild,
    Node, NodeKind, PendingBookmarks, Section, SectionChild, TextDefaults,
};
use crate::tokens::{FontDef, Tokens};

/// The default rules: what a `<markdown>` with no styles looks like.
const DEFAULT_STYLES: &str = include_str!("../assets/markdown-styles.xml");

/// The default template that wraps a `.md` file for the command line and the extension.
const DEFAULT_TEMPLATE: &str = include_str!("../assets/markdown-template.xml");

/// The rules a `<markdown-styles>` may hold, each with the attributes it may set.
const RULES: &[(&str, &[&str])] = &[
    ("h1", TEXT_RULE), ("h2", TEXT_RULE), ("h3", TEXT_RULE), ("h4", TEXT_RULE), ("h5", TEXT_RULE), ("h6", TEXT_RULE),
    ("p", TEXT_RULE),
    ("code", &["font", "font-size", "color", "line-height", "padding", "background", "border", "radius", "tab-size"]),
    ("quote", &[
        "font", "font-size", "bold", "color", "align", "line-height", "padding", "background", "border", "radius",
        "bar-color", "bar-width",
    ]),
    ("li", &["font", "font-size", "color", "gap", "padding", "background", "border", "indent", "bullet"]),
    ("a", &["font", "color", "bold", "underline"]),
    ("strong", &["font", "color", "bold"]),
    ("em", &["font", "color"]),
    ("del", &["font", "color", "strike"]),
    ("table", &["border", "stripe", "gap", "padding", "background"]),
    ("th", CELL_RULE), ("td", CELL_RULE),
];

const TEXT_RULE: &[&str] = &[
    "font", "font-size", "bold", "color", "align", "line-height", "padding", "background", "border", "radius",
];
const CELL_RULE: &[&str] = &["font", "font-size", "bold", "color", "align", "padding", "background", "border"];

/// The attributes of a rule that belong to the text inside a block, not to the box around it.
const TEXT_ATTRS: [&str; 6] = ["font", "font-size", "bold", "color", "align", "line-height"];

/// The attributes of a rule that belong to the box around a block of text.
const BOX_ATTRS: [&str; 4] = ["padding", "background", "border", "radius"];

/// The space between blocks when `gap` says nothing.
const DEFAULT_GAP: &str = "l";

/// How many levels of headings are bookmarks when `bookmarks` says nothing.
const DEFAULT_BOOKMARK_LEVELS: u8 = 6;

// ── Rules ─────────────────────────────────────────────────────────────────────

/// What a rule sets: attribute names and values, in name order.
type Rule = BTreeMap<String, String>;

/// The rules of a `<markdown>`: the default rules, over which a file and the inline rules are laid, attribute by
/// attribute, so that a style that is not set stays what it was.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Rules(BTreeMap<String, Rule>);

impl Rules {
    /// The rules every `<markdown>` starts from.
    fn defaults() -> Result<Rules, String> {
        let doc = roxmltree::Document::parse(DEFAULT_STYLES).map_err(|e| format!("default markdown styles: {e}"))?;
        Rules::from_elem(&doc.root_element())
    }

    /// The rules written in a `<markdown-styles>`.
    fn from_elem(styles: &roxmltree::Node) -> Result<Rules, String> {
        let mut rules = Rules::default();
        for rule in elems(styles) {
            let attrs: Vec<(String, String)> =
                rule.attributes().map(|a| (a.name().to_string(), a.value().to_string())).collect();
            rules.set(rule.tag_name().name(), attrs).map_err(|e| format!("<{}>{}: {e}", rule.tag_name().name(), node_loc(&rule)))?;
        }
        Ok(rules)
    }

    /// The rules written in a `markdown-styles` node of a kit tree.
    fn from_tree(styles: &Value) -> Result<Rules, String> {
        let mut rules = Rules::default();
        for rule in styles.get("nodes").and_then(Value::as_array).into_iter().flatten() {
            let name = rule.get("type").and_then(Value::as_str).unwrap_or("?");
            let attrs: Vec<(String, String)> = rule
                .get("attrs")
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
                .map(|(key, value)| (key.clone(), value.as_str().map(str::to_string).unwrap_or_else(|| value.to_string())))
                .collect();
            rules.set(name, attrs).map_err(|e| format!("markdown-styles: {name}: {e}"))?;
        }
        Ok(rules)
    }

    /// Set the attributes of the rule `name`, which must be one a `<markdown-styles>` may hold.
    fn set(&mut self, name: &str, attrs: Vec<(String, String)>) -> Result<(), String> {
        let allowed = RULES
            .iter()
            .find(|(rule, _)| *rule == name)
            .map(|(_, allowed)| *allowed)
            .ok_or_else(|| {
                let names: Vec<&str> = RULES.iter().map(|(rule, _)| *rule).collect();
                format!("unknown rule — expected {}", names.join(", "))
            })?;
        let rule = self.0.entry(name.to_string()).or_default();
        for (key, value) in attrs {
            if !allowed.contains(&key.as_str()) {
                return Err(format!("unknown attribute '{key}' — expected {}", allowed.join(", ")));
            }
            rule.insert(key, value);
        }
        Ok(())
    }

    /// Lay `over` on these rules: its attributes replace the ones with the same name.
    fn lay(&mut self, over: Rules) {
        for (name, rule) in over.0 {
            self.0.entry(name).or_default().extend(rule);
        }
    }

    fn rule(&self, name: &str) -> Rule {
        self.0.get(name).cloned().unwrap_or_default()
    }

}

// ── The spec a `<markdown>` leaves in the tree ────────────────────────────────

/// What the expansion needs, kept in the node until the text is known.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkdownSpec {
    rules:       Rules,
    /// The Markdown written inline, with its common indent removed. With `data_path`, what a document with no data
    /// shows.
    text:        String,
    /// Where in the data the Markdown is.
    data_path:   Option<String>,
    /// How many levels of headings are bookmarks.
    bookmarks:   u8,
    /// The space between blocks, as written: a token or a length.
    gap:         String,
    tokens:      Tokens,
    fonts:       HashMap<String, FontDef>,
    /// The images the document declares: asset name to registry key.
    images:      HashMap<String, String>,
    /// The `src` of each image the document declares, to its asset name.
    image_srcs:  HashMap<String, String>,
    defaults:    TextDefaults,
}

/// What the reading of a document keeps for its `<markdown>` elements.
#[derive(Debug, Clone, Default)]
pub(crate) struct Assets {
    /// The `src` of each image the document declares, to its asset name.
    pub image_srcs:   HashMap<String, String>,
    /// The text of the files that a `<markdown-styles src>` names, loaded by whoever calls the engine.
    pub styles_files: HashMap<String, String>,
}

/// What a section hands to the reading of its `<markdown>`.
pub(crate) struct SectionContext<'a> {
    pub tokens:      &'a Tokens,
    pub fonts:       &'a HashMap<String, FontDef>,
    pub images:      &'a HashMap<String, String>,
    pub image_srcs:  &'a HashMap<String, String>,
    pub defaults:    &'a TextDefaults,
    /// The text of the files that a `<markdown-styles src>` names, loaded by whoever calls the engine.
    pub styles_files: &'a HashMap<String, String>,
}

/// What a `<markdown>` says, read from XML or from a kit tree.
struct Declaration {
    styles:    Option<(Option<String>, Rules)>,
    data_path: Option<String>,
    text:      String,
}

/// Read a `<markdown>` of an XML document into the child of the section that holds it.
pub(crate) fn parse_elem(elem: &roxmltree::Node, cx: &SectionContext) -> Result<SectionChild, String> {
    let at = |e: String| format!("<markdown>{}: {e}", node_loc(elem));
    let mut declaration = Declaration { styles: None, data_path: None, text: String::new() };
    let mut seen_content = false;
    for child in elems(elem) {
        match child.tag_name().name() {
            "markdown-styles" => {
                if declaration.styles.is_some() {
                    return Err(at("a <markdown> has one <markdown-styles>".into()));
                }
                let rules = Rules::from_elem(&child)?;
                declaration.styles = Some((child.attribute("src").map(str::to_string), rules));
            }
            "markdown-content" => {
                if seen_content {
                    return Err(at("a <markdown> has one <markdown-content>".into()));
                }
                seen_content = true;
                declaration.data_path = child.attribute("data-value").map(str::to_string);
                declaration.text = dedent(&child.children().filter(|n| n.is_text()).filter_map(|n| n.text()).collect::<String>());
            }
            other => return Err(at(format!(
                "unexpected child <{other}> — expected <markdown-styles> or <markdown-content>"
            ))),
        }
    }
    if !seen_content {
        return Err(at("needs a <markdown-content>".into()));
    }
    build(elem, declaration, cx).map_err(|e| if e.starts_with('<') { e } else { at(e) })
}

/// Read a `markdown` node of a kit tree into the child of the section that holds it.
pub(crate) fn parse_tree(json: &Value, cx: &SectionContext) -> Result<SectionChild, String> {
    let mut declaration = Declaration { styles: None, data_path: None, text: String::new() };
    let mut seen_content = false;
    for child in json.get("nodes").and_then(Value::as_array).into_iter().flatten() {
        match child.get("type").and_then(Value::as_str) {
            Some("markdown-styles") => {
                if declaration.styles.is_some() {
                    return Err("markdown: a markdown has one markdown-styles".into());
                }
                declaration.styles = Some((
                    crate::parse::jattr(child, "src").map(str::to_string),
                    Rules::from_tree(child)?,
                ));
            }
            Some("markdown-content") => {
                if seen_content {
                    return Err("markdown: a markdown has one markdown-content".into());
                }
                seen_content = true;
                declaration.data_path = crate::parse::jattr(child, "data-value").map(str::to_string);
                let text: String = child
                    .get("nodes")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect();
                declaration.text = dedent(&text);
            }
            other => return Err(format!(
                "markdown: child 'type' must be 'markdown-styles' or 'markdown-content', got {other:?}"
            )),
        }
    }
    if !seen_content {
        return Err("markdown: needs a markdown-content".into());
    }
    build(&JsonAttrs(json), declaration, cx).map_err(|e| format!("markdown: {e}"))
}

/// The child of the section for what a `<markdown>` declared and the attributes it carries.
fn build(a: &impl Attrs, declaration: Declaration, cx: &SectionContext) -> Result<SectionChild, String> {
    let margin = layout_margin(a, cx.tokens)?;
    let gap_text = a.get("gap").unwrap_or(DEFAULT_GAP);
    let gap = cx.tokens.resolve_space(gap_text)?;
    let bookmarks = match a.get("bookmarks") {
        None    => DEFAULT_BOOKMARK_LEVELS,
        Some(v) => v.parse::<u8>().ok().filter(|n| *n <= 6).ok_or_else(|| format!("invalid bookmarks '{v}': expected 0 to 6"))?,
    };
    let defaults = cx.defaults.overridden_by(cx.tokens, a.get("font"), a.get("font-size"), a.get("color"))?;

    let mut rules = Rules::defaults()?;
    let mut written: Vec<String> = Vec::new();
    if let Some((src, inline)) = declaration.styles {
        if let Some(src) = src {
            let file = cx.styles_files.get(&src).ok_or_else(|| format!(
                "<markdown-styles src=\"{src}\"> is read by whoever renders the document, as the src of a font or an image \
                 is, and this one did not load it"
            ))?;
            let doc = roxmltree::Document::parse(file).map_err(|e| format!("markdown-styles file '{src}': {e}"))?;
            if doc.root_element().tag_name().name() != "markdown-styles" {
                return Err(format!("markdown-styles file '{src}': the root element must be <markdown-styles>"));
            }
            let file_rules = Rules::from_elem(&doc.root_element()).map_err(|e| format!("markdown-styles file '{src}': {e}"))?;
            written.extend(file_rules.0.keys().cloned());
            rules.lay(file_rules);
        }
        written.extend(inline.0.keys().cloned());
        rules.lay(inline);
    }

    let spec = MarkdownSpec {
        rules,
        text: declaration.text,
        data_path: declaration.data_path,
        bookmarks,
        gap: gap_text.to_string(),
        tokens: cx.tokens.clone(),
        fonts: cx.fonts.clone(),
        images: cx.images.clone(),
        image_srcs: cx.image_srcs.clone(),
        defaults,
    };
    // A rule that was written, in the document or in a file, is tried now: a value that does not read is told with
    // the name of its rule, not when the first block that uses it comes up.
    for name in &written {
        let sample = RULE_SAMPLES.iter().find(|(rule, _)| rule == name).map_or("x", |(_, sample)| *sample);
        expand(&spec, sample).map_err(|e| {
            format!("the {name} rule: {}", e.strip_prefix("markdown line 1: ").unwrap_or(&e))
        })?;
    }

    let mut node = Node::layout_default();
    node.kind = NodeKind::Markdown;
    node.debug = a.get("debug") == Some("true");
    node.detail = Some(Box::new(crate::parse::Detail::Markdown(Box::new(spec))));
    Ok(SectionChild::Layout { margin, gap, children: vec![LayoutChild::Content(node)] })
}

/// A Markdown that uses a rule, to try the rule with.
const RULE_SAMPLES: [(&str, &str); 17] = [
    ("h1", "# x"), ("h2", "## x"), ("h3", "### x"), ("h4", "#### x"), ("h5", "##### x"), ("h6", "###### x"),
    ("p", "x"), ("code", "`x`\n\n```\nx\n```"), ("quote", "> x"), ("li", "- x\n\n1. x"), ("a", "[x](#x)"),
    ("strong", "**x**"), ("em", "*x*"), ("del", "~~x~~"), ("table", "| x |\n|---|\n| x |"),
    ("th", "| x |\n|---|\n| x |"), ("td", "| x |\n|---|\n| x |"),
];

/// Remove the indent that all the lines of `text` share, so that indenting the XML does not make a code block of it.
/// A line with only white space does not count, and is left empty.
pub(crate) fn dedent(text: &str) -> String {
    let indent = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start_matches([' ', '\t']).len())
        .min()
        .unwrap_or(0);
    let mut out = String::with_capacity(text.len());
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 { out.push('\n'); }
        if !line.trim().is_empty() { out.push_str(line.get(indent..).unwrap_or(line)); }
    }
    out
}

// ── When the text is known ────────────────────────────────────────────────────

/// How far `expand_pending` goes.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Pass {
    /// The document has just been read: expand the Markdown written inline, and check the text that stands in for
    /// the data of the rest.
    Read,
    /// The data is in: expand the Markdown that comes from it.
    Data,
    /// Whatever is left, with the text it stands in with.
    Rest,
}

/// Expand the `<markdown>` nodes of `sections` that this pass reaches, and settle the ids of the bookmarks they
/// bring.
pub(crate) fn expand_pending(sections: &mut [Section], data: Option<&Value>, pass: Pass) -> Result<(), String> {
    let mut expanded = false;
    for section in sections.iter_mut() {
        for child in &mut section.children {
            let SectionChild::Layout { children, .. } = child else { continue };
            let mut index = 0;
            while index < children.len() {
                let LayoutChild::Content(node) = &children[index] else { index += 1; continue };
                let Some(spec) = markdown_spec(node) else { index += 1; continue };
                let debug = node.debug;
                let text = match (&spec.data_path, pass) {
                    (None, _) => spec.text.clone(),
                    (Some(_), Pass::Read) => {
                        // Nothing to expand yet, but the text that stands in for the data has to read.
                        expand(spec, &spec.text)?;
                        index += 1;
                        continue;
                    }
                    (Some(path), Pass::Data) => match data {
                        Some(root) => crate::data::text_at(path, root)?,
                        None       => spec.text.clone(),
                    },
                    (Some(_), Pass::Rest) => spec.text.clone(),
                };
                // The text that stands in was read when the document was; if it fails now, the node is dropped.
                let mut nodes = if pass == Pass::Rest { expand(spec, &text).unwrap_or_default() } else { expand(spec, &text)? };
                if debug { nodes.iter_mut().for_each(outline); }
                let count = nodes.len();
                children.splice(index..=index, nodes.into_iter().map(LayoutChild::Content));
                index += count;
                expanded = true;
            }
        }
    }
    if expanded {
        crate::parse::assign_bookmark_ids(sections)?;
    }
    Ok(())
}

/// Outline `node` and every box in it, as `debug` on the `<markdown>` asks.
fn outline(node: &mut Node) {
    node.debug = true;
    node.children.iter_mut().for_each(outline);
}

fn markdown_spec(node: &Node) -> Option<&MarkdownSpec> {
    match node.detail.as_deref() {
        Some(crate::parse::Detail::Markdown(spec)) if node.kind == NodeKind::Markdown => Some(spec),
        _ => None,
    }
}

/// The template that wraps a `.md` file when no other is given.
pub fn default_template() -> &'static str {
    DEFAULT_TEMPLATE
}

/// The rules that a `<markdown>` with no styles uses, as the file to start a style of one's own from.
pub fn default_styles() -> &'static str {
    DEFAULT_STYLES
}

// ── Expansion ─────────────────────────────────────────────────────────────────

/// The nodes of the layout for `markdown`, styled by the rules of `spec`.
pub(crate) fn expand(spec: &MarkdownSpec, markdown: &str) -> Result<Vec<Node>, String> {
    let blocks = Writer::new(spec, markdown).blocks()?;
    let mut nodes = Vec::new();
    for block in blocks {
        let xml = format!("<md>{}</md>", block.xml);
        let doc = roxmltree::Document::parse(&xml).map_err(|e| format!("markdown line {}: {e}", block.line))?;
        let mut pending = PendingBookmarks::default();
        for child in elems(&doc.root_element()) {
            if child.tag_name().name() == "bookmark" {
                pending.push(crate::parse::xml_bookmark(&child).map_err(|e| at_line(block.line, &e))?);
                continue;
            }
            let mut parsed = parse_node(&child, &spec.tokens, &spec.images).map_err(|e| at_line(block.line, &e))?;
            pending.attach(&mut parsed);
            nodes.push(resolve_parsed_node(parsed, &spec.defaults, &spec.fonts));
        }
        pending.finish("markdown").map_err(|e| at_line(block.line, &e))?;
    }
    Ok(nodes)
}

/// An error of the parser, which names a place in the XML written here, said as a place in the Markdown.
fn at_line(line: usize, error: &str) -> String {
    let mut out = String::new();
    let mut rest = error;
    while let Some(start) = rest.find(" at line ") {
        out.push_str(&rest[..start]);
        let after = &rest[start + " at line ".len()..];
        let digits = after.chars().take_while(|c| c.is_ascii_digit() || *c == ',' || *c == ' ' || c.is_ascii_alphabetic()).count();
        rest = after.get(digits..).unwrap_or("");
    }
    out.push_str(rest);
    format!("markdown line {line}: {out}")
}

/// One block of the Markdown written as XML, and the line it starts on.
struct Block {
    xml:  String,
    line: usize,
}

/// What the rules of the quotes and the lists around a block say about its words.
#[derive(Clone, Default)]
struct Around {
    text: Vec<Rule>,
}

/// The style of the words being read.
#[derive(Clone, Default)]
struct Style {
    strong: bool,
    em:     bool,
    del:    bool,
    link:   Option<String>,
}

/// A piece of the words of a block.
enum Piece {
    Text { text: String, style: Style },
    Code { text: String, style: Style },
    Note(String),
    Image { dest: String, line: usize },
}

struct Writer<'a> {
    spec:    &'a MarkdownSpec,
    events:  Vec<(Event<'a>, Range<usize>)>,
    at:      usize,
    lines:   Vec<usize>,
    notes:   HashMap<String, String>,
    /// The font the words of the block being written are set in, for the italic of a built-in font.
    font:    String,
}

impl<'a> Writer<'a> {
    fn new(spec: &'a MarkdownSpec, markdown: &'a str) -> Self {
        let options = Options::ENABLE_TABLES
            | Options::ENABLE_FOOTNOTES
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
            | Options::ENABLE_MATH;
        let events: Vec<(Event, Range<usize>)> = Parser::new_ext(markdown, options).into_offset_iter().collect();
        let mut lines = vec![0];
        lines.extend(markdown.match_indices('\n').map(|(i, _)| i + 1));
        Writer { spec, events, at: 0, lines, notes: HashMap::new(), font: spec.defaults.font.clone() }
    }

    /// The line of the Markdown that `offset` is on, counting from 1.
    fn line(&self, offset: usize) -> usize {
        self.lines.partition_point(|start| *start <= offset)
    }

    fn here(&self) -> usize {
        self.events.get(self.at).map_or(1, |(_, range)| self.line(range.start))
    }

    fn fail(&self, line: usize, message: &str) -> String {
        format!("markdown line {line}: {message}")
    }

    fn blocks(mut self) -> Result<Vec<Block>, String> {
        self.collect_notes();
        self.read_blocks(None, &Around::default())
    }

    /// Read the text of every footnote definition first: a note is written where it is referred to.
    fn collect_notes(&mut self) {
        let mut at = 0;
        while at < self.events.len() {
            if let Event::Start(Tag::FootnoteDefinition(label)) = &self.events[at].0 {
                let label = label.to_string();
                let mut depth = 0;
                let mut words = String::new();
                at += 1;
                while at < self.events.len() {
                    match &self.events[at].0 {
                        Event::Start(Tag::FootnoteDefinition(_)) => depth += 1,
                        Event::End(TagEnd::FootnoteDefinition) if depth == 0 => break,
                        Event::End(TagEnd::FootnoteDefinition) => depth -= 1,
                        Event::Text(text) | Event::Code(text) => {
                            if !words.is_empty() && !words.ends_with(' ') { words.push(' '); }
                            words.push_str(text);
                        }
                        Event::SoftBreak | Event::HardBreak => words.push(' '),
                        _ => {}
                    }
                    at += 1;
                }
                self.notes.insert(label, words.trim().to_string());
            }
            at += 1;
        }
    }

    /// Read blocks until the end of the container `until`, or the end of the text when there is none.
    fn read_blocks(&mut self, until: Option<TagEnd>, around: &Around) -> Result<Vec<Block>, String> {
        let mut blocks = Vec::new();
        while self.at < self.events.len() {
            let line = self.here();
            let (event, _) = self.events[self.at].clone();
            match event {
                Event::End(end) if Some(end) == until => {
                    self.at += 1;
                    return Ok(blocks);
                }
                Event::Start(Tag::Paragraph) => {
                    self.at += 1;
                    let pieces = self.read_pieces(Some(TagEnd::Paragraph))?;
                    self.paragraph(pieces, line, around, &mut blocks)?;
                }
                Event::Start(Tag::Heading { level, .. }) => {
                    self.at += 1;
                    let pieces = self.read_pieces(Some(TagEnd::Heading(level)))?;
                    blocks.push(self.heading(level, pieces, line)?);
                }
                Event::Start(Tag::BlockQuote(_)) => {
                    self.at += 1;
                    let inner_around = self.around_quote(around);
                    let inner = self.read_blocks(Some(TagEnd::BlockQuote(None)), &inner_around)?;
                    blocks.push(self.quote(inner, line)?);
                }
                Event::Start(Tag::CodeBlock(kind)) => {
                    self.at += 1;
                    let mut text = String::new();
                    while let Some((event, _)) = self.events.get(self.at) {
                        self.at += 1;
                        match event {
                            Event::Text(t) => text.push_str(t),
                            Event::End(TagEnd::CodeBlock) => break,
                            _ => {}
                        }
                    }
                    let _ = kind == CodeBlockKind::Indented;
                    blocks.push(self.code_block(&text, line));
                }
                Event::Start(Tag::List(first)) => {
                    self.at += 1;
                    blocks.push(self.list(first, line, around)?);
                }
                Event::Start(Tag::Table(aligns)) => {
                    self.at += 1;
                    blocks.push(self.table(&aligns, line)?);
                }
                Event::Start(Tag::FootnoteDefinition(_)) => self.skip(TagEnd::FootnoteDefinition),
                Event::Start(Tag::MetadataBlock(MetadataBlockKind::YamlStyle)) => self.skip(TagEnd::MetadataBlock(MetadataBlockKind::YamlStyle)),
                Event::Start(Tag::MetadataBlock(MetadataBlockKind::PlusesStyle)) => self.skip(TagEnd::MetadataBlock(MetadataBlockKind::PlusesStyle)),
                Event::Start(Tag::HtmlBlock) | Event::Html(_) | Event::InlineHtml(_) => {
                    return Err(self.fail(line, "raw HTML is not supported: write the Markdown it stands for, or leave it out"));
                }
                Event::InlineMath(_) | Event::DisplayMath(_) => {
                    return Err(self.fail(line, "math is not supported: write it as text"));
                }
                Event::Rule => {
                    self.at += 1;
                    blocks.push(Block { xml: "<divider/>".into(), line });
                }
                Event::Start(Tag::Image { .. } | Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Link { .. })
                | Event::Text(_) | Event::Code(_) | Event::SoftBreak | Event::HardBreak | Event::FootnoteReference(_) => {
                    // The words of a tight list item stand without a paragraph around them.
                    let pieces = self.read_pieces(None)?;
                    self.paragraph(pieces, line, around, &mut blocks)?;
                }
                other => return Err(self.fail(line, &format!("{} is not supported", describe(&other)))),
            }
        }
        Ok(blocks)
    }

    /// Step over a block that draws nothing, to the end `end` of it.
    fn skip(&mut self, end: TagEnd) {
        self.at += 1;
        let mut depth = 0;
        while let Some((event, _)) = self.events.get(self.at) {
            self.at += 1;
            match event {
                Event::Start(tag) if tag.to_end() == end => depth += 1,
                Event::End(e) if *e == end => {
                    if depth == 0 { return; }
                    depth -= 1;
                }
                _ => {}
            }
        }
    }

    /// Read the words of a block, up to the end `until` of it or, for a block with no tag of its own, up to the next
    /// block.
    fn read_pieces(&mut self, until: Option<TagEnd>) -> Result<Vec<Piece>, String> {
        let line = self.here();
        let mut read = Vec::new();
        self.read_words(until, &Style::default(), &mut read)?;
        // The parser gives text in pieces, cut where a `[` or a `*` was tried and found to be plain text.
        let mut pieces: Vec<Piece> = Vec::with_capacity(read.len());
        for piece in read {
            match (pieces.last_mut(), piece) {
                (Some(Piece::Text { text: last, style: before }), Piece::Text { text, style }) if style_eq(before, &style) => {
                    last.push_str(&text);
                }
                (_, piece) => pieces.push(piece),
            }
        }
        for piece in &pieces {
            if let Piece::Text { text, .. } = piece {
                if let Some(label) = undefined_note(text) {
                    return Err(self.fail(line, &format!(
                        "the footnote [^{label}] has no definition; define it with [^{label}]: and its words, or write \\[ to show it as text"
                    )));
                }
            }
        }
        Ok(pieces)
    }

    fn read_words(&mut self, until: Option<TagEnd>, style: &Style, pieces: &mut Vec<Piece>) -> Result<(), String> {
        while self.at < self.events.len() {
            let line = self.here();
            let (event, _) = self.events[self.at].clone();
            match event {
                Event::End(end) if Some(end) == until => {
                    self.at += 1;
                    return Ok(());
                }
                // The end of the container the words stand in.
                Event::End(_) => return Ok(()),
                Event::Text(text) => {
                    pieces.push(Piece::Text { text: text.to_string(), style: style.clone() });
                    self.at += 1;
                }
                Event::Code(text) => {
                    pieces.push(Piece::Code { text: text.to_string(), style: style.clone() });
                    self.at += 1;
                }
                Event::SoftBreak | Event::HardBreak => {
                    pieces.push(Piece::Text { text: " ".into(), style: style.clone() });
                    self.at += 1;
                }
                Event::FootnoteReference(label) => {
                    let note = self.notes.get(label.as_ref()).ok_or_else(|| {
                        self.fail(line, &format!("the footnote [^{label}] has no definition"))
                    })?;
                    pieces.push(Piece::Note(note.clone()));
                    self.at += 1;
                }
                Event::Start(Tag::Emphasis) => {
                    self.at += 1;
                    self.read_words(Some(TagEnd::Emphasis), &Style { em: true, ..style.clone() }, pieces)?;
                }
                Event::Start(Tag::Strong) => {
                    self.at += 1;
                    self.read_words(Some(TagEnd::Strong), &Style { strong: true, ..style.clone() }, pieces)?;
                }
                Event::Start(Tag::Strikethrough) => {
                    self.at += 1;
                    self.read_words(Some(TagEnd::Strikethrough), &Style { del: true, ..style.clone() }, pieces)?;
                }
                Event::Start(Tag::Link { dest_url, .. }) => {
                    self.at += 1;
                    let link = Style { link: Some(dest_url.to_string()), ..style.clone() };
                    self.read_words(Some(TagEnd::Link), &link, pieces)?;
                }
                Event::Start(Tag::Image { dest_url, .. }) => {
                    self.at += 1;
                    // The alt text is not kept: there are no tagged PDFs yet to carry it.
                    while let Some((event, _)) = self.events.get(self.at) {
                        self.at += 1;
                        if matches!(event, Event::End(TagEnd::Image)) { break; }
                    }
                    pieces.push(Piece::Image { dest: dest_url.to_string(), line });
                }
                Event::Html(_) | Event::InlineHtml(_) => {
                    return Err(self.fail(line, "raw HTML is not supported: write the Markdown it stands for, or leave it out"));
                }
                Event::InlineMath(_) | Event::DisplayMath(_) => {
                    return Err(self.fail(line, "math is not supported: write it as text"));
                }
                // A block that begins ends the words of a block with no tag of its own.
                Event::Start(_) | Event::Rule | Event::TaskListMarker(_) if until.is_none() => return Ok(()),
                other => return Err(self.fail(line, &format!("{} is not supported here", describe(&other)))),
            }
        }
        Ok(())
    }

    // ── Rules at work ─────────────────────────────────────────────────────────

    /// The attributes of the rule `name` for words, over which the quotes and items around it lay theirs.
    fn text_rule(&self, name: &str, around: &Around) -> Rule {
        let mut rule = self.spec.rules.rule(name);
        if name == "p" {
            for over in &around.text {
                for (key, value) in over {
                    if TEXT_ATTRS.contains(&key.as_str()) { rule.insert(key.clone(), value.clone()); }
                }
            }
        }
        rule
    }

    fn around_quote(&self, around: &Around) -> Around {
        let mut inner = around.clone();
        inner.text.push(self.spec.rules.rule("quote"));
        inner
    }

    /// The font a rule sets the words in, or the one they are in without it.
    fn font_of(&self, rule: &Rule) -> String {
        let name = rule.get("font").map(String::as_str).unwrap_or(&self.spec.defaults.font);
        resolve_font_alias(name, &self.spec.fonts)
    }

    /// `<text>` for the words of a block, wrapped in a box when the rule has a box around the words.
    fn text_block(&mut self, name: &str, rule: &Rule, pieces: &[Piece], extra: &[(&str, &str)], bookmark: Option<String>) -> Result<String, String> {
        self.font = self.font_of(rule);
        let mut text_attrs = String::new();
        for key in TEXT_ATTRS {
            if let Some(value) = rule.get(key) { push_attr(&mut text_attrs, key, value); }
        }
        let has_box = BOX_ATTRS.iter().any(|key| rule.contains_key(*key));
        let mut inner = String::new();
        if !has_box {
            for (key, value) in extra { push_attr(&mut text_attrs, key, value); }
        }
        inner.push_str(&format!("<text{text_attrs}>"));
        for piece in pieces { self.write_piece(&mut inner, piece, name)?; }
        inner.push_str("</text>");
        let mut xml = bookmark.unwrap_or_default();
        if has_box {
            let mut box_attrs = String::new();
            for key in BOX_ATTRS {
                if let Some(value) = rule.get(key) { push_attr(&mut box_attrs, key, value); }
            }
            for (key, value) in extra { push_attr(&mut box_attrs, key, value); }
            xml.push_str(&format!("<column{box_attrs}>{inner}</column>"));
        } else {
            xml.push_str(&inner);
        }
        Ok(xml)
    }

    /// The attributes a run of words gets from the styles it is in.
    fn style_attrs(&self, style: &Style) -> Rule {
        let mut attrs = Rule::new();
        let lay = |attrs: &mut Rule, rule: Rule| attrs.extend(rule);
        if style.strong { lay(&mut attrs, self.spec.rules.rule("strong")); }
        if style.em {
            let rule = self.spec.rules.rule("em");
            if !rule.contains_key("font") {
                if let Some(face) = italic_face(&self.font) { attrs.insert("font".into(), face.into()); }
            }
            lay(&mut attrs, rule);
        }
        if style.del { lay(&mut attrs, self.spec.rules.rule("del")); }
        if let Some(link) = &style.link {
            lay(&mut attrs, self.spec.rules.rule("a"));
            attrs.insert("href".into(), link.clone());
        }
        attrs
    }

    fn write_piece(&self, out: &mut String, piece: &Piece, block: &str) -> Result<(), String> {
        match piece {
            Piece::Text { text, style } => {
                let attrs = self.style_attrs(style);
                if attrs.is_empty() {
                    out.push_str(&escape_text(text));
                } else {
                    out.push_str(&format!("<span{}>{}</span>", attrs_xml(&attrs), escape_text(text)));
                }
            }
            Piece::Code { text, style } => {
                let code = self.spec.rules.rule("code");
                if let Some(link) = &style.link {
                    // A code-span has no link, so a linked run of code is a span in the code font.
                    let mut attrs = self.style_attrs(style);
                    attrs.insert("font".into(), code.get("font").cloned().unwrap_or_else(|| "Courier".into()));
                    attrs.insert("href".into(), link.clone());
                    out.push_str(&format!("<span{}>{}</span>", attrs_xml(&attrs), escape_text(text)));
                } else {
                    let mut attrs = Rule::new();
                    for key in ["font", "color", "background"] {
                        if let Some(value) = code.get(key) { attrs.insert(key.into(), value.clone()); }
                    }
                    out.push_str(&format!("<code-span{}>{}</code-span>", attrs_xml(&attrs), escape_text(text)));
                }
            }
            Piece::Note(words) => {
                let _ = block;
                out.push_str(&format!("<fn>{}</fn>", escape_text(words)));
            }
            Piece::Image { .. } => {}
        }
        Ok(())
    }

    // ── Blocks ────────────────────────────────────────────────────────────────

    fn paragraph(&mut self, pieces: Vec<Piece>, line: usize, around: &Around, blocks: &mut Vec<Block>) -> Result<(), String> {
        // An image is a block of its own, so the words around it are blocks too.
        let mut words: Vec<Piece> = Vec::new();
        let flush = |this: &mut Self, words: &mut Vec<Piece>, blocks: &mut Vec<Block>| -> Result<(), String> {
            if words.iter().all(|piece| matches!(piece, Piece::Text { text, .. } if text.trim().is_empty())) {
                words.clear();
                return Ok(());
            }
            let rule = this.text_rule("p", around);
            let xml = this.text_block("p", &rule, words, &[], None)?;
            words.clear();
            blocks.push(Block { xml, line });
            Ok(())
        };
        for piece in pieces {
            match piece {
                Piece::Image { dest, line: image_line } => {
                    flush(self, &mut words, blocks)?;
                    let name = self.image_name(&dest, image_line)?;
                    blocks.push(Block { xml: format!("<img name=\"{}\" width=\"fill\"/>", escape_attr(&name)), line: image_line });
                }
                other => words.push(other),
            }
        }
        flush(self, &mut words, blocks)
    }

    /// The asset an image of the Markdown is: the one named `dest`, or the one whose `src` is.
    fn image_name(&self, dest: &str, line: usize) -> Result<String, String> {
        let lower = dest.to_ascii_lowercase();
        if ["http://", "https://", "ftp://", "file://", "data:"].iter().any(|scheme| lower.starts_with(scheme)) {
            return Err(self.fail(line, &format!(
                "an image's source must be the name or the src of an asset, not a URL: '{dest}'"
            )));
        }
        if self.spec.images.contains_key(dest) {
            return Ok(dest.to_string());
        }
        let bare = dest.strip_prefix("./").unwrap_or(dest);
        self.spec.image_srcs.get(dest).or_else(|| self.spec.image_srcs.get(bare)).cloned().ok_or_else(|| {
            self.fail(line, &format!("an image's source must be the name or the src of an asset: '{dest}' is neither"))
        })
    }

    fn heading(&mut self, level: HeadingLevel, pieces: Vec<Piece>, line: usize) -> Result<Block, String> {
        let number = level as usize;
        let name = format!("h{number}");
        let rule = self.spec.rules.rule(&name);
        let label: String = pieces
            .iter()
            .filter_map(|piece| match piece {
                Piece::Text { text, .. } | Piece::Code { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        let label = label.split_whitespace().collect::<Vec<_>>().join(" ");
        let bookmark = (number as u8 <= self.spec.bookmarks && !label.is_empty())
            .then(|| format!("<bookmark label=\"{}\" level=\"{number}\"/>", escape_attr(&label)));
        let xml = self.text_block(&name, &rule, &pieces, &[("paginate", "keep-next")], bookmark)?;
        Ok(Block { xml, line })
    }

    /// A quote is a table with a row for each of its blocks: a narrow cell filled with the color of the bar, and the
    /// cell that holds the block. The bar is cell backgrounds that touch, the space between two blocks being in the
    /// padding of both cells, and the quote splits across pages between its blocks.
    fn quote(&self, inner: Vec<Block>, line: usize) -> Result<Block, String> {
        let rule = self.spec.rules.rule("quote");
        let tokens = &self.spec.tokens;
        let invalid = |what: &str, e: String| self.fail(line, &format!("the quote rule: {what}: {e}"));
        let width = rule.get("bar-width").map(String::as_str).unwrap_or("3pt");
        let padding = match rule.get("padding") {
            Some(value) => tokens.resolve_spacing(value).map_err(|e| invalid("padding", e))?,
            None        => [0.0; 4],
        };
        let gap = tokens.resolve_space(&self.spec.gap).map_err(|e| invalid("gap", e))?;
        let mut bar = String::new();
        if let Some(color) = rule.get("bar-color") { push_attr(&mut bar, "background", color); }
        let mut rows = String::new();
        for (i, block) in inner.iter().enumerate() {
            let bottom = padding[2] + if i + 1 < inner.len() { gap } else { 0.0 };
            let mut cell = format!(" padding=\"{}pt {}pt {}pt {}pt\"", padding[0], padding[1], bottom, padding[3]);
            for key in ["background", "border", "radius"] {
                if let Some(value) = rule.get(key) { push_attr(&mut cell, key, value); }
            }
            rows.push_str(&format!("<tr><td{bar}></td><td{cell}>{}</td></tr>", block.xml));
        }
        let xml = if rows.is_empty() { String::new() } else {
            format!("<table cols=\"{} 1fr\">{rows}</table>", escape_attr(width))
        };
        Ok(Block { xml, line })
    }

    fn code_block(&self, text: &str, line: usize) -> Block {
        let rule = self.spec.rules.rule("code");
        let text = text.strip_suffix('\n').unwrap_or(text);
        // The line break after the start tag is not part of the code, so one is written to keep a first empty line.
        Block { xml: format!("<code{}>\n{}</code>", attrs_xml(&rule), escape_text(text)), line }
    }

    fn list(&mut self, first: Option<u64>, line: usize, around: &Around) -> Result<Block, String> {
        let rule = self.spec.rules.rule("li");
        let mut attrs = String::new();
        match first {
            Some(start) => {
                push_attr(&mut attrs, "type", "number");
                if start != 1 { push_attr(&mut attrs, "start", &start.to_string()); }
            }
            None => push_attr(&mut attrs, "type", "bullet"),
        }
        // The shape of a bullet is the bullet list's alone: a number list writes numbers.
        for key in ["indent", "bullet", "gap"] {
            if key == "bullet" && first.is_some() { continue; }
            if let Some(value) = rule.get(key) { push_attr(&mut attrs, key, value); }
        }
        let mut item_attrs = String::new();
        for key in ["padding", "background", "border"] {
            if let Some(value) = rule.get(key) { push_attr(&mut item_attrs, key, value); }
        }
        let mut inner_around = around.clone();
        let mut words = Rule::new();
        for key in ["font", "font-size", "color"] {
            if let Some(value) = rule.get(key) { words.insert(key.into(), value.clone()); }
        }
        inner_around.text.push(words);

        let mut items = String::new();
        while self.at < self.events.len() {
            let (event, _) = self.events[self.at].clone();
            match event {
                Event::Start(Tag::Item) => {
                    self.at += 1;
                    let blocks = self.read_blocks(Some(TagEnd::Item), &inner_around)?;
                    let body: String = blocks.iter().map(|block| block.xml.as_str()).collect();
                    items.push_str(&format!("<li{item_attrs}>{body}</li>"));
                }
                Event::End(TagEnd::List(_)) => {
                    self.at += 1;
                    break;
                }
                _ => {
                    return Err(self.fail(self.here(), "a list holds items only"));
                }
            }
        }
        Ok(Block { xml: format!("<list{attrs}>{items}</list>"), line })
    }

    fn table(&mut self, aligns: &[pulldown_cmark::Alignment], line: usize) -> Result<Block, String> {
        let columns = aligns.len().max(1);
        let rule = self.spec.rules.rule("table");
        let mut attrs = format!(" cols=\"{}\"", vec!["1fr"; columns].join(" "));
        for (key, value) in &rule { push_attr(&mut attrs, key, value); }

        let mut head = String::new();
        let mut rows = String::new();
        while self.at < self.events.len() {
            let (event, _) = self.events[self.at].clone();
            self.at += 1;
            match event {
                Event::Start(Tag::TableHead) => {
                    let cells = self.table_cells(aligns, "th", TagEnd::TableHead)?;
                    head = format!("<thead>{cells}</thead>");
                }
                Event::Start(Tag::TableRow) => {
                    let cells = self.table_cells(aligns, "td", TagEnd::TableRow)?;
                    rows.push_str(&format!("<tr>{cells}</tr>"));
                }
                Event::End(TagEnd::Table) => break,
                _ => {}
            }
        }
        Ok(Block { xml: format!("<table{attrs}>{head}{rows}</table>"), line })
    }

    fn table_cells(&mut self, aligns: &[pulldown_cmark::Alignment], rule_name: &str, until: TagEnd) -> Result<String, String> {
        let rule = self.spec.rules.rule(rule_name);
        let mut cells = String::new();
        let mut column = 0;
        while self.at < self.events.len() {
            let line = self.here();
            let (event, _) = self.events[self.at].clone();
            self.at += 1;
            match event {
                Event::Start(Tag::TableCell) => {
                    let pieces = self.read_pieces(Some(TagEnd::TableCell))?;
                    if pieces.iter().any(|piece| matches!(piece, Piece::Note(_))) {
                        return Err(self.fail(line, "a footnote can't be in a table"));
                    }
                    if pieces.iter().any(|piece| matches!(piece, Piece::Image { .. })) {
                        return Err(self.fail(line, "an image can't be in a table cell"));
                    }
                    let mut cell = rule.clone();
                    let align = match aligns.get(column) {
                        Some(pulldown_cmark::Alignment::Center) => Some("center"),
                        Some(pulldown_cmark::Alignment::Right)  => Some("right"),
                        Some(pulldown_cmark::Alignment::Left)   => Some("left"),
                        _ => None,
                    };
                    if let Some(align) = align { cell.insert("align".into(), align.into()); }
                    let mut box_attrs = String::new();
                    for key in ["padding", "background", "border"] {
                        if let Some(value) = cell.remove(key) { push_attr(&mut box_attrs, key, &value); }
                    }
                    let text = self.text_block("p", &cell, &pieces, &[], None)?;
                    cells.push_str(&format!("<td{box_attrs}>{text}</td>"));
                    column += 1;
                }
                Event::End(end) if end == until => break,
                _ => {}
            }
        }
        while column < aligns.len() {
            cells.push_str("<td><text></text></td>");
            column += 1;
        }
        Ok(cells)
    }
}

/// The italic face of a built-in font, or none for a font that has to be named.
fn italic_face(font: &str) -> Option<&'static str> {
    Some(match font {
        "Helvetica" | "Helvetica-Oblique"             => "Helvetica-Oblique",
        "Helvetica-Bold" | "Helvetica-BoldOblique"    => "Helvetica-Oblique",
        "Times-Roman" | "Times-Italic"                => "Times-Italic",
        "Times-Bold" | "Times-BoldItalic"             => "Times-Italic",
        "Courier" | "Courier-Oblique"                 => "Courier-Oblique",
        "Courier-Bold" | "Courier-BoldOblique"        => "Courier-Oblique",
        _ => return None,
    })
}

fn style_eq(a: &Style, b: &Style) -> bool {
    a.strong == b.strong && a.em == b.em && a.del == b.del && a.link == b.link
}

/// The label of a footnote reference that the parser left in `text` because nothing defines it.
fn undefined_note(text: &str) -> Option<&str> {
    let start = text.find("[^")?;
    let rest = &text[start + 2..];
    let end = rest.find(']')?;
    let label = &rest[..end];
    (!label.is_empty() && !label.contains(char::is_whitespace)).then_some(label)
}

fn describe(event: &Event) -> String {
    match event {
        Event::Start(tag) => format!("{tag:?}").split(['(', ' ', '{']).next().unwrap_or("this").to_string(),
        Event::TaskListMarker(_) => "a task list".into(),
        _ => "this".into(),
    }
}

fn push_attr(out: &mut String, key: &str, value: &str) {
    out.push_str(&format!(" {key}=\"{}\"", escape_attr(value)));
}

fn attrs_xml(rule: &Rule) -> String {
    let mut out = String::new();
    for (key, value) in rule { push_attr(&mut out, key, value); }
    out
}

/// Text for XML. A control character the format can't hold is dropped.
fn escape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c if (c as u32) < 0x20 && !matches!(c, '\n' | '\t' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

fn escape_attr(text: &str) -> String {
    escape_text(text).replace('"', "&quot;").replace('\n', " ").replace('\t', " ").replace('\r', " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{parse, parse_with, Resources};

    /// A document whose one section is a `<markdown>` with `attrs`, `styles` and the Markdown inline.
    fn document(attrs: &str, styles: &str, markdown: &str) -> String {
        format!(
            r#"<lpdf version="1"><assets><image name="chart" src="img/chart.png"/></assets><document><section>
            <markdown {attrs}>{styles}<markdown-content><![CDATA[{markdown}]]></markdown-content></markdown>
            </section></document></lpdf>"#
        )
    }

    fn nodes(markdown: &str) -> Vec<Node> {
        nodes_in(&document("", "", markdown))
    }

    fn nodes_in(xml: &str) -> Vec<Node> {
        parse(xml).unwrap().section_layouts().remove(0).children
    }

    fn error(markdown: &str) -> String {
        parse(&document("", "", markdown)).unwrap_err()
    }

    /// The words of a text, with a space where the source has one.
    fn words(node: &Node) -> String {
        let mut out = String::new();
        for run in &node.text_runs {
            if run.leading_space && !out.is_empty() { out.push(' '); }
            out.push_str(&run.text);
        }
        out
    }

    /// Every text under `node`, in order.
    fn texts(node: &Node) -> Vec<&Node> {
        let mut found = Vec::new();
        if node.kind == NodeKind::Text { found.push(node); }
        for child in &node.children { found.extend(texts(child)); }
        found
    }

    #[test]
    fn a_heading_is_a_bold_text_that_stays_with_what_follows_and_a_bookmark() {
        let nodes = nodes("# Release notes\n\n## Fixes\n");
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].kind, NodeKind::Text);
        assert_eq!(words(&nodes[0]), "Release notes");
        assert_eq!(nodes[0].font, "Helvetica-Bold");
        assert_eq!(nodes[0].font_size, 28.0);
        assert_eq!(nodes[0].paginate, crate::parse::Paginate::KeepNext);
        assert_eq!((nodes[0].bookmarks[0].label.as_str(), nodes[0].bookmarks[0].level, nodes[0].bookmarks[0].id.as_str()), ("Release notes", 1, "release-notes"));
        assert_eq!((nodes[1].font_size, nodes[1].bookmarks[0].level), (20.0, 2));
    }

    #[test]
    fn bookmarks_keeps_the_levels_up_to_n_and_zero_keeps_none() {
        let two = nodes_in(&document(r#"bookmarks="2""#, "", "# A\n\n## B\n\n### C\n"));
        assert_eq!(two.iter().map(|n| n.bookmarks.len()).collect::<Vec<_>>(), [1, 1, 0]);
        let none = nodes_in(&document(r#"bookmarks="0""#, "", "# A\n"));
        assert!(none[0].bookmarks.is_empty());
    }

    #[test]
    fn a_heading_id_is_the_slug_of_its_text_and_repeats_take_a_suffix() {
        let nodes = nodes("# 3.1 Methods\n\n# 3.1 Methods\n");
        assert_eq!(nodes[0].bookmarks[0].id, "3-1-methods");
        assert_eq!(nodes[1].bookmarks[0].id, "3-1-methods-2");
    }

    #[test]
    fn a_paragraph_is_a_text_and_a_soft_break_is_a_space() {
        let nodes = nodes("one\ntwo\n\nthree\n");
        assert_eq!(nodes.len(), 2);
        assert_eq!(words(&nodes[0]), "one two");
        assert_eq!(words(&nodes[1]), "three");
    }

    #[test]
    fn emphasis_strong_and_strikethrough_are_spans_and_a_built_in_font_gets_its_italic_face() {
        let nodes = nodes("a *b* **c** ~~d~~ ***e***\n");
        let runs = &nodes[0].text_runs;
        assert_eq!(runs.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(), ["a", "b", "c", "d", "e"]);
        assert_eq!(runs[1].font.as_deref(), Some("Helvetica-Oblique"));
        assert_eq!(runs[2].font.as_deref(), Some("Helvetica-Bold"));
        assert!(runs[3].strike);
        assert_eq!(runs[4].font.as_deref(), Some("Helvetica-BoldOblique"));
    }

    #[test]
    fn an_embedded_font_names_its_faces_in_the_rules_of_em_and_strong() {
        let xml = format!(
            r#"<lpdf version="1"><assets><font name="body" core="Times-Roman"/><font name="body-italic" core="Times-Italic"/></assets>
            <document font="body"><section><markdown>
            <markdown-styles><em font="body-italic"/></markdown-styles>
            <markdown-content><![CDATA[a *b*]]></markdown-content></markdown></section></document></lpdf>"#
        );
        let nodes = nodes_in(&xml);
        assert_eq!(nodes[0].text_runs[1].font.as_deref(), Some("Times-Italic"));
    }

    #[test]
    fn inline_code_is_a_code_span_and_a_link_is_a_span_with_its_target() {
        let nodes = nodes("see `x` and [the docs](https://lpdf.io) or [up](#intro)\n");
        let runs = &nodes[0].text_runs;
        assert_eq!((runs[1].text.as_str(), runs[1].font.as_deref(), runs[1].background.as_deref()), ("x", Some("Courier"), Some("#f5f5f5")));
        assert_eq!((runs[3].text.as_str(), runs[3].href.as_deref(), runs[3].underline), ("the docs", Some("https://lpdf.io"), true));
        assert_eq!(runs[5].href.as_deref(), Some("#intro"));
    }

    #[test]
    fn a_thematic_break_is_a_divider() {
        let nodes = nodes("above\n\n---\n\nbelow\n");
        assert_eq!(nodes.iter().map(|n| n.kind.clone()).collect::<Vec<_>>(), [NodeKind::Text, NodeKind::Divider, NodeKind::Text]);
    }

    #[test]
    fn an_image_is_found_by_the_name_of_its_asset_or_by_its_src() {
        for destination in ["chart", "img/chart.png", "./img/chart.png"] {
            let nodes = nodes(&format!("![a chart]({destination})\n"));
            assert_eq!(nodes[0].kind, NodeKind::Img, "{destination}");
            assert_eq!(nodes[0].image_name.as_deref(), Some("chart"), "{destination}");
        }
    }

    #[test]
    fn words_around_an_image_are_blocks_of_their_own() {
        let nodes = nodes("before ![x](chart) after\n");
        assert_eq!(nodes.iter().map(|n| n.kind.clone()).collect::<Vec<_>>(), [NodeKind::Text, NodeKind::Img, NodeKind::Text]);
    }

    #[test]
    fn an_image_that_is_not_an_asset_or_is_a_url_is_an_error_that_names_the_line() {
        let err = error("one\n\n![x](missing.png)\n");
        assert!(err.contains("markdown line 3") && err.contains("neither"), "{err}");
        let err = error("![x](https://example.com/a.png)\n");
        assert!(err.contains("markdown line 1") && err.contains("not a URL"), "{err}");
    }

    #[test]
    fn raw_html_and_math_are_errors_that_name_the_line() {
        let err = error("fine\n\n<div>x</div>\n");
        assert!(err.contains("markdown line 3") && err.contains("raw HTML"), "{err}");
        let err = error("fine <b>x</b>\n");
        assert!(err.contains("markdown line 1") && err.contains("raw HTML"), "{err}");
        let err = error("one\n\n$x^2$ is math\n");
        assert!(err.contains("markdown line 3") && err.contains("math"), "{err}");
    }

    #[test]
    fn a_price_is_not_math() {
        let nodes = nodes("It costs $5 or $6 in all.\n");
        assert_eq!(words(&nodes[0]), "It costs $5 or $6 in all.");
    }

    #[test]
    fn front_matter_is_not_drawn() {
        let nodes = nodes("---\ntitle: Notes\nauthor: Me\n---\n\nBody\n");
        assert_eq!(nodes.len(), 1);
        assert_eq!(words(&nodes[0]), "Body");
    }

    #[test]
    fn a_code_block_keeps_its_spaces_and_lines_and_ignores_the_language() {
        let nodes = nodes("```rust\nfn main() {\n    run();\n}\n```\n");
        assert_eq!(nodes[0].kind, NodeKind::Column);
        let text = &nodes[0].children[0];
        assert_eq!(text.text_runs[0].text, "fn main() {\n    run();\n}");
        assert_eq!(nodes[0].font, "Courier");
    }

    #[test]
    fn an_indented_block_is_code_too_and_a_first_empty_line_is_kept() {
        let nodes = nodes("text\n\n    a\n\n    b\n");
        assert_eq!(nodes[1].children[0].text_runs[0].text, "a\n\nb");
    }

    #[test]
    fn a_bullet_list_and_a_number_list_are_lists_that_nest() {
        let nodes = nodes("- one\n- two\n  - nested\n\n3. three\n4. four\n");
        let all: Vec<String> = nodes.iter().flat_map(|n| texts(n)).map(words).collect();
        // a number list writes its numbers, a bullet list draws shapes
        assert_eq!(all, ["one", "two", "nested", "3.", "three", "4.", "four"]);
        // `lists::lower` made columns and rows of them, a marker beside each item
        assert!(nodes[0].children.iter().all(|item| item.kind == NodeKind::Row));
        let numbers: Vec<String> = nodes[1].children.iter().map(|item| words(texts(item)[0])).collect();
        assert_eq!(numbers, ["3.", "4."]);
    }

    #[test]
    fn a_quote_has_a_bar_beside_each_of_its_blocks() {
        let nodes = nodes("> one\n>\n> two\n");
        assert_eq!(nodes[0].kind, NodeKind::Table);
        assert_eq!(nodes[0].table_cols, "3pt 1fr");
        let rows = &nodes[0].children;
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].children[0].background.as_deref(), Some("#737373"));
        assert_eq!(words(texts(&rows[1])[0]), "two");
        assert_eq!(texts(&rows[0])[0].text_color.as_deref(), Some("#737373"));
    }

    #[test]
    fn a_table_has_a_header_row_and_takes_the_alignment_of_each_column() {
        let nodes = nodes("| Name | Qty | Mid |\n|:-----|----:|:---:|\n| Pen | 2 | x |\n");
        assert_eq!(nodes[0].kind, NodeKind::Table);
        assert_eq!(nodes[0].table_cols, "1fr 1fr 1fr");
        let head = &nodes[0].children[0];
        assert_eq!(head.kind, NodeKind::TableHead);
        assert_eq!(head.children.iter().map(|c| words(texts(c)[0])).collect::<Vec<_>>(), ["Name", "Qty", "Mid"]);
        assert_eq!(head.children.iter().map(|c| texts(c)[0].text_align.clone()).collect::<Vec<_>>(),
            [crate::parse::TextAlign::Left, crate::parse::TextAlign::Right, crate::parse::TextAlign::Center]);
        assert_eq!(texts(&head.children[0])[0].font, "Helvetica-Bold");
        assert_eq!(nodes[0].children[1].kind, NodeKind::TableRow);
    }

    #[test]
    fn a_footnote_is_a_note_at_its_reference_and_a_missing_definition_is_an_error() {
        let nodes = nodes("Claim.[^1]\n\n[^1]: The note.\n");
        assert_eq!(nodes.len(), 1);
        let note = nodes[0].text_runs.iter().find_map(|r| r.footnote.as_ref()).expect("a footnote run");
        assert_eq!(note.runs[0].text, "The note.");
        let err = error("one\n\nClaim.[^2]\n");
        assert!(err.contains("markdown line 3") && err.contains("[^2]") && err.contains("no definition"), "{err}");
    }

    #[test]
    fn a_footnote_in_a_table_cell_is_an_error() {
        let err = error("| a |\n|---|\n| b[^1] |\n\n[^1]: n\n");
        assert!(err.contains("markdown line 3") && err.contains("table"), "{err}");
    }

    #[test]
    fn task_lists_are_text() {
        let nodes = nodes("- [ ] todo\n");
        assert_eq!(words(texts(&nodes[0])[0]), "[ ] todo");
    }

    #[test]
    fn the_indent_of_the_xml_does_not_make_a_code_block() {
        let xml = document("", "", "\n        # Title\n\n        text\n");
        let nodes = nodes_in(&xml);
        assert_eq!(nodes.iter().map(|n| words(n)).collect::<Vec<_>>(), ["Title", "text"]);
    }

    #[test]
    fn dedent_leaves_blank_lines_empty_and_keeps_the_indent_beyond_the_common_one() {
        assert_eq!(dedent("  a\n\n    b\n  "), "a\n\n  b\n");
    }

    #[test]
    fn the_gap_is_the_space_between_the_blocks_of_the_section() {
        let mut doc = parse(&document(r#"gap="10pt""#, "", "a\n\nb\n")).unwrap();
        assert_eq!(doc.section_layouts()[0].gap, 10.0);
        let mut doc = parse(&document("", "", "a\n")).unwrap();
        assert_eq!(doc.section_layouts()[0].gap, 14.0);
    }

    #[test]
    fn the_margin_and_the_font_of_the_markdown_are_its_own() {
        let mut doc = parse(&document(r##"margin="20pt 30pt" font="Times-Roman" font-size="12pt" color="#ff0000""##, "", "a\n")).unwrap();
        let layout = doc.section_layouts().remove(0);
        assert_eq!(layout.margin, [20.0, 30.0, 20.0, 30.0]);
        assert_eq!((layout.children[0].font.as_str(), layout.children[0].font_size), ("Times-Roman", 12.0));
        assert_eq!(layout.children[0].text_color.as_deref(), Some("#ff0000"));
    }

    #[test]
    fn inline_rules_override_the_defaults_one_attribute_at_a_time() {
        let styles = r##"<markdown-styles><h1 color="#ff0000"/><p font-size="l"/></markdown-styles>"##;
        let nodes = nodes_in(&document("", styles, "# T\n\np\n"));
        // the color is new; the size and bold of the default h1 stay
        assert_eq!((nodes[0].text_color.as_deref(), nodes[0].font_size, nodes[0].font.as_str()), (Some("#ff0000"), 28.0, "Helvetica-Bold"));
        assert_eq!(nodes[1].font_size, 14.0);
    }

    #[test]
    fn a_rule_has_the_box_of_its_element_around_the_words() {
        let styles = r##"<markdown-styles><p background="#eeeeee" padding="s"/></markdown-styles>"##;
        let nodes = nodes_in(&document("", styles, "p\n"));
        assert_eq!((nodes[0].kind.clone(), nodes[0].background.as_deref(), nodes[0].padding), (NodeKind::Column, Some("#eeeeee"), [4.0; 4]));
        assert_eq!(words(&nodes[0].children[0]), "p");
    }

    #[test]
    fn a_rule_that_is_not_one_or_an_attribute_it_does_not_take_is_an_error() {
        let err = parse(&document("", "<markdown-styles><h7/></markdown-styles>", "a")).unwrap_err();
        assert!(err.contains("<h7>") && err.contains("unknown rule"), "{err}");
        let err = parse(&document("", r#"<markdown-styles><em href="x"/></markdown-styles>"#, "a")).unwrap_err();
        assert!(err.contains("<em>") && err.contains("unknown attribute 'href'"), "{err}");
    }

    #[test]
    fn a_rule_value_that_does_not_read_is_an_error_when_the_document_is() {
        let err = parse(&document("", r#"<markdown-styles><h1 font-size="huge"/></markdown-styles>"#, "a")).unwrap_err();
        assert!(err.contains("the h1 rule") && err.contains("huge"), "{err}");
    }

    #[test]
    fn a_styles_file_is_laid_under_the_inline_rules_and_over_the_defaults() {
        let xml = document("", r##"<markdown-styles src="styles.xml"><h2 color="#00ff00"/></markdown-styles>"##, "# a\n\n## b\n");
        let file = r##"<markdown-styles><h1 color="#ff0000"/><h2 color="#0000ff" font-size="l"/></markdown-styles>"##;
        let mut resources = Resources::default();
        resources.markdown_styles.insert("styles.xml".into(), file.into());
        let nodes = parse_with(&xml, &resources).unwrap().section_layouts().remove(0).children;
        assert_eq!(nodes[0].text_color.as_deref(), Some("#ff0000"));
        assert_eq!((nodes[1].text_color.as_deref(), nodes[1].font_size), (Some("#00ff00"), 14.0));
    }

    #[test]
    fn a_styles_file_that_was_not_loaded_is_an_error_that_says_so() {
        let err = parse(&document("", r#"<markdown-styles src="styles.xml"/>"#, "a")).unwrap_err();
        assert!(err.contains("src=\"styles.xml\"") && err.contains("did not load"), "{err}");
    }

    #[test]
    fn the_markdown_comes_from_the_data_and_a_document_with_no_data_shows_its_inline_text() {
        let xml = r#"<lpdf version="1"><document><section><markdown>
            <markdown-content data-value="body"><![CDATA[# Fallback]]></markdown-content></markdown></section></document></lpdf>"#;
        let mut doc = parse(xml).unwrap();
        crate::data::apply(&mut doc, r##"{"body": "# From data\n\ntext"}"##).unwrap();
        let nodes = doc.section_layouts().remove(0).children;
        assert_eq!(nodes.iter().map(words).collect::<Vec<_>>(), ["From data", "text"]);
        assert_eq!(nodes[0].bookmarks[0].id, "from-data");

        assert_eq!(parse(xml).unwrap().section_layouts()[0].children.iter().map(words).collect::<Vec<_>>(), ["Fallback"]);

        let mut doc = parse(xml).unwrap();
        crate::data::apply(&mut doc, r#"{"other": 1}"#).unwrap();
        assert!(doc.section_layouts()[0].children.is_empty());
    }

    #[test]
    fn data_that_is_not_a_string_is_an_error_that_names_the_path() {
        let xml = r#"<lpdf version="1"><document><section><markdown>
            <markdown-content data-value="body"/></markdown></section></document></lpdf>"#;
        let mut doc = parse(xml).unwrap();
        let err = crate::data::apply(&mut doc, r#"{"body": 3}"#).unwrap_err();
        assert!(err.contains("data-value=\"body\"") && err.contains("a number"), "{err}");
    }

    #[test]
    fn markdown_from_the_data_is_read_for_its_errors_like_markdown_written_inline() {
        let xml = r#"<lpdf version="1"><document><section><markdown>
            <markdown-content data-value="body"/></markdown></section></document></lpdf>"#;
        let mut doc = parse(xml).unwrap();
        let err = crate::data::apply(&mut doc, r#"{"body": "ok\n\n<b>x</b>"}"#).unwrap_err();
        assert!(err.contains("markdown line 3") && err.contains("raw HTML"), "{err}");
    }

    #[test]
    fn a_section_has_a_layout_or_a_markdown_and_not_both_and_a_markdown_needs_its_content() {
        let both = r#"<lpdf version="1"><document><section><layout/><markdown><markdown-content/></markdown></section></document></lpdf>"#;
        assert!(parse(both).unwrap_err().contains("not both"));
        let both = r#"<lpdf version="1"><document><section><markdown><markdown-content/></markdown><layout/></section></document></lpdf>"#;
        assert!(parse(both).unwrap_err().contains("not both"));
        let bare = r#"<lpdf version="1"><document><section><markdown/></section></document></lpdf>"#;
        assert!(parse(bare).unwrap_err().contains("needs a <markdown-content>"));
        let odd = r#"<lpdf version="1"><document><section><markdown><text/></markdown></section></document></lpdf>"#;
        assert!(parse(odd).unwrap_err().contains("unexpected child <text>"));
    }

    #[test]
    fn layers_paint_over_and_under_the_markdown_where_they_are_written() {
        let xml = r##"<lpdf version="1"><document><section>
            <layer><rect x="0pt" y="0pt" width="10pt" height="10pt" fill="#000000"/></layer>
            <markdown><markdown-content>hi</markdown-content></markdown>
            <layer><rect x="0pt" y="0pt" width="10pt" height="10pt" fill="#000000"/></layer>
            </section></document></lpdf>"##;
        let layout = parse(xml).unwrap().section_layouts().remove(0);
        assert_eq!((layout.underlays.len(), layout.overlays.len()), (1, 1));
    }

    #[test]
    fn a_tree_reads_a_markdown_as_xml_does() {
        let tree = r##"{"version":1,"type":"document","nodes":[{"type":"section","nodes":[{"type":"markdown","attrs":{"gap":"8pt"},"nodes":[
            {"type":"markdown-styles","attrs":{},"nodes":[{"type":"h1","attrs":{"color":"#ff0000"}}]},
            {"type":"markdown-content","attrs":{},"nodes":["    # Title\n\n    text"]}]}]}]}"##;
        let mut doc = crate::parse::parse_tree(tree).unwrap();
        let layout = doc.section_layouts().remove(0);
        assert_eq!(layout.gap, 8.0);
        assert_eq!(layout.children.iter().map(words).collect::<Vec<_>>(), ["Title", "text"]);
        assert_eq!(layout.children[0].text_color.as_deref(), Some("#ff0000"));
    }

    #[test]
    fn the_default_template_and_styles_are_documents_the_engine_reads() {
        let mut doc = parse(default_template()).unwrap();
        assert_eq!(doc.section_layouts().len(), 1);
        let styles = roxmltree::Document::parse(default_styles()).unwrap();
        assert_eq!(styles.root_element().tag_name().name(), "markdown-styles");
    }

    #[test]
    fn the_default_template_renders_a_file_from_the_data_it_is_given() {
        let mut doc = parse(default_template()).unwrap();
        crate::data::apply(&mut doc, r##"{"markdown": "# Hello\n\nworld"}"##).unwrap();
        let layout = doc.section_layouts().remove(0);
        assert_eq!(layout.children.iter().map(words).collect::<Vec<_>>(), ["Hello", "world"]);
        assert_eq!(layout.margin, [54.0; 4]);
    }

    #[test]
    fn a_long_markdown_renders_to_a_pdf_on_several_pages_with_the_gap_between_blocks() {
        let body: String = (1..=120).map(|n| format!("Paragraph number {n} of the long text.\n\n")).collect();
        let xml = document("", "", &body);
        let pdf = crate::LpdfEngine::render_xml_to_pdf_bytes(&xml).unwrap();
        assert!(pdf.starts_with(b"%PDF"));
        let pages = crate::layout::layout_document(&parse(&xml).unwrap().section_layouts());
        assert!(pages.len() > 2, "{} pages", pages.len());
    }

    #[test]
    fn markdown_does_not_change_a_document_without_it() {
        // the gap of a layout is none, as before
        let xml = r#"<lpdf version="1"><document><section><layout><text>a</text></layout></section></document></lpdf>"#;
        assert_eq!(parse(xml).unwrap().section_layouts()[0].gap, 0.0);
    }
}
