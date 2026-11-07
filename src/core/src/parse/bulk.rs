//! The rules of a bulk container: `bulk="true"` on a table, a list, a grid or a column, whose child with `data-source`
//! is a template the layout binds a record at a time, a page of them at a time, from the data (`layout::bulk`). It can
//! because every record holds plain texts of a height the template bounds, and nothing that reaches past its record.
//! The rules are checked once, on the template, when the document is read. A rule broken is an error that names it,
//! never a quiet fall back to the general layout, which a customer would find out about at 300,000 records.

use super::{
    ColumnFill, HeightMode, LayoutChild, Node, NodeKind, Output, Paginate, ParsedNode, RowKeep, Section, SectionChild,
    DEFAULT_ORPHANS, DEFAULT_WIDOWS,
};

impl ParsedNode {
    /// Whether the element has `bulk="true"`.
    pub(super) fn is_bulk(&self) -> bool {
        self.data_attrs.as_ref().is_some_and(|d| d.bulk)
    }

    fn has_data_source(&self) -> bool {
        self.data_attrs.as_ref().is_some_and(|d| d.data_source.is_some())
    }
}

/// What a bulk container of `kind` is called in an error: `table` in "a bulk table".
fn name(kind: &NodeKind) -> &'static str {
    match kind {
        NodeKind::Table => "table",
        NodeKind::List  => "list",
        NodeKind::Grid  => "grid",
        _               => "column",
    }
}

/// The child a bulk container of `kind` repeats, as an error names it.
fn repeated(kind: &NodeKind) -> &'static str {
    match kind {
        NodeKind::Table => "tr",
        NodeKind::List  => "li",
        NodeKind::Grid  => "child",
        _               => "text",
    }
}

/// The error of a bulk container of `kind` that breaks `rule`, with what breaks it.
fn broken(kind: &NodeKind, rule: &str, what: String) -> String {
    format!("a bulk {} {rule}: {what}", name(kind))
}

/// What a bulk `container` says when it is written in `parent`, not directly in the layout.
pub(super) fn not_in(container: &ParsedNode, parent: &str) -> String {
    broken(&container.kind, "sits directly in the layout", format!("this one is in a {parent}"))
}

const ONLY_TEXT: &str = "holds only text";
const ONE_TEXT_A_CELL: &str = "holds one text in a cell, since a column of them is nesting";
const ONE_LEVEL: &str = "has one level";
const PLAIN_WORDS: &str = "holds plain words";
const NO_LINKS: &str = "has no links, bookmarks, footnotes or references";
const NEVER_SPLITS: &str = "never splits a record across pages, so it sets no widows or orphans";
const NO_ROWSPAN: &str = "has no rowspan, which ties records together";
const NO_PAGINATE: &str = "has no paginate, and neither does what is in it";
const OWN_HEIGHT: &str = "takes its height from its records";
const COLS: &str = "sets its cols in fr, pt, mm, in or %";

/// Check the rules on a bulk container that the parser has read, children and all, and give each text of it that
/// sets no `max-lines` one line, ending in an ellipsis, so that a record stays as high as the template says.
pub(super) fn check(node: &mut ParsedNode) -> Result<(), String> {
    let kind = node.kind.clone();
    let source = format!("repeats one {} with data-source", repeated(&kind));
    if node.has_data_source() {
        return Err(broken(&kind, &source, "the data-source goes on that child, not on the container".to_string()));
    }
    match node.children.iter().filter(|child| child.has_data_source()).count() {
        1 => {}
        0 => return Err(broken(&kind, &source, "it has none".to_string())),
        n => return Err(broken(&kind, &source, format!("it has {n}"))),
    }
    if node.height_mode != HeightMode::Auto {
        return Err(broken(&kind, OWN_HEIGHT, "it has a height".to_string()));
    }
    if kind == NodeKind::Table && node.table_cols.split_whitespace().any(|unit| unit == "auto") {
        return Err(broken(&kind, COLS, "an auto column would need every record first".to_string()));
    }
    if node.paginate != Paginate::None {
        return Err(broken(&kind, NO_PAGINATE, format!("it has paginate=\"{}\"", paginate_value(&node.paginate))));
    }
    for (i, child) in node.children.iter().enumerate() {
        let place = if child.has_data_source() {
            format!("repeated {}", noun_of_child(&kind, child))
        } else if child.kind == NodeKind::TableHead {
            "header row".to_string()
        } else {
            format!("{} {}", ordinal(i + 1), noun_of_child(&kind, child))
        };
        Record { kind: &kind, path: vec![place] }.check_child(child)?;
    }
    one_line_texts(node);
    Ok(())
}

