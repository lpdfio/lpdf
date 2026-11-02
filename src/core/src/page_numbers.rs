// Page numbers across sections: the number each section's pages start from, in which format and after which
// prefix, what `{page}` and `{pages}` print, and the ranges of the viewer's page labels (`/PageLabels`).
//
// Pages count on through the document, from 1, in decimal. A section changes that only when it says so, with
// `page-start`, `page-format` or `page-prefix`, and one that says nothing carries on the count, the format and the
// prefix of the one before. A run is the pages from one `page-start` (or the start of the document) to the next,
// and `{pages}` counts it.

use crate::parse::{ListFormat, SectionLayout};
use crate::render::{PageLabel, RenderPage};

/// The highest number a page shows in letters or roman numerals: past it, as a list's numerals do, a page is
/// numbered in decimal, so that its label stays short. Letters as a PDF viewer counts them repeat one letter, so
/// page 3999 is 154 letters long.
const MAX_LETTERED: u64 = 3999;

/// What a section says about its page numbers: `page-start`, `page-format` and `page-prefix`, each `None` when
/// it doesn't set it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PageNumbering {
    pub start:  Option<u32>,
    pub format: Option<ListFormat>,
    pub prefix: Option<String>,
}

impl PageNumbering {
    /// Whether the section sets any of the three, which starts a range of the viewer's page labels.
    pub fn is_set(&self) -> bool {
        self.start.is_some() || self.format.is_some() || self.prefix.is_some()
    }
}

/// What `{page}` and `{pages}` print on one page: the page at `index` of a section with `numbers`. A text with
/// neither token, the usual one, never writes them.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PageTokens<'a> {
    numbers: &'a SectionNumbers,
    index:   usize,
}

/// The numbers of a lone page numbered 1.
static FIRST_PAGE: SectionNumbers = SectionNumbers { first: 1, format: ListFormat::Decimal, prefix: String::new(), run_pages: 1 };

impl<'a> PageTokens<'a> {
    /// For what is drawn outside the pages' numbering, such as a shape in the flow, which shows no text.
    pub(crate) const FIRST: PageTokens<'static> = PageTokens { numbers: &FIRST_PAGE, index: 0 };

    pub(crate) fn new(numbers: &'a SectionNumbers, index: usize) -> Self {
        Self { numbers, index }
    }

    pub(crate) fn page(&self) -> String {
        self.numbers.page(self.index)
    }

    pub(crate) fn pages(&self) -> String {
        self.numbers.pages()
    }
}

/// The numbers of one section's pages.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SectionNumbers {
    /// The number of its first page.
    pub first:     u64,
    pub format:    ListFormat,
    pub prefix:    String,
    /// How many pages its run has.
    pub run_pages: usize,
}

impl SectionNumbers {
    /// The numbers of a section laid out on its own: its pages are the run.
    #[cfg(test)]
    pub(crate) fn alone(numbering: &PageNumbering, pages: usize) -> Self {
        Self {
            first:     numbering.start.map_or(1, u64::from),
            format:    numbering.format.unwrap_or_default(),
            prefix:    numbering.prefix.clone().unwrap_or_default(),
            run_pages: pages,
        }
    }

    /// What `{page}` prints on the section's page at `index`, from 0: its number in the format, after the prefix.
    pub(crate) fn page(&self, index: usize) -> String {
        format!("{}{}", self.prefix, format_page_number(self.format, self.first + index as u64))
    }

    /// What `{pages}` prints: how many pages the run has, in the format and without the prefix, since it is a
    /// count and not a page's label.
    pub(crate) fn pages(&self) -> String {
        format_page_number(self.format, self.run_pages as u64)
    }
}

