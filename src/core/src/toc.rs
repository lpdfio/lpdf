// The table of contents: a `<section type="toc">` whose content is bound to `$toc`, the list of the document's
// bookmarks with the page each one is on.
//
// The list needs the pages of the other sections, and the TOC's own length moves the pages after it. So the
// TOC is laid out last: the other sections first, then the TOC with page numbers that assume its length, again
// until that length holds. A longer TOC only makes page numbers bigger, which can only make it longer, so the
// length settles; a TOC of one page, the usual case, takes one pass.
//
// Each page number is the one its footer prints (`page_numbers`), which the TOC's length moves too. Where a
// bookmark lands doesn't depend on the numbers, so the other sections are laid out once, numbered as if the TOC
// is one page long, and laid out again only when its length moved the numbers one of them shows.

use serde_json::{json, Map, Value};

use crate::data;
use crate::layout::{fill_section, finish_section, layout_numbered, shows_page_numbers, FilledSection};
use crate::page_numbers::{self, SectionNumbers};
use crate::parse::{SectionLayout, TocOptions, TOC_DATA_KEY};
use crate::render::{placed_anchors, RenderPage};

/// More passes than a TOC needs: a pass that does not settle adds a page or more to the TOC.
const MAX_PASSES: usize = 4;

/// Lay out `sections`, of which the one at `toc_index` is the table of contents.
pub(crate) fn layout_with_toc(sections: &[SectionLayout], toc_index: usize, options: &TocOptions) -> Vec<RenderPage> {
    let filled: Vec<Option<FilledSection>> = sections.iter().enumerate()
        .map(|(i, section)| (i != toc_index).then(|| fill_section(section)))
        .collect();
    // A section lays out to one page at least, so one is the first guess.
    let mut counts: Vec<usize> = filled.iter().map(|f| f.as_ref().map_or(1, FilledSection::page_count)).collect();
    let guessed = page_numbers::number_sections(sections, &counts);
    let mut laid: Vec<Vec<RenderPage>> = sections.iter().zip(filled).zip(&guessed)
        .map(|((section, filled), numbers)| filled.map_or_else(Vec::new, |f| finish_section(section, f, numbers)))
        .collect();

    let (mut toc, mut toc_filled) = toc_pass(sections, toc_index, options, &laid, &counts);
    for _ in 1..MAX_PASSES {
        if toc_filled.page_count() == counts[toc_index] {
            break;
        }
        counts[toc_index] = toc_filled.page_count();
        (toc, toc_filled) = toc_pass(sections, toc_index, options, &laid, &counts);
    }
    counts[toc_index] = toc_filled.page_count();

    let numbers = page_numbers::number_sections(sections, &counts);
    laid[toc_index] = finish_section(&toc, toc_filled, &numbers[toc_index]);
    for (i, section) in sections.iter().enumerate() {
        if i != toc_index && numbers[i] != guessed[i] && shows_page_numbers(section) {
            laid[i] = layout_numbered(section, &numbers[i]);
        }
    }
    let mut pages: Vec<RenderPage> = laid.into_iter().flatten().collect();
    page_numbers::mark_label_ranges(&mut pages, sections, &numbers, &counts);
    pages
}

/// The TOC section bound to the entries of the other sections' pages `laid`, numbered as if the sections have
/// `counts` pages, and filled.
fn toc_pass(
    sections:  &[SectionLayout],
    toc_index: usize,
    options:   &TocOptions,
    laid:      &[Vec<RenderPage>],
    counts:    &[usize],
) -> (SectionLayout, FilledSection) {
    let numbers = page_numbers::number_sections(sections, counts);
    let entries = toc_entries(laid, toc_index, &numbers, options.indent_step);
    let toc = bound_section(&sections[toc_index], options, entries);
    let filled = fill_section(&toc);
    (toc, filled)
}

/// `$toc`: the bookmarks of the sections' pages `laid` but the TOC's, each with the number its page shows.
fn toc_entries(laid: &[Vec<RenderPage>], toc_index: usize, numbers: &[SectionNumbers], indent_step: f32) -> Value {
    let placed = laid.iter().zip(numbers).enumerate()
        .filter(|&(i, _)| i != toc_index)
        .flat_map(|(_, (pages, numbers))| placed_anchors(pages).into_iter().map(move |p| (numbers.page(p.page), p.anchor)));
    let entries = placed
        .enumerate()
        .map(|(i, (page, anchor))| json!({
            "index":  i + 1,
            "level":  anchor.level,
            "label":  anchor.label,
            "page":   page,
            "anchor": format!("#{}", anchor.id),
            "indent": format!("{}pt", f32::from(anchor.level - 1) * indent_step),
        }))
        .collect();
    Value::Array(entries)
}

