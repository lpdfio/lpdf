// Cross-references: a `<span bookmark-ref="id">` links to a bookmark and shows, in its words, the page the bookmark
// is on as the footer of that page prints it (`{bookmark-page}`) and the bookmark's label (`{bookmark-label}`).
//
// Where a bookmark lands depends on the layout, and the words of a reference are part of it: a longer number can
// wrap a line and push the bookmark onto the next page. So a document with references is laid out, each reference
// is given the page its bookmark landed on, and the document is laid out again, until every reference shows the
// page its bookmark is on. A reference to a later page can move its bookmark back and forth for ever, "viii"
// wrapping its line and pushing the bookmark to ix, and "ix" unwrapping it again, so each reference keeps room on
// its line for the widest words it has shown (`Reference::widest`). The room only grows, so the layout stops
// moving, at worst with a small gap after a narrower number. A document with no reference is laid out once.

use std::collections::HashMap;

use crate::canvas::CanvasPrimitive;
use crate::layout::{self, text_width};
use crate::page_numbers;
use crate::parse::{Node, Reference, SectionLayout, TextRun, BOOKMARK_LABEL, BOOKMARK_PAGE};
use crate::render::{placed_anchors, RenderPage};

/// More passes than a document needs: after the first, each pass that does not settle moves a bookmark that a
/// reference names, or widens the room a reference keeps.
const MAX_PASSES: usize = 8;

/// Whether a text of `sections` holds a reference.
pub(crate) fn any(sections: &[SectionLayout]) -> bool {
    let mut found = false;
    for_each_text(sections, &mut |node| found = found || runs_of(node).any(|run| run.reference.is_some()));
    found
}

/// Fails on a reference that names no bookmark of `sections`, with the id and where the reference is written.
pub(crate) fn check(sections: &[SectionLayout]) -> Result<(), String> {
    let labels = bookmark_labels(sections);
    let mut unknown: Option<&Reference> = None;
    for_each_text(sections, &mut |node| {
        if unknown.is_none() {
            unknown = runs_of(node).filter_map(|run| run.reference.as_deref()).find(|r| !labels.contains_key(&r.id));
        }
    });
    match unknown {
        Some(reference) => Err(format!("{}: bookmark-ref '{}' names no bookmark", reference.place, reference.id)),
        None            => Ok(()),
    }
}

/// Lay out `sections`, which hold references, until each reference shows the page its bookmark is on.
pub(crate) fn layout_with_references(sections: &[SectionLayout]) -> Vec<RenderPage> {
    let mut sections = sections.to_vec();
    let labels = bookmark_labels(&sections);
    // Nothing is laid out yet, so the first pass shows no page.
    fill_references(&mut sections, &labels, &HashMap::new());
    let mut laid = layout::lay_out_document(&sections);
    for _ in 1..MAX_PASSES {
        let pages = bookmark_pages(&laid);
        if !fill_references(&mut sections, &labels, &pages) {
            break;
        }
        laid = layout::lay_out_document(&sections);
    }
    laid.pages
}

/// The page each bookmark of `laid` is on, by id, as the footer of that page prints it.
fn bookmark_pages(laid: &layout::LaidDocument) -> HashMap<String, String> {
    let printed = page_numbers::page_labels(&laid.numbers, &laid.counts);
    placed_anchors(&laid.pages).into_iter()
        .map(|placed| (placed.anchor.id.clone(), printed[placed.page].clone()))
        .collect()
}

/// The label of each bookmark of `sections`, by id. Where a `data-source` repeats one, the first is the one that
/// links reach.
fn bookmark_labels(sections: &[SectionLayout]) -> HashMap<String, String> {
    let mut labels = HashMap::new();
    let mut add = |bookmark: &crate::parse::Bookmark| {
        labels.entry(bookmark.id.clone()).or_insert_with(|| bookmark.label.clone());
    };
    for section in sections {
        section.bookmark.iter().for_each(&mut add);
        for node in &section.children {
            for_each_node(node, &mut |node| node.bookmarks.iter().for_each(&mut add));
        }
    }
    labels
}