/// The numbers of each section of `sections`, which has `counts[i]` pages.
pub(crate) fn number_sections(sections: &[SectionLayout], counts: &[usize]) -> Vec<SectionNumbers> {
    let mut out: Vec<SectionNumbers> = Vec::with_capacity(sections.len());
    let mut next = 1u64;
    let mut format = ListFormat::Decimal;
    let mut prefix = String::new();
    let mut run_start = 0;
    for (i, (section, &count)) in sections.iter().zip(counts).enumerate() {
        let numbering = &section.page_numbering;
        if let Some(start) = numbering.start {
            close_run(&mut out[run_start..], &counts[run_start..i]);
            run_start = i;
            next = u64::from(start);
        }
        if let Some(f) = numbering.format {
            format = f;
        }
        if let Some(p) = &numbering.prefix {
            prefix = p.clone();
        }
        out.push(SectionNumbers { first: next, format, prefix: prefix.clone(), run_pages: 0 });
        next += count as u64;
    }
    close_run(&mut out[run_start..], &counts[run_start..]);
    out
}

/// What `{page}` prints on each page of sections with `numbers`, which have `counts[i]` pages.
pub(crate) fn page_labels(numbers: &[SectionNumbers], counts: &[usize]) -> Vec<String> {
    numbers.iter().zip(counts).flat_map(|(numbers, &count)| (0..count).map(|i| numbers.page(i))).collect()
}

/// Give the sections of a run the number of pages it has.
fn close_run(run: &mut [SectionNumbers], counts: &[usize]) {
    let pages = counts.iter().sum();
    for numbers in run {
        numbers.run_pages = pages;
    }
}

/// Mark on `pages` where each range of the viewer's page labels starts: the first page, and the first page of
/// each section that sets its numbering, with the style, the prefix and the first number. A section in letters or
/// numerals whose numbers pass `MAX_LETTERED` gets one more, in decimal, where they do. No section sets any: no
/// labels, since a viewer counts 1, 2, 3 without them.
pub(crate) fn mark_label_ranges(pages: &mut [RenderPage], sections: &[SectionLayout], numbers: &[SectionNumbers], counts: &[usize]) {
    if !sections.iter().any(|s| s.page_numbering.is_set()) {
        return;
    }
    let mut first_page = 0;
    for (i, ((section, numbers), &count)) in sections.iter().zip(numbers).zip(counts).enumerate() {
        let label = |format, start| Some(PageLabel { format, prefix: numbers.prefix.clone(), start });
        let lettered = numbers.format != ListFormat::Decimal;
        if i == 0 || section.page_numbering.is_set() {
            let format = if lettered && numbers.first > MAX_LETTERED { ListFormat::Decimal } else { numbers.format };
            pages[first_page].label = label(format, numbers.first);
        }
        let last = numbers.first + count as u64 - 1;
        if lettered && numbers.first <= MAX_LETTERED && last > MAX_LETTERED {
            let index = first_page + (MAX_LETTERED + 1 - numbers.first) as usize;
            pages[index].label = label(ListFormat::Decimal, MAX_LETTERED + 1);
        }
        first_page += count;
    }
}

/// `number`, 1 or more, as a page shows it in `format`. Letters count as a PDF viewer counts page labels (ISO
/// 32000-1 §12.4.2): a to z, then aa to zz, then aaa, where a list's letters go aa, ab. Numerals are a list's.
/// Past `MAX_LETTERED`, a number is written in decimal.
pub(crate) fn format_page_number(format: ListFormat, number: u64) -> String {
    match format {
        ListFormat::LowerAlpha if number <= MAX_LETTERED => letters(number),
        ListFormat::UpperAlpha if number <= MAX_LETTERED => letters(number).to_uppercase(),
        ListFormat::LowerRoman if number <= MAX_LETTERED => crate::lists::roman(number as i64),
        ListFormat::UpperRoman if number <= MAX_LETTERED => crate::lists::roman(number as i64).to_uppercase(),
        _ => number.to_string(),
    }
}