/// The TOC section with its content bound to the caller's data and `$toc`.
fn bound_section(section: &SectionLayout, options: &TocOptions, entries: Value) -> SectionLayout {
    let mut root = match &options.data {
        Some(Value::Object(fields)) => fields.clone(),
        _                           => Map::new(),
    };
    root.insert(TOC_DATA_KEY.to_string(), entries);

    let mut bound = section.clone();
    bound.children = data::apply_toc(std::mem::take(&mut bound.children), &Value::Object(root), options.indent_step);
    crate::lists::lower_all(&mut bound.children);
    bound
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{layout_document, layout_page};
    use crate::parse::parse;
    use crate::render::RenderNode;

    fn document(sections: &str) -> String {
        format!(r#"<lpdf version="1"><document>{sections}</document></lpdf>"#)
    }

    fn render(sections: &str) -> Vec<RenderPage> {
        layout_document(&parse(&document(sections)).unwrap().section_layouts())
    }

    /// The lines of text on `pages`, in the order they are drawn.
    fn texts(pages: &[RenderPage]) -> Vec<String> {
        fn walk(nodes: &[RenderNode], out: &mut Vec<String>) {
            for node in nodes {
                match node {
                    RenderNode::Text(t) => out.push(t.content.clone()),
                    RenderNode::Box(b)  => walk(&b.children, out),
                    RenderNode::Link(l) => walk(&l.children, out),
                    _ => {}
                }
            }
        }
        let mut out = Vec::new();
        for page in pages {
            walk(&page.nodes, &mut out);
        }
        out
    }

    /// A table of contents that lists each entry as its label and its page number.
    const TOC: &str = r#"<section type="toc"><layout>
        <column data-source="$toc"><text data-value="label">Label</text><text data-value="page">0</text></column>
    </layout></section>"#;

    #[test]
    fn entries_have_index_level_label_page_anchor_and_indent() {
        let sections = parse(&document(
            r#"<section type="toc"><layout><text>Contents</text></layout></section>
               <section><layout>
                   <bookmark label="Introduction"/><text>Introduction</text>
                   <bookmark label="Background" level="2"/><text>Background</text>
               </layout></section>
               <section><layout><bookmark label="Methods"/><text>Methods</text></layout></section>"#,
        )).unwrap().section_layouts();
        let laid: Vec<Vec<RenderPage>> = sections.iter()
            .map(|section| if section.toc.is_some() { Vec::new() } else { layout_page(section) })
            .collect();
        let numbers = page_numbers::number_sections(&sections, &[1, 1, 1]);

        assert_eq!(toc_entries(&laid, 0, &numbers, 14.0), json!([
            { "index": 1, "level": 1, "label": "Introduction", "page": "2", "anchor": "#introduction", "indent": "0pt" },
            { "index": 2, "level": 2, "label": "Background",   "page": "2", "anchor": "#background",   "indent": "14pt" },
            { "index": 3, "level": 1, "label": "Methods",      "page": "3", "anchor": "#methods",      "indent": "0pt" },
        ]));
    }

    #[test]
    fn page_numbers_count_the_pages_before_the_toc_and_its_own() {
        let pages = render(&format!(
            r#"<section><layout><bookmark label="Alpha"/><text>Alpha</text></layout></section>
               {TOC}
               <section><layout><bookmark label="Beta"/><text>Beta</text></layout></section>"#
        ));
        assert_eq!(pages.len(), 3);
        assert_eq!(texts(&pages[1..2]), ["Alpha", "1", "Beta", "3"]);
    }

    #[test]
    fn toc_longer_than_a_page_numbers_the_pages_after_its_own_length() {
        // Seventy parts on a page each list on two TOC pages, which the first pass, assuming one, gets wrong
        // by a page; their numbers also cross from one digit to two.
        let parts: String = (1..=70)
            .map(|n| format!(r#"<bookmark label="Part {n}"/><text paginate="break-before">Part {n}</text>"#))
            .collect();
        let pages = render(&format!(
            r#"<section type="toc"><layout>
                   <column data-source="$toc"><row><text data-value="label">Label</text><text width="fill" data-value="page">0</text></row></column>
               </layout></section>
               <section><layout>{parts}</layout></section>"#
        ));
        let toc_len = pages.len() - 70;
        assert_eq!(toc_len, 2);

        let anchors = placed_anchors(&pages);
        let lines = texts(&pages[..toc_len]);
        assert_eq!(lines.len(), 140);
        for (entry, pair) in lines.chunks(2).enumerate() {
            let placed = &anchors[entry];
            assert_eq!(pair[0], placed.anchor.label);
            assert_eq!(pair[1], (placed.page + 1).to_string(), "{}", placed.anchor.label);
        }
    }

    #[test]
    fn link_to_hash_in_an_entry_points_at_its_bookmark() {
        let pages = render(
            r##"<section type="toc"><layout>
                   <column data-source="$toc">
                       <link href="#"><text data-value="label">Label</text></link>
                       <text>See <span href="#">there</span></text>
                   </column>
               </layout></section>
               <section><layout>
                   <bookmark label="Alpha"/><text>Alpha</text>
                   <bookmark label="Beta" id="second"/><text>Beta</text>
               </layout></section>"##,
        );
        fn urls(nodes: &[RenderNode], out: &mut Vec<String>) {
            for node in nodes {
                match node {
                    RenderNode::Link(l) => { out.push(l.url.clone()); urls(&l.children, out); }
                    RenderNode::Box(b)  => urls(&b.children, out),
                    _ => {}
                }
            }
        }
        let mut found = Vec::new();
        urls(&pages[0].nodes, &mut found);
        assert_eq!(found, ["#alpha", "#alpha", "#second", "#second"]);
    }

    #[test]
    fn entries_are_indented_by_level() {
        let pages = render(
            r#"<section type="toc" toc-indent-step="20pt"><layout>
                   <column data-source="$toc"><text data-value="label">Label</text></column>
               </layout></section>
               <section><layout>
                   <bookmark label="Top"/><text>Top</text>
                   <bookmark label="Third" level="3"/><text>Third</text>
               </layout></section>"#,
        );
        let x_of = |label: &str| -> f32 {
            fn find(nodes: &[RenderNode], label: &str) -> Option<f32> {
                nodes.iter().find_map(|node| match node {
                    RenderNode::Text(t) if t.content == label => Some(t.x),
                    RenderNode::Box(b) => find(&b.children, label),
                    _ => None,
                })
            }
            find(&pages[0].nodes, label).unwrap_or_else(|| panic!("no text {label}"))
        };
        assert_eq!(x_of("Third") - x_of("Top"), 40.0);
    }

    #[test]
    fn bookmarks_of_the_toc_section_are_in_the_outline_and_not_in_the_list() {
        let pages = render(
            r#"<section type="toc"><bookmark label="Contents"/><layout>
                   <bookmark label="Listed nowhere"/><text>Contents</text>
                   <column data-source="$toc"><text data-value="label">Label</text></column>
               </layout></section>
               <section><layout><bookmark label="Alpha"/><text>Alpha</text></layout></section>"#,
        );
        assert_eq!(texts(&pages[..1]), ["Contents", "Alpha"]);
        let ids: Vec<&str> = placed_anchors(&pages).iter().map(|p| p.anchor.id.as_str()).collect();
        assert_eq!(ids, ["contents", "listed-nowhere", "alpha"]);
    }

    #[test]
    fn toc_binds_the_callers_data_beside_toc() {
        let mut doc = parse(&document(
            r#"<section type="toc"><layout>
                   <text data-value="title">Title</text>
                   <column data-source="$toc"><text data-value="label">Label</text></column>
               </layout></section>
               <section><layout><bookmark label="Alpha"/><text data-value="body">Body</text></layout></section>"#,
        )).unwrap();
        data::apply(&mut doc, r#"{"title": "Report", "body": "Text", "$toc": ["ignored"]}"#).unwrap();
        let pages = layout_document(&doc.section_layouts());
        assert_eq!(texts(&pages), ["Report", "Alpha", "Text"]);
    }
}