/// What `child`, of a bulk container of `kind`, is called where an error says which one breaks a rule.
fn noun_of_child(kind: &NodeKind, child: &ParsedNode) -> &'static str {
    match kind {
        NodeKind::Table => "row",
        NodeKind::List  => "item",
        NodeKind::Grid  => "cell",
        _ if child.kind == NodeKind::Text => "text",
        _               => "child",
    }
}

/// A child of a bulk container being checked: the container's kind, and where the node being checked is, from the
/// container down, as an error says it: `repeated row`, `second cell`, `text`.
struct Record<'a> {
    kind: &'a NodeKind,
    path: Vec<String>,
}

impl Record<'_> {
    /// The error for `rule`, at the node the path leads to, which `does` what breaks it: "its repeated row's second
    /// cell holds a list".
    fn error(&self, rule: &str, does: &str) -> String {
        broken(self.kind, rule, format!("its {} {does}", self.path.join("'s ")))
    }

    /// The same, one step further down, at `place`.
    fn at(&mut self, place: String) -> &mut Self {
        self.path.push(place);
        self
    }

    fn up(&mut self) {
        self.path.pop();
    }

    /// Check a child of the container: a row of a table, an item of a list, a cell of a grid or a text of a column.
    fn check_child(&mut self, child: &ParsedNode) -> Result<(), String> {
        self.check_box(child, false)?;
        match self.kind {
            NodeKind::Table => {
                if child.keep == RowKeep::WithNext {
                    return Err(self.error(NO_PAGINATE, "has keep=\"with-next\""));
                }
                for (c, cell) in child.children.iter().enumerate() {
                    self.at(format!("{} cell", ordinal(c + 1)));
                    self.check_box(cell, true)?;
                    if cell.row_span > 1 {
                        return Err(self.error(NO_ROWSPAN, &format!("has rowspan=\"{}\"", cell.row_span)));
                    }
                    match cell.children.as_slice() {
                        [] => {}
                        [text] if text.kind == NodeKind::Text => {
                            self.at("text".to_string());
                            self.check_text(text)?;
                            self.up();
                        }
                        [other] => return Err(self.holds(other)),
                        many if many.iter().all(|n| n.kind == NodeKind::Text) => {
                            return Err(self.error(ONE_TEXT_A_CELL, &format!("holds {}", many.len())));
                        }
                        many => {
                            let other = many.iter().find(|n| n.kind != NodeKind::Text).unwrap_or(&many[0]);
                            return Err(self.holds(other));
                        }
                    }
                    self.up();
                }
                Ok(())
            }
            NodeKind::List => self.check_texts(&child.children),
            NodeKind::Grid => match child.kind {
                NodeKind::Text => self.check_text(child),
                NodeKind::Column if !is_code(child) => self.check_texts(&child.children),
                _ => Err(self.error(ONLY_TEXT, &format!("is {}", what(child)))),
            },
            _ => match child.kind {
                NodeKind::Text => self.check_text(child),
                _ => Err(self.error(ONLY_TEXT, &format!("is {}", what(child)))),
            },
        }
    }

    /// Check that `nodes`, what an item of a list or a cell of a grid holds, are texts.
    fn check_texts(&mut self, nodes: &[ParsedNode]) -> Result<(), String> {
        let several = nodes.len() > 1;
        for (i, node) in nodes.iter().enumerate() {
            if node.kind != NodeKind::Text {
                return Err(self.holds(node));
            }
            self.at(if several { format!("{} text", ordinal(i + 1)) } else { "text".to_string() });
            self.check_text(node)?;
            self.up();
        }
        Ok(())
    }

    /// The error for a node that holds `other` where a text goes: a list, a table or a grid inside a record is
    /// nesting, anything else is not text.
    fn holds(&self, other: &ParsedNode) -> String {
        let rule = if matches!(other.kind, NodeKind::List | NodeKind::Table | NodeKind::Grid) { ONE_LEVEL } else { ONLY_TEXT };
        self.error(rule, &format!("holds {}", what(other)))
    }

    /// The rules of any box in a record, and of a text: what it may not have whatever it is. `inside` is a node
    /// inside a child of the container, which may not repeat with a data-source of its own.
    fn check_box(&self, node: &ParsedNode, inside: bool) -> Result<(), String> {
        if node.paginate != Paginate::None {
            return Err(self.error(NO_PAGINATE, &format!("has paginate=\"{}\"", paginate_value(&node.paginate))));
        }
        if inside && node.has_data_source() {
            return Err(self.error(&format!("repeats one {} with data-source", repeated(self.kind)), "has a data-source of its own"));
        }
        if !node.bookmarks.is_empty() {
            return Err(self.error(NO_LINKS, "has a bookmark"));
        }
        Ok(())
    }

    /// The rules of a text of a record: plain words, and nothing that reaches past the record.
    fn check_text(&self, text: &ParsedNode) -> Result<(), String> {
        // A text that is the child itself, the record of a column or a grid, repeats with its data-source.
        self.check_box(text, self.path.len() > 1)?;
        if text.widows != DEFAULT_WIDOWS {
            return Err(self.error(NEVER_SPLITS, "sets widows"));
        }
        if text.orphans != DEFAULT_ORPHANS {
            return Err(self.error(NEVER_SPLITS, "sets orphans"));
        }
        for run in &text.text_runs {
            // A reference links to its bookmark, so it is told apart from a link first.
            if run.reference.is_some() {
                return Err(self.error(NO_LINKS, "holds a reference to a bookmark"));
            }
            if run.href.is_some() {
                return Err(self.error(NO_LINKS, "holds a link"));
            }
            if run.footnote.is_some() {
                return Err(self.error(NO_LINKS, "holds a footnote"));
            }
            if run.font.is_some() || run.color.is_some() || run.underline || run.strike || run.bold || run.background.is_some() {
                return Err(self.error(PLAIN_WORDS, "holds a span"));
            }
        }
        Ok(())
    }
}