/// Give each reference of `sections` its words with the page of its bookmark in `pages` and the label in `labels`,
/// and the room for them. Returns whether the words of any reference changed.
fn fill_references(sections: &mut [SectionLayout], labels: &HashMap<String, String>, pages: &HashMap<String, String>) -> bool {
    let mut changed = false;
    for_each_text_mut(sections, &mut |node| {
        for run in &mut node.text_runs {
            changed |= fill_run(run, &node.font, labels, pages);
            for note_run in run.footnote.iter_mut().flat_map(|note| note.runs.iter_mut()) {
                changed |= fill_run(note_run, &node.font, labels, pages);
            }
        }
    });
    changed
}

/// `fill_references` for one run of a text set in `font`.
fn fill_run(run: &mut TextRun, font: &str, labels: &HashMap<String, String>, pages: &HashMap<String, String>) -> bool {
    let Some(reference) = run.reference.as_deref_mut() else { return false };
    let page = pages.get(&reference.id).map_or("", String::as_str);
    let label = labels.get(&reference.id).map_or("", String::as_str);
    let words = fill(&reference.words, page, label);
    // The widths compare the same at any size.
    let font = run.font.as_deref().unwrap_or(font);
    if text_width(font, &words, 1.0) > text_width(font, &reference.widest, 1.0) {
        reference.widest = words.clone();
    }
    reference.shown = words.clone();
    if run.text == words {
        return false;
    }
    run.text = words;
    true
}