/// 1 is a, 26 is z, 27 is aa, 28 is bb, 53 is aaa.
fn letters(number: u64) -> String {
    let letter = char::from(b'a' + ((number - 1) % 26) as u8);
    std::iter::repeat_n(letter, ((number - 1) / 26 + 1) as usize).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(start: Option<u32>, format: Option<ListFormat>, prefix: Option<&str>) -> SectionLayout {
        let mut section = crate::parse::parse(r#"<lpdf version="1"><document><section><layout/></section></document></lpdf>"#)
            .unwrap()
            .section_layouts()
            .remove(0);
        section.page_numbering = PageNumbering { start, format, prefix: prefix.map(str::to_string) };
        section
    }

    /// The labels of every page of sections with `counts` pages each.
    fn labels(sections: &[SectionLayout], counts: &[usize]) -> Vec<String> {
        page_labels(&number_sections(sections, counts), counts)
    }

    #[test]
    fn page_letters_repeat_one_letter_past_z_as_a_viewer_counts_them() {
        let lower: Vec<String> = [1, 2, 26, 27, 28, 52, 53, 79].iter().map(|&n| format_page_number(ListFormat::LowerAlpha, n)).collect();
        assert_eq!(lower, ["a", "b", "z", "aa", "bb", "zz", "aaa", "aaaa"]);
        assert_eq!(format_page_number(ListFormat::UpperAlpha, 28), "BB");
    }

    #[test]
    fn page_numerals_are_a_lists_numerals() {
        assert_eq!(format_page_number(ListFormat::LowerRoman, 4), "iv");
        assert_eq!(format_page_number(ListFormat::UpperRoman, 1994), "MCMXCIV");
        assert_eq!(format_page_number(ListFormat::Decimal, 19), "19");
    }

    #[test]
    fn letters_and_numerals_past_3999_are_written_in_decimal() {
        assert_eq!(format_page_number(ListFormat::LowerRoman, 3999), "mmmcmxcix");
        assert_eq!(format_page_number(ListFormat::LowerRoman, 4000), "4000");
        assert_eq!(format_page_number(ListFormat::LowerAlpha, 3999).len(), 154);
        assert_eq!(format_page_number(ListFormat::UpperAlpha, 4000), "4000");
    }

    #[test]
    fn pages_count_on_through_sections_that_set_nothing() {
        let sections = [section(None, None, None), section(None, None, None)];
        assert_eq!(labels(&sections, &[2, 3]), ["1", "2", "3", "4", "5"]);
        let numbers = number_sections(&sections, &[2, 3]);
        assert_eq!((numbers[0].pages(), numbers[1].pages()), ("5".to_string(), "5".to_string()));
    }

    #[test]
    fn a_book_numbers_its_front_matter_in_numerals_and_restarts_for_chapters_and_appendix() {
        let sections = [
            section(None, Some(ListFormat::LowerRoman), None),
            section(None, None, None),
            section(Some(1), Some(ListFormat::Decimal), None),
            section(None, None, None),
            section(Some(1), None, Some("A-")),
        ];
        assert_eq!(
            labels(&sections, &[2, 2, 3, 2, 2]),
            ["i", "ii", "iii", "iv", "1", "2", "3", "4", "5", "A-1", "A-2"],
        );
        let pages: Vec<String> = number_sections(&sections, &[2, 2, 3, 2, 2]).iter().map(SectionNumbers::pages).collect();
        assert_eq!(pages, ["iv", "iv", "5", "5", "2"]);
    }

    #[test]
    fn a_format_alone_carries_the_count_on() {
        let sections = [section(None, Some(ListFormat::LowerRoman), None), section(None, Some(ListFormat::Decimal), None)];
        assert_eq!(labels(&sections, &[2, 2]), ["i", "ii", "3", "4"]);
    }

    #[test]
    fn an_empty_prefix_takes_the_one_before_away() {
        let sections = [section(None, None, Some("A-")), section(Some(1), None, Some(""))];
        assert_eq!(labels(&sections, &[1, 1]), ["A-1", "1"]);
    }

    #[test]
    fn a_page_start_above_one_continues_a_count_from_elsewhere() {
        let sections = [section(Some(321), None, None)];
        assert_eq!(labels(&sections, &[2]), ["321", "322"]);
        assert_eq!(number_sections(&sections, &[2])[0].pages(), "2");
    }

    fn ranges(sections: &[SectionLayout], counts: &[usize]) -> Vec<(usize, ListFormat, String, u64)> {
        let numbers = number_sections(sections, counts);
        let mut pages: Vec<RenderPage> = (0..counts.iter().sum()).map(|_| RenderPage::blank()).collect();
        mark_label_ranges(&mut pages, sections, &numbers, counts);
        pages.iter().enumerate()
            .filter_map(|(i, p)| p.label.as_ref().map(|l| (i, l.format, l.prefix.clone(), l.start)))
            .collect()
    }

    #[test]
    fn no_section_that_sets_its_numbering_gives_no_label_ranges() {
        assert_eq!(ranges(&[section(None, None, None), section(None, None, None)], &[2, 2]), []);
    }

    #[test]
    fn label_ranges_start_on_the_first_page_and_at_each_section_that_sets_its_numbering() {
        let sections = [
            section(None, None, None),
            section(None, Some(ListFormat::LowerRoman), None),
            section(None, None, None),
            section(Some(1), Some(ListFormat::Decimal), Some("A-")),
        ];
        assert_eq!(ranges(&sections, &[1, 2, 2, 3]), [
            (0, ListFormat::Decimal, String::new(), 1),
            (1, ListFormat::LowerRoman, String::new(), 2),
            (5, ListFormat::Decimal, "A-".to_string(), 1),
        ]);
    }

    #[test]
    fn numerals_that_pass_3999_get_a_decimal_range_where_they_do() {
        let sections = [section(Some(3998), Some(ListFormat::UpperRoman), None), section(None, None, None)];
        assert_eq!(labels(&sections, &[3, 1]), ["MMMCMXCVIII", "MMMCMXCIX", "4000", "4001"]);
        assert_eq!(ranges(&sections, &[3, 1]), [
            (0, ListFormat::UpperRoman, String::new(), 3998),
            (2, ListFormat::Decimal, String::new(), 4000),
        ]);
    }
}

/// The numbers as the pages print them, in a footer, a layer and a table of contents, and as the PDF labels them.
#[cfg(test)]
mod rendered {
    use crate::layout::layout_document;
    use crate::parse::{parse, parse_tree};
    use crate::render::{RenderNode, RenderPage};

    fn render(sections: &str) -> Vec<RenderPage> {
        layout_document(&parse(&format!(r#"<lpdf version="1"><document>{sections}</document></lpdf>"#)).unwrap().section_layouts())
    }

    /// The texts of `page`, in the order they are drawn.
    fn texts(page: &RenderPage) -> Vec<String> {
        fn walk(nodes: &[RenderNode], out: &mut Vec<String>) {
            for node in nodes {
                match node {
                    RenderNode::Text(t)        => out.push(t.content.clone()),
                    RenderNode::Box(b)         => walk(&b.children, out),
                    RenderNode::Link(l)        => walk(&l.children, out),
                    RenderNode::CanvasLayer(l) => walk(&l.children, out),
                    _ => {}
                }
            }
        }
        let mut out = Vec::new();
        walk(&page.nodes, &mut out);
        out
    }

    /// What each page's footer, `p. {page}/{pages}`, prints.
    fn footers(pages: &[RenderPage]) -> Vec<String> {
        pages.iter()
            .map(|page| texts(page).into_iter().find_map(|t| t.strip_prefix("p. ").map(str::to_string)).unwrap_or_default())
            .collect()
    }

    /// A section with `attrs` whose flow is `pages` pages, each starting with a bookmark labelled `name` and the page,
    /// under the footer `p. {page}/{pages}`.
    fn section(attrs: &str, name: &str, pages: usize) -> String {
        let flow: String = (1..=pages)
            .map(|n| format!(r#"<bookmark label="{name} {n}"/><text paginate="break-before">{name} {n}</text>"#))
            .collect();
        format!(r#"<section {attrs}><layout>{flow}<region pin="bottom"><text>p. {{page}}/{{pages}}</text></region></layout></section>"#)
    }

    /// The labels the PDF gives the pages: where each range starts, its style, prefix and first number.
    fn ranges(pages: &[RenderPage]) -> Vec<(usize, String)> {
        pages.iter().enumerate()
            .filter_map(|(i, p)| p.label.as_ref().map(|l| (i, format!("{:?} {:?} {}", l.format, l.prefix, l.start))))
            .collect()
    }

    const TOC: &str = r#"<section type="toc"><layout>
        <column data-source="$toc"><text data-value="label">Label</text><text data-value="page">0</text></column>
        <region pin="bottom"><text>p. {page}/{pages}</text></region>
    </layout></section>"#;

    #[test]
    fn a_book_shows_the_footers_number_in_its_contents_and_its_labels() {
        let pages = render(&[
            section(r#"page-format="lower-roman""#, "Cover", 2),
            TOC.to_string(),
            section("", "Preface", 1),
            section(r#"page-start="1" page-format="decimal""#, "Chapter", 3),
            section(r#"page-start="1" page-prefix="A-""#, "Appendix", 2),
        ].concat());
        assert_eq!(footers(&pages), ["i/iv", "ii/iv", "iii/iv", "iv/iv", "1/3", "2/3", "3/3", "A-1/2", "A-2/2"]);
        let contents = texts(&pages[2]);
        let entry = |label: &str| contents.iter().position(|t| t == label).map(|i| contents[i + 1].as_str());
        assert_eq!(entry("Cover 2"), Some("ii"));
        assert_eq!(entry("Preface 1"), Some("iv"));
        assert_eq!(entry("Chapter 1"), Some("1"));
        assert_eq!(entry("Appendix 2"), Some("A-2"));
        assert_eq!(ranges(&pages), [
            (0, r#"LowerRoman "" 1"#.to_string()),
            (4, r#"Decimal "" 1"#.to_string()),
            (7, r#"Decimal "A-" 1"#.to_string()),
        ]);
    }

    #[test]
    fn pages_count_the_run_up_to_the_next_page_start() {
        let invoice = section(r#"page-start="1""#, "Invoice", 2);
        let pages = render(&[invoice.clone(), invoice.clone(), invoice].concat());
        assert_eq!(footers(&pages), ["1/2", "2/2", "1/2", "2/2", "1/2", "2/2"]);
    }

    #[test]
    fn sections_that_set_nothing_count_on_and_get_no_labels() {
        let pages = render(&[section("", "Part", 2), section("", "Part", 1)].concat());
        assert_eq!(footers(&pages), ["1/3", "2/3", "3/3"]);
        assert_eq!(ranges(&pages), []);
    }

    #[test]
    fn page_letters_past_z_repeat_a_letter_in_the_footer_the_contents_and_the_labels() {
        let pages = render(&[TOC.replace("<section ", r#"<section page-format="lower-alpha" "#), section("", "Page", 29)].concat());
        // The contents list each page on two lines, so they take two pages.
        let footers = footers(&pages);
        assert_eq!(footers.len(), 31);
        assert_eq!(texts(&pages[2]).first().map(String::as_str), Some("Page 1"));
        assert_eq!(footers[25..29], ["z/ee", "aa/ee", "bb/ee", "cc/ee"]);
        let contents: Vec<String> = pages[..2].iter().flat_map(texts).collect();
        let entry = |label: &str| contents.iter().position(|t| t == label).map(|i| contents[i + 1].as_str());
        assert_eq!(entry("Page 26"), Some("bb"));
        assert_eq!(entry("Page 27"), Some("cc"));
        assert_eq!(ranges(&pages), [(0, r#"LowerAlpha "" 1"#.to_string())]);
    }

    #[test]
    fn a_layer_prints_the_numbers_of_the_page_it_is_drawn_on() {
        let layer = r#"<layer><text-at x="40pt" y="800pt">p. {page}/{pages}</text-at></layer>"#;
        let pages = render(&format!(
            r#"<section page-format="upper-roman"><layout><text>One</text></layout>{layer}</section>
               <section><layout><text>Two</text><text paginate="break-before">Three</text></layout>{layer}</section>"#
        ));
        assert_eq!(footers(&pages), ["I/III", "II/III", "III/III"]);
    }

    #[test]
    fn a_contents_longer_than_a_page_moves_the_numbers_of_the_sections_after_it() {
        // Seventy entries of two lines each list on three pages, which the guess of one gets wrong by two.
        let pages = render(&[section(r#"page-format="lower-roman""#, "Front", 1), TOC.to_string(), section("", "Part", 70)].concat());
        assert_eq!(pages.len(), 74);
        assert_eq!(texts(&pages[4]).first().map(String::as_str), Some("Part 1"));
        let footers = footers(&pages);
        assert_eq!(footers[..5], ["i/lxxiv", "ii/lxxiv", "iii/lxxiv", "iv/lxxiv", "v/lxxiv"]);
        assert_eq!(footers.last().map(String::as_str), Some("lxxiv/lxxiv"));
        let contents: Vec<String> = pages[1..4].iter().flat_map(texts).collect();
        let entry = |label: &str| contents.iter().position(|t| t == label).map(|i| contents[i + 1].as_str());
        assert_eq!(entry("Front 1"), Some("i"));
        assert_eq!(entry("Part 1"), Some("v"));
        assert_eq!(entry("Part 70"), Some("lxxiv"));
    }

    #[test]
    fn the_catalog_has_a_page_label_range_at_each_start() {
        let xml = format!(
            r#"<lpdf version="1"><document>{}{}{}</document></lpdf>"#,
            section(r#"page-format="lower-roman""#, "Cover", 2),
            section(r#"page-start="1" page-format="decimal""#, "Chapter", 2),
            section(r#"page-start="5" page-prefix="A-""#, "Appendix", 1),
        );
        let pdf = crate::LpdfEngine::render_xml_to_pdf_bytes(&xml).unwrap();
        let text = String::from_utf8_lossy(&pdf).split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(text.contains("/PageLabels << /Nums [0 << /S /r >> 2 << /S /D >> 4 << /S /D /P (A-) /St 5 >>] >>"), "{text}");

        let plain = format!(r#"<lpdf version="1"><document>{}</document></lpdf>"#, section("", "Part", 2));
        let pdf = crate::LpdfEngine::render_xml_to_pdf_bytes(&plain).unwrap();
        assert!(!String::from_utf8_lossy(&pdf).contains("/PageLabels"));
    }

    #[test]
    fn a_page_start_below_one_fails_with_the_line() {
        for value in ["0", "-1"] {
            let xml = format!("<lpdf version=\"1\"><document>\n<section page-start=\"{value}\"><layout/></section></document></lpdf>");
            let error = parse(&xml).err().unwrap_or_default();
            assert!(error.starts_with("<section> at line 2, col 1: page-start must be a whole number from 1 to 2147483647"), "{error}");
        }
        let error = parse(r#"<lpdf version="1"><document><section page-format="cover"><layout/></section></document></lpdf>"#)
            .err().unwrap_or_default();
        assert!(error.contains("page-format"), "{error}");
    }

    #[test]
    fn a_tree_sets_the_numbering_as_xml_does() {
        let tree = |attrs: &str| format!(
            r#"{{"version":1,"type":"document","attrs":{{}},"nodes":[{{"type":"section","attrs":{attrs},"nodes":[
                {{"type":"layout","attrs":{{}},"nodes":[{{"type":"region","attrs":{{"pin":"bottom"}},"nodes":[{{"type":"text","attrs":{{}},"nodes":["p. {{page}}/{{pages}}"]}}]}}]}}]}}]}}"#
        );
        let mut doc = parse_tree(&tree(r#"{"page-start":4,"page-format":"upper-alpha","page-prefix":"B-"}"#)).unwrap();
        assert_eq!(footers(&layout_document(&doc.section_layouts())), ["B-D/A"]);
        let error = parse_tree(&tree(r#"{"page-start":0}"#)).err().unwrap_or_default();
        assert_eq!(error, "section: page-start must be a whole number from 1 to 2147483647, not 0");
    }
}