/// Whether `node` is a `<code>`, which the parser makes a column around one text that keeps its spaces.
fn is_code(node: &ParsedNode) -> bool {
    node.kind == NodeKind::Column && node.children.len() == 1 && node.children[0].pre_tab > 0
}

/// What `node` is, as an error says it: `a list`, `an image`, `code`.
fn what(node: &ParsedNode) -> &'static str {
    if is_code(node) {
        return "code";
    }
    match node.kind {
        NodeKind::Column    => "a column",
        NodeKind::Row       => "a row",
        NodeKind::Grid      => "a grid",
        NodeKind::Frame     => "a frame",
        NodeKind::Divider   => "a divider",
        NodeKind::Text      => "a text",
        NodeKind::Link      => "a link",
        NodeKind::Img       => "an image",
        NodeKind::Shape     => "a shape",
        NodeKind::Barcode   => "a barcode",
        NodeKind::Field     => "a field",
        NodeKind::Table     => "a table",
        NodeKind::TableHead => "a header row",
        NodeKind::TableRow  => "a row",
        NodeKind::TableCell => "a cell",
        NodeKind::List      => "a list",
        NodeKind::ListItem  => "an item",
        NodeKind::Markdown  => "Markdown",
    }
}

fn paginate_value(paginate: &Paginate) -> &'static str {
    match paginate {
        Paginate::None        => "",
        Paginate::No          => "no",
        Paginate::BreakBefore => "break-before",
        Paginate::BreakAfter  => "break-after",
        Paginate::KeepNext    => "keep-next",
    }
}

/// `n` as a word that counts, from 1: `first`, `second`, then `11th`.
fn ordinal(n: usize) -> String {
    const WORDS: [&str; 10] = ["first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth", "ninth", "tenth"];
    match WORDS.get(n.wrapping_sub(1)) {
        Some(word) => (*word).to_string(),
        None => {
            let suffix = match (n % 10, n % 100) {
                (_, 11..=13) => "th",
                (1, _) => "st",
                (2, _) => "nd",
                (3, _) => "rd",
                _ => "th",
            };
            format!("{n}{suffix}")
        }
    }
}

/// Give every text below `node` that sets no `max-lines` one line, which ends in an ellipsis when its words are
/// longer, so that no value is cut without showing it.
fn one_line_texts(node: &mut ParsedNode) {
    if node.kind == NodeKind::Text && node.max_lines.is_none() {
        node.max_lines = Some(1);
        node.ellipsis = true;
    }
    node.children.iter_mut().for_each(one_line_texts);
}