/// `words` with each `{bookmark-page}` replaced by `page` and each `{bookmark-label}` by `label`. What they are
/// replaced by is not read again for tokens.
fn fill(words: &str, page: &str, label: &str) -> String {
    let mut out = String::with_capacity(words.len());
    let mut rest = words;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        if let Some(after) = rest.strip_prefix(BOOKMARK_PAGE) {
            out.push_str(page);
            rest = after;
        } else if let Some(after) = rest.strip_prefix(BOOKMARK_LABEL) {
            out.push_str(label);
            rest = after;
        } else {
            out.push('{');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}

/// The runs of a text, with those of its footnotes' notes.
fn runs_of(node: &Node) -> impl Iterator<Item = &TextRun> {
    node.text_runs.iter().flat_map(|run| {
        std::iter::once(run).chain(run.footnote.iter().flat_map(|note| note.runs.iter()))
    })
}

/// Call `visit` with every node of the layouts of `sections`, each below the other, and every `text-at` of their
/// layers.
fn for_each_text<'a>(sections: &'a [SectionLayout], visit: &mut impl FnMut(&'a Node)) {
    for section in sections {
        for node in &section.children {
            for_each_node(node, visit);
        }
        for layer in section.underlays.iter().chain(&section.overlays) {
            for child in &layer.children {
                if let CanvasPrimitive::Placed(placed) = child {
                    visit(&placed.node);
                }
            }
        }
    }
}

fn for_each_text_mut(sections: &mut [SectionLayout], visit: &mut impl FnMut(&mut Node)) {
    for section in sections {
        for node in &mut section.children {
            for_each_node_mut(node, visit);
        }
        for layer in section.underlays.iter_mut().chain(&mut section.overlays) {
            for child in &mut layer.children {
                if let CanvasPrimitive::Placed(placed) = child {
                    visit(&mut placed.node);
                }
            }
        }
    }
}

fn for_each_node<'a>(node: &'a Node, visit: &mut impl FnMut(&'a Node)) {
    visit(node);
    for child in &node.children {
        for_each_node(child, visit);
    }
}

fn for_each_node_mut(node: &mut Node, visit: &mut impl FnMut(&mut Node)) {
    visit(node);
    for child in &mut node.children {
        for_each_node_mut(child, visit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::layout_document;
    use crate::parse::parse;
    use crate::render::{RenderLink, RenderNode};

    fn document(body: &str) -> String {
        format!(r#"<lpdf version="1"><document>{body}</document></lpdf>"#)
    }

    fn sections(body: &str) -> Vec<SectionLayout> {
        parse(&document(body)).unwrap().section_layouts()
    }

    fn render(body: &str) -> Vec<RenderPage> {
        layout_document(&sections(body))
    }

    /// The texts drawn on `page`, in the order they are drawn, with the links they are in.
    fn texts(page: &RenderPage) -> Vec<String> {
        fn walk(nodes: &[RenderNode], out: &mut Vec<String>) {
            for node in nodes {
                match node {
                    RenderNode::Text(t) => out.push(t.content.clone()),
                    RenderNode::Box(b) => walk(&b.children, out),
                    RenderNode::Link(l) => walk(&l.children, out),
                    RenderNode::CanvasLayer(l) => walk(&l.children, out),
                    _ => {}
                }
            }
        }
        let mut out = Vec::new();
        walk(&page.nodes, &mut out);
        out
    }

    /// The links on `page` whose text is `words`.
    fn links_of<'a>(page: &'a RenderPage, words: &str) -> Vec<&'a RenderLink> {
        fn walk<'a>(nodes: &'a [RenderNode], words: &str, out: &mut Vec<&'a RenderLink>) {
            for node in nodes {
                match node {
                    RenderNode::Link(l) if l.children.iter().any(|c| matches!(c, RenderNode::Text(t) if t.content == words)) => out.push(l),
                    RenderNode::Box(b) => walk(&b.children, words, out),
                    RenderNode::CanvasLayer(l) => walk(&l.children, words, out),
                    _ => {}
                }
            }
        }
        let mut out = Vec::new();
        walk(&page.nodes, words, &mut out);
        out
    }

    /// The index of the page the bookmark `id` is on.
    fn page_of(pages: &[RenderPage], id: &str) -> usize {
        placed_anchors(pages).iter().find(|p| p.anchor.id == id).map(|p| p.page).unwrap_or_else(|| panic!("no bookmark {id}"))
    }

    /// Clause 4 on the third page, after a page that refers to it.
    const CLAUSE_4: &str = r#"
        <text paginate="break-before">Two</text>
        <bookmark id="notice" label="clause 4"/>
        <text paginate="break-before">4. Notice.</text>"#;

    #[test]
    fn a_reference_shows_the_page_and_the_label_of_its_bookmark_and_links_to_it() {
        let pages = render(&format!(
            r#"<section><layout>
                   <text>Set in <span bookmark-ref="notice">{{bookmark-label}}, on page {{bookmark-page}}</span>.</text>
                   {CLAUSE_4}
               </layout></section>"#
        ));
        assert_eq!(texts(&pages[0]), ["Set in", "clause 4, on page 3", "."]);
        assert_eq!(links_of(&pages[0], "clause 4, on page 3")[0].url, "#notice");
    }

    #[test]
    fn a_reference_prints_the_page_in_the_format_and_prefix_its_footer_prints_it() {
        let pages = render(
            r#"<section><layout><text>See <span bookmark-ref="table">page {bookmark-page}</span>.</text></layout></section>
               <section page-start="1" page-format="lower-roman" page-prefix="A-"><layout>
                   <text>Appendix</text>
                   <bookmark id="table" label="Table 1"/>
                   <text paginate="break-before">Table 1</text>
               </layout></section>"#,
        );
        assert_eq!(texts(&pages[0]), ["See", "page A-ii", "."]);
    }

    #[test]
    fn a_reference_works_in_a_footer_a_text_at_and_a_footnote() {
        let pages = render(&format!(
            r#"<section><layout>
                   <text>Body<fn>As <span bookmark-ref="notice">page {{bookmark-page}}</span> says.</fn></text>
                   {CLAUSE_4}
                   <region pin="bottom"><text>Notice: <span bookmark-ref="notice">page {{bookmark-page}}</span></text></region>
               </layout>
               <layer><text-at x="10pt" y="10pt">See <span bookmark-ref="notice">{{bookmark-label}}</span></text-at></layer>
               </section>"#
        ));
        for page in &pages {
            let words = texts(page);
            assert!(words.contains(&"page 3".to_string()), "{words:?}");
            assert!(words.contains(&"clause 4".to_string()), "{words:?}");
        }
        assert_eq!(texts(&pages[0]).iter().filter(|w| *w == "page 3").count(), 2, "the footnote and the footer");
    }

    #[test]
    fn a_reference_that_moves_its_bookmark_keeps_room_for_its_widest_words_and_settles() {
        let xml = include_str!("../../../test/fixtures/showcase-reference-flip.xml");
        let sections = parse(xml).unwrap().section_layouts();
        // No page, then viii, which pushes clause 4 to ix, then ix in the room of viii.
        assert_eq!(passes(&sections), 3);
        let pages = layout_document(&sections);
        assert_eq!(page_of(&pages, "notice"), 8, "clause 4 is on page ix");
        let page_viii = texts(&pages[7]);
        assert!(page_viii.contains(&"on page ix".to_string()), "{page_viii:?}");
        // "on page viii" is 12 characters of Courier at 10pt.
        assert_eq!(links_of(&pages[7], "on page ix")[0].width, 72.0);
    }

    #[test]
    fn a_reference_settles_when_its_number_moves_the_pages_of_the_sections_after_it() {
        // Sixty references, each one line, to a bookmark that they push onto a later page: its number grows from
        // nothing on the first pass to two digits, and the references show the page it lands on.
        let lines: String = (0..60)
            .map(|_| r#"<text>See <span bookmark-ref="end">page {bookmark-page}</span> for the end.</text>"#)
            .collect();
        let pages = render(&format!(
            r#"<section><layout>{lines}<bookmark id="end" label="End"/><text>The end.</text></layout></section>"#
        ));
        let end = page_of(&pages, "end");
        assert!(texts(&pages[0]).contains(&format!("page {}", end + 1)));
    }

    /// How many times `sections` are laid out, the whole document each time.
    fn passes(sections: &[SectionLayout]) -> usize {
        layout::DOCUMENT_PASSES.with(|passes| passes.set(0));
        layout_document(sections);
        layout::DOCUMENT_PASSES.with(|passes| passes.get())
    }

    #[test]
    fn a_document_without_references_is_laid_out_once() {
        let without = sections(r##"<section><layout><bookmark id="a" label="A"/><text>See <span href="#a">A</span></text></layout></section>"##);
        assert_eq!(passes(&without), 1);
        // The first pass finds the page, the second shows it.
        assert_eq!(passes(&sections_with_reference()), 2);
    }

    fn sections_with_reference() -> Vec<SectionLayout> {
        sections(r#"<section><layout><bookmark id="a" label="A"/><text><span bookmark-ref="a">{bookmark-page}</span></text></layout></section>"#)
    }

    #[test]
    fn a_reference_to_no_bookmark_fails_with_its_line() {
        let sections = sections(r#"<section><layout>
<text>See <span bookmark-ref="nowhere">page {bookmark-page}</span></text></layout></section>"#);
        let err = check(&sections).unwrap_err();
        assert_eq!(err, "<span> at line 2, col 11: bookmark-ref 'nowhere' names no bookmark");
        assert!(check(&sections_with_reference()).is_ok());
    }

    #[test]
    fn another_token_in_a_reference_fails_with_its_line() {
        let err = parse(&document(r#"<section><layout>
<text>See <span bookmark-ref="a">page {page}</span></text></layout></section>"#)).unwrap_err();
        assert_eq!(err, "<span> at line 2, col 11: unknown token '{page}' in a span with bookmark-ref — expected {bookmark-page} or {bookmark-label}");
    }

    #[test]
    fn a_reference_takes_no_href() {
        let err = parse(&document(r##"<section><layout><text><span bookmark-ref="a" href="#a">x</span></text></layout></section>"##)).unwrap_err();
        assert!(err.contains("leave out href"), "{err}");
    }

    #[test]
    fn a_reference_in_code_keeps_room_after_its_last_character_and_settles() {
        // The flip of `showcase-reference-flip`, with the sentence in a code: there it wraps at the space before
        // "viii.", and "ix." in the room of "viii." keeps it wrapped.
        let flip = include_str!("../../../test/fixtures/showcase-reference-flip.xml").replace(
            r#"<text width="240pt" font-size="10" line-height="1.5" paginate="break-before">The notice period is set out <span bookmark-ref="notice">on page {bookmark-page}</span>.</text>"#,
            r#"<code width="240pt" padding="0" font-size="10" line-height="1.5" paginate="break-before">The notice period is set out <span bookmark-ref="notice">on page {bookmark-page}</span>.</code>"#,
        );
        assert!(flip.contains("<code "), "the fixture's sentence moved");
        let sections = parse(&flip).unwrap().section_layouts();
        assert_eq!(passes(&sections), 3);
        let pages = layout_document(&sections);
        assert_eq!(page_of(&pages, "notice"), 8, "clause 4 is on page ix");
        assert_eq!(texts(&pages[7])[..4], ["The notice period is set out ", "on page", "ix", "."]);
        // "ix" and the room of the two characters "viii" has more, in Courier at 10pt.
        let width = links_of(&pages[7], "ix")[0].width;
        assert!((width - 24.0).abs() < 0.01, "{width}");
    }

    #[test]
    fn a_reference_in_code_split_across_pages_keeps_its_room_on_the_next() {
        // A hundred lines of code, which run onto a second page, with a reference on the last line that has shown
        // wider words before.
        let lines: String = (1..100).map(|n| format!("line {n}\n")).collect();
        let mut sections = sections(&format!(
            r#"<section><layout>
                   <code>{lines}see <span bookmark-ref="end">page {{bookmark-page}}</span>
next</code>
                   <bookmark id="end" label="The end"/>
                   <text paginate="break-before">The end.</text>
               </layout></section>"#
        ));
        for_each_text_mut(&mut sections, &mut |node| {
            for run in &mut node.text_runs {
                if let Some(reference) = run.reference.as_deref_mut() {
                    reference.widest = "page 9999".to_string();
                }
            }
        });
        let pages = layout_document(&sections);
        assert_eq!(page_of(&pages, "end"), 2);
        let link = links_of(&pages[1], "page 3")[0];
        assert_eq!(link.width, text_width("Courier", "page 9999", 11.0), "the room of the widest words");
        assert!(texts(&pages[1]).contains(&"next".to_string()), "the line after the reference");
    }

    #[test]
    fn a_reference_in_a_kit_tree_is_read_as_in_xml() {
        let tree = r#"{"version":1,"type":"document","attrs":{},"nodes":[{"type":"section","attrs":{},"nodes":[{"type":"layout","attrs":{},"nodes":[
            {"type":"text","attrs":{},"nodes":["See ",{"type":"span","attrs":{"bookmark-ref":"a"},"nodes":["page {bookmark-page}"]}]},
            {"type":"bookmark","attrs":{"id":"a","label":"A"}},
            {"type":"text","attrs":{"paginate":"break-before"},"nodes":["A"]}]}]}]}"#;
        let pages = layout_document(&crate::parse::parse_tree(tree).unwrap().section_layouts());
        assert_eq!(texts(&pages[0]), ["See", "page 2"]);

        let tree = tree.replace(r#""bookmark-ref":"a""#, r#""bookmark-ref":"b""#);
        let err = check(&crate::parse::parse_tree(&tree).unwrap().section_layouts()).unwrap_err();
        assert_eq!(err, "span: bookmark-ref 'b' names no bookmark");
    }

    #[test]
    fn a_token_is_filled_once() {
        assert_eq!(fill("{bookmark-label} on {bookmark-page}, {x}", "iv", "{bookmark-page}"), "{bookmark-page} on iv, {x}");
    }
}