/// Check the rules a bulk container sets for the document around it: it is laid out once, untagged, and not in
/// balanced columns.
pub(super) fn check_document(sections: &[Section], output: &Output) -> Result<(), String> {
    let layout = || sections.iter().flat_map(|section| {
        section.children.iter().filter_map(move |child| match child {
            SectionChild::Layout { children, .. } => Some((section, children)),
            _ => None,
        })
    });
    let found: Vec<(&Section, &NodeKind)> = layout()
        .flat_map(|(section, children)| children.iter().map(move |child| (section, child)))
        .filter_map(|(section, child)| match child {
            LayoutChild::Content(node) if node.data_attrs.as_ref().is_some_and(|d| d.bulk) => Some((section, &node.kind)),
            _ => None,
        })
        .collect();
    let Some(&(_, kind)) = found.first() else { return Ok(()) };
    let once = "is laid out once";
    if sections.iter().any(|s| s.options.toc.is_some()) {
        return Err(broken(kind, once, "a table of contents lays its document out again until the page count settles, so take out the section type=\"toc\" or bulk".to_string()));
    }
    // Only a document with a bulk container is walked for references: every other one would pay for it.
    let references = layout().flat_map(|(_, children)| children).any(|child| match child {
        LayoutChild::Content(node) => has_reference(node),
        LayoutChild::Region(region) => region.children.iter().any(has_reference),
    });
    if references {
        return Err(broken(kind, once, "a reference to a bookmark lays its document out again until the page numbers settle, so take out the bookmark-ref or bulk".to_string()));
    }
    if crate::pdfa::Standards::of(output).tagged() {
        return Err(broken(kind, "is not tagged for a reader", "pdfua and a pdfa of level a tag every record, so render it without bulk, or take a pdfa of level b or u".to_string()));
    }
    let balanced = |section: &Section| section.options.columns.count > 1 && section.options.columns.fill == ColumnFill::Balance;
    if let Some((_, kind)) = found.iter().find(|(section, _)| balanced(section)) {
        return Err(broken(kind, "fills each page once", "balanced columns fill the last page again to level it, so set column-fill=\"auto\" or take out bulk".to_string()));
    }
    Ok(())
}

/// Whether a text at `node` or below it holds a reference to a bookmark.
fn has_reference(node: &Node) -> bool {
    node.text_runs.iter().any(|run| run.reference.is_some()) || node.children.iter().any(has_reference)
}

#[cfg(test)]
mod tests {
    use crate::parse::{parse, parse_tree};

    /// The error of a document whose layout holds `body`, after `head` in the document.
    fn error_of(head: &str, section: &str, body: &str) -> String {
        let xml = format!(r#"<lpdf version="1"><document size="a4">{head}<section{section}><layout>{body}</layout></section></document></lpdf>"#);
        parse(&xml).expect_err("the document breaks a rule of bulk")
    }

    /// The error of a layout that holds `body`.
    fn error(body: &str) -> String {
        error_of("", "", body)
    }

    fn assert_says(error: &str, says: &str) {
        assert!(error.contains(says), "the error says {error:?}, not {says:?}");
    }

    const ROW: &str = r#"<tr data-source="entries"><td><text data-value="date">-</text></td><td><text data-value="amount">-</text></td></tr>"#;

    #[test]
    fn a_bulk_container_repeats_one_child_with_data_source() {
        assert_says(&error(r#"<table cols="1fr" bulk="true"><tr><td><text>A</text></td></tr></table>"#),
            "a bulk table repeats one tr with data-source: it has none");
        assert_says(&error(&format!(r#"<table cols="1fr 1fr" bulk="true">{ROW}{ROW}</table>"#)),
            "a bulk table repeats one tr with data-source: it has 2");
        assert_says(&error(&format!(r#"<table cols="1fr 1fr" bulk="true" data-source="ledgers">{ROW}</table>"#)),
            "a bulk table repeats one tr with data-source: the data-source goes on that child, not on the container");
        assert_says(&error(r#"<list bulk="true"><li data-source="a"><text>A</text></li><li><text data-source="b">B</text></li></list>"#),
            "a bulk list repeats one li with data-source: its second item's text has a data-source of its own");
    }

    #[test]
    fn a_bulk_container_sits_directly_in_the_layout() {
        assert_says(&error(&format!(r#"<column><table cols="1fr 1fr" bulk="true">{ROW}</table></column>"#)),
            "<table> at line 1, col 64: a bulk table sits directly in the layout: this one is in a <column>");
        assert_says(&error(&format!(r#"<region pin="top"><table cols="1fr 1fr" bulk="true">{ROW}</table></region>"#)),
            "a bulk table sits directly in the layout: this one is in a <region>");
    }

    #[test]
    fn a_bulk_table_sets_its_columns_without_auto() {
        assert_says(&error(&format!(r#"<table cols="auto 60pt" bulk="true">{ROW}</table>"#)),
            "a bulk table sets its cols in fr, pt, mm, in or %: an auto column would need every record first");
    }

    #[test]
    fn a_bulk_container_takes_its_height_from_its_records() {
        assert_says(&error(r#"<column height="400pt" bulk="true"><text data-source="lines">-</text></column>"#),
            "a bulk column takes its height from its records: it has a height");
    }

    #[test]
    fn a_bulk_container_and_what_is_in_it_have_no_paginate() {
        assert_says(&error(&format!(r#"<table cols="1fr 1fr" paginate="break-before" bulk="true">{ROW}</table>"#)),
            "a bulk table has no paginate, and neither does what is in it: it has paginate=\"break-before\"");
        assert_says(&error(r#"<column bulk="true"><text data-source="lines" paginate="keep-next">-</text></column>"#),
            "a bulk column has no paginate, and neither does what is in it: its repeated text has paginate=\"keep-next\"");
        assert_says(&error(r#"<table cols="1fr" bulk="true"><tr data-source="e" keep="with-next"><td><text>-</text></td></tr></table>"#),
            "a bulk table has no paginate, and neither does what is in it: its repeated row has keep=\"with-next\"");
    }

    #[test]
    fn a_bulk_container_holds_only_text() {
        assert_says(&error(r#"<table cols="1fr 1fr" bulk="true"><tr data-source="e"><td><text>-</text></td><td><shape type="rect" width="8pt" height="8pt"/></td></tr></table>"#),
            "a bulk table holds only text: its repeated row's second cell holds a shape");
        assert_says(&error(r#"<grid cols="2" bulk="true"><row data-source="e"><text>-</text></row></grid>"#),
            "a bulk grid holds only text: its repeated cell is a row");
        assert_says(&error(r#"<grid cols="2" bulk="true"><column data-source="e"><text>-</text><divider/></column></grid>"#),
            "a bulk grid holds only text: its repeated cell holds a divider");
        assert_says(&error(r#"<column bulk="true"><text>Title</text><code data-source="e">x</code></column>"#),
            "a bulk column holds only text: its repeated child is code");
        assert_says(&error(r#"<list bulk="true"><li data-source="e"><text>-</text><link href="https://a.example"><text>Go</text></link></li></list>"#),
            "a bulk list holds only text: its repeated item holds a link");
    }

    #[test]
    fn a_bulk_container_has_one_level() {
        assert_says(&error(r#"<list bulk="true"><li data-source="e"><text>-</text><list><li><text>Inner</text></li></list></li></list>"#),
            "a bulk list has one level: its repeated item holds a list");
        assert_says(&error(r#"<table cols="1fr" bulk="true"><tr data-source="e"><td><table cols="1fr"><tr><td><text>-</text></td></tr></table></td></tr></table>"#),
            "a bulk table has one level: its repeated row's first cell holds a table");
    }

    #[test]
    fn a_bulk_table_holds_one_text_in_a_cell() {
        assert_says(&error(r#"<table cols="1fr" bulk="true"><tr data-source="e"><td><text>-</text><text>-</text></td></tr></table>"#),
            "a bulk table holds one text in a cell, since a column of them is nesting: its repeated row's first cell holds 2");
    }

    #[test]
    fn a_bulk_container_holds_plain_words() {
        assert_says(&error(r#"<column bulk="true"><text data-source="e">A <span color="primary">b</span></text></column>"#),
            "a bulk column holds plain words: its repeated text holds a span");
    }

    #[test]
    fn a_bulk_container_has_no_links_bookmarks_footnotes_or_references() {
        let rule = "a bulk table has no links, bookmarks, footnotes or references";
        assert_says(&error(r#"<table cols="1fr" bulk="true"><thead><td><bookmark label="Here"/><text>-</text></td></thead><tr data-source="e"><td><text>-</text></td></tr></table>"#),
            &format!("{rule}: its header row's first cell's text has a bookmark"));
        assert_says(&error(r#"<column bulk="true"><text data-source="e">-<fn>A note</fn></text></column>"#),
            "a bulk column has no links, bookmarks, footnotes or references: its repeated text holds a footnote");
        assert_says(&error(r#"<column bulk="true"><text data-source="e"><span href="https://a.example">Go</span></text></column>"#),
            "a bulk column has no links, bookmarks, footnotes or references: its repeated text holds a link");
        assert_says(&error(r#"<bookmark id="t" label="T"/><text>T</text><table cols="1fr" bulk="true"><tr data-source="e"><td><text>See <span bookmark-ref="t">page {bookmark-page}</span></text></td></tr></table>"#),
            &format!("{rule}: its repeated row's first cell's text holds a reference to a bookmark"));
    }

    #[test]
    fn a_bulk_container_never_splits_a_record() {
        assert_says(&error(r#"<column bulk="true"><text data-source="e" widows="3">-</text></column>"#),
            "a bulk column never splits a record across pages, so it sets no widows or orphans: its repeated text sets widows");
        assert_says(&error(r#"<list bulk="true"><li data-source="e"><text orphans="1">-</text></li></list>"#),
            "a bulk list never splits a record across pages, so it sets no widows or orphans: its repeated item's text sets orphans");
    }

    #[test]
    fn a_bulk_table_has_no_rowspan() {
        assert_says(&error(r#"<table cols="1fr 1fr" bulk="true"><tr data-source="e"><td rowspan="2"><text>-</text></td><td><text>-</text></td></tr><tr><td><text>-</text></td></tr></table>"#),
            "a bulk table has no rowspan, which ties records together: its repeated row's first cell has rowspan=\"2\"");
    }

    #[test]
    fn a_bulk_container_is_laid_out_once() {
        let table = format!(r#"<table cols="1fr 1fr" bulk="true">{ROW}</table>"#);
        let toc = r#"<section type="toc"><layout><column data-source="$toc"><text data-value="label">-</text></column></layout></section>"#;
        assert_says(&error_of(toc, "", &table),
            "a bulk table is laid out once: a table of contents lays its document out again until the page count settles");
        let reference = format!(r#"<bookmark id="t" label="T"/><text>T, on page <span bookmark-ref="t">{{bookmark-page}}</span></text>{table}"#);
        assert_says(&error(&reference),
            "a bulk table is laid out once: a reference to a bookmark lays its document out again until the page numbers settle");
    }

    #[test]
    fn a_bulk_container_is_not_tagged() {
        let table = format!(r#"<table cols="1fr 1fr" bulk="true">{ROW}</table>"#);
        for output in [r#"<meta title="T" lang="en"/><output pdfua="1"/>"#, r#"<output pdfa="2a"/>"#] {
            assert_says(&error_of(output, "", &table), "a bulk table is not tagged for a reader: pdfua and a pdfa of level a tag every record");
        }
        let untagged = format!(r#"<lpdf version="1"><document size="a4"><output pdfa="2b"/><section><layout>{table}</layout></section></document></lpdf>"#);
        assert!(parse(&untagged).is_ok(), "a pdfa of level b is not tagged");
    }

    #[test]
    fn a_bulk_container_is_not_in_balanced_columns() {
        let table = format!(r#"<table cols="1fr 1fr" bulk="true">{ROW}</table>"#);
        assert_says(&error_of("", r#" columns="2" column-fill="balance""#, &table),
            "a bulk table fills each page once: balanced columns fill the last page again to level it");
    }

    #[test]
    fn a_kit_tree_has_no_data_for_a_bulk_container() {
        let tree = r#"{"version":1,"type":"document","nodes":[{"type":"section","nodes":[{"type":"layout","nodes":[
            {"type":"column","attrs":{"bulk":true},"nodes":[{"type":"text","nodes":[{"type":"run","text":"A"}]}]}]}]}]}"#;
        let error = parse_tree(tree).expect_err("a kit tree binds no data");
        assert_says(&error, "column: a bulk column repeats one text with data-source: it has none");
    }

    #[test]
    fn a_text_of_a_bulk_container_that_sets_no_max_lines_holds_one_line() {
        let xml = r#"<lpdf version="1"><document size="a4"><section><layout><column bulk="true"><text data-source="e">-</text><text max-lines="3">-</text></column></layout></section></document></lpdf>"#;
        let mut doc = parse(xml).unwrap();
        let column = doc.section_layouts().remove(0).children.remove(0);
        let texts: Vec<(Option<usize>, bool)> = column.children.iter().map(|text| (text.max_lines, text.ellipsis)).collect();
        assert_eq!(texts, [(Some(1), true), (Some(3), false)]);
    }
}
