//! The logical structure of a tagged PDF (ISO 32000-1, 14.7 and 14.8), built while the pages are drawn: what each
//! drawn thing is, from the tags the layout put on the render tree (`render::Tag`), and the marked content that ties
//! the drawing to it. Everything that is not tagged is marked as an artifact, which a screen reader skips.
//!
//! `Marks` is off for a document that is not tagged, and then every call here draws what it is given and nothing
//! else, so such a document keeps its bytes. `pdf.rs` writes the elements, the parent tree and the catalog entries once
//! the pages are drawn (`write_structure`).

use std::collections::HashMap;

use pdf_writer::{Content, Name};

use crate::render::{Tag, TagKind};

/// A kind of structure element, by the standard structure types of ISO 32000-1, 14.8.4.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Kind {
    Document,
    P,
    H(u8),
    Table,
    THead,
    TBody,
    TR,
    /// A header cell, which heads its column, and a data cell, with the columns and rows each spans.
    TH { col_span: u16, row_span: u16 },
    TD { col_span: u16, row_span: u16 },
    L,
    LI,
    Lbl,
    LBody,
    Link,
    /// An image, a barcode or a layer, with what it shows.
    Figure(String),
    Note,
    TOC,
    TOCI,
}

impl Kind {
    /// The structure type's name, as `/S` writes it, and as the tag of the marked content drawn in it.
    pub(crate) fn name(&self) -> &'static [u8] {
        match self {
            Kind::Document  => b"Document",
            Kind::P         => b"P",
            Kind::H(1)      => b"H1",
            Kind::H(2)      => b"H2",
            Kind::H(3)      => b"H3",
            Kind::H(4)      => b"H4",
            Kind::H(5)      => b"H5",
            Kind::H(_)      => b"H6",
            Kind::Table     => b"Table",
            Kind::THead     => b"THead",
            Kind::TBody     => b"TBody",
            Kind::TR        => b"TR",
            Kind::TH { .. } => b"TH",
            Kind::TD { .. } => b"TD",
            Kind::L         => b"L",
            Kind::LI        => b"LI",
            Kind::Lbl       => b"Lbl",
            Kind::LBody     => b"LBody",
            Kind::Link      => b"Link",
            Kind::Figure(_) => b"Figure",
            Kind::Note      => b"Note",
            Kind::TOC       => b"TOC",
            Kind::TOCI      => b"TOCI",
        }
    }
}

/// What an element holds, in reading order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Kid {
    /// A structure element, by its place in `Marks::elements`.
    Element(usize),
    /// A marked-content sequence: the stream it is in, by its place in `Marks::streams`, and its MCID there.
    Marked { stream: usize, mcid: i32 },
    /// An annotation, by the number `Marks::annotation` gave it. A link that `pdf.rs` drops leaves it out.
    Annotation(usize),
}

/// A structure element.
#[derive(Debug, Clone)]
pub(crate) struct Element {
    pub kind:   Kind,
    pub parent: usize,
    pub kids:   Vec<Kid>,
    /// The page it is first drawn on.
    pub page:   usize,
    /// The words drawn in it and in the elements below it, for the description of a link.
    pub words:  String,
    /// The column a table cell starts in, which keeps the cells of a row in order.
    column:     Option<u16>,
    /// The body of a table, made when its first row of data is drawn.
    body:       Option<usize>,
}

/// A content stream that marked content is drawn in: a page's own, or a transparency group drawn on a page. A
/// group's place among the page's groups is known once it is drawn.
#[derive(Debug, Clone)]
pub(crate) struct MarkedStream {
    pub page:   usize,
    pub group:  Option<usize>,
    /// The element each MCID of the stream belongs to: MCID `i` is drawn in `owners[i]`.
    pub owners: Vec<usize>,
}

/// The structure a document's pages are drawn in, and where the drawing is in it.
pub(crate) struct Marks {
    on:              bool,
    pub elements:    Vec<Element>,
    /// The element of each tag id seen, so the parts of a node split across lines or pages stay one element.
    by_id:           HashMap<u32, usize>,
    pub streams:     Vec<MarkedStream>,
    /// The stream being drawn into.
    stream:          usize,
    page:            usize,
    /// The element the drawing belongs to; 0 is the document.
    parent:          usize,
    /// Inside a figure or an artifact, which is marked as a whole: nothing in it is marked again.
    sealed:          bool,
    /// The element a sealed figure is, or the document inside an artifact: what a link inside it belongs to.
    seal_owner:      usize,
    /// How many annotations `annotation` has numbered.
    annotations:     usize,
    /// For an element whose last drawn kid is a link, that link's URL and element. The next part of a link that
    /// wraps onto another line, or onto the next page, where its paragraph is entered again, is drawn as a link of
    /// its own and goes on in it. Anything else drawn in the element ends it: words, a figure, an element.
    last_links:      HashMap<usize, (String, usize)>,
}

/// Where the drawing was before it entered an element, to go back to.
pub(crate) struct Entered {
    parent: usize,
}

impl Marks {
    /// The marks of a document that is not tagged: every call draws and marks nothing.
    pub(crate) fn off() -> Self {
        Marks {
            on: false, elements: Vec::new(), by_id: HashMap::new(), streams: Vec::new(), stream: 0, page: 0,
            parent: 0, sealed: false, seal_owner: 0, annotations: 0, last_links: HashMap::new(),
        }
    }

    /// The marks of a tagged document: a `Document` element that holds everything.
    pub(crate) fn tagged() -> Self {
        let document = Element {
            kind: Kind::Document, parent: 0, kids: Vec::new(), page: 0, words: String::new(), column: None, body: None,
        };
        Marks { on: true, elements: vec![document], ..Marks::off() }
    }

    pub(crate) fn is_on(&self) -> bool {
        self.on
    }

    /// Whether what is drawn now is to be marked: the document is tagged, and the drawing is not inside a figure or an
    /// artifact that is marked as a whole.
    pub(crate) fn marking(&self) -> bool {
        self.on && !self.sealed
    }

    /// Start drawing page `page`, in its own content stream, at the top of the structure.
    pub(crate) fn begin_page(&mut self, page: usize) {
        if !self.on { return; }
        self.page = page;
        self.streams.push(MarkedStream { page, group: None, owners: Vec::new() });
        self.stream = self.streams.len() - 1;
        self.parent = 0;
        self.sealed = false;
    }

    /// Draw content that a reader reads, such as a line of text, in the element the drawing is in: a marked-content
    /// sequence with an MCID, which the element lists. Outside every element it is an artifact.
    pub(crate) fn content(&mut self, content: &mut Content, words: &str, draw: impl FnOnce(&mut Content, &mut Self)) {
        if !self.marking() {
            return draw(content, self);
        }
        if self.parent == 0 {
            return self.artifact(content, draw);
        }
        // Words drawn after a link, in the element it is in, end it: a link drawn after them is another one.
        self.last_links.remove(&self.parent);
        let owner = self.parent;
        let mcid = self.next_mcid(owner);
        self.add_words(words);
        content.begin_marked_content_with_properties(Name(self.elements[owner].kind.name())).properties().identify(mcid);
        draw(content, self);
        content.end_marked_content();
    }

    /// Draw what is not read, such as a background, a border or a rule, as an artifact, with everything inside it.
    pub(crate) fn artifact(&mut self, content: &mut Content, draw: impl FnOnce(&mut Content, &mut Self)) {
        if !self.marking() {
            return draw(content, self);
        }
        content.begin_marked_content(Name(b"Artifact"));
        self.seal(0, content, draw);
        content.end_marked_content();
    }

    /// Draw something that is marked as a whole by its tag: a figure, with its description, or an artifact. Without a
    /// tag it is an artifact, as an image of a layer is.
    pub(crate) fn whole(&mut self, content: &mut Content, tag: Option<&Tag>, draw: impl FnOnce(&mut Content, &mut Self)) {
        match tag.map(|tag| &tag.kind) {
            Some(TagKind::Figure(alt)) if self.marking() => {
                // What a figure shows is what a link around it says. A figure drawn after a link ends it, as the new
                // element does (`element`).
                self.add_words(alt);
                let figure = self.element(0, Kind::Figure(alt.clone()), self.parent);
                let mcid = self.next_mcid(figure);
                content.begin_marked_content_with_properties(Name(b"Figure")).properties().identify(mcid);
                self.seal(figure, content, draw);
                content.end_marked_content();
            }
            _ => self.artifact(content, draw),
        }
    }

    /// Whether `tag` marks what it is on as a whole: a figure or an artifact. A header row that was drawn on an earlier
    /// page is drawn again as an artifact, since it is not new content.
    pub(crate) fn is_whole(&self, tag: &Tag) -> bool {
        match tag.kind {
            TagKind::Figure(_) | TagKind::Artifact => true,
            TagKind::TableHead => self.by_id.get(&tag.id).is_some_and(|&head| self.elements[head].page < self.page),
            _ => false,
        }
    }

    /// Go into the element of `tag`, which is not one marked as a whole (`is_whole`): what is drawn until `leave` is
    /// in it. The element is made the first time its id is seen, under the element the drawing is in; entered again,
    /// as a paragraph is on the next page, it goes on where it was, a link it ended in included.
    pub(crate) fn enter(&mut self, tag: &Tag) -> Option<Entered> {
        if !self.marking() {
            return None;
        }
        let parent = self.parent;
        self.parent = match &tag.kind {
            TagKind::Paragraph => self.element(tag.id, Kind::P, parent),
            TagKind::Heading(level) => self.element(tag.id, Kind::H(*level), parent),
            TagKind::Table => self.element(tag.id, Kind::Table, parent),
            // The header row is a `THead` that holds one row; the cells go in the row.
            TagKind::TableHead => match self.by_id.get(&tag.id) {
                Some(&row) => row,
                None => {
                    let head = self.element(0, Kind::THead, parent);
                    let row = self.element(0, Kind::TR, head);
                    self.by_id.insert(tag.id, row);
                    row
                }
            },
            // A row of the body goes in the table's `TBody`, made with its first row.
            TagKind::TableRow => {
                let body = if self.elements[parent].kind == Kind::Table {
                    match self.elements[parent].body {
                        Some(body) => body,
                        None => {
                            let body = self.element(0, Kind::TBody, parent);
                            self.elements[parent].body = Some(body);
                            body
                        }
                    }
                } else {
                    parent
                };
                self.element(tag.id, Kind::TR, body)
            }
            TagKind::TableCell { header, col_span, row_span, column } => {
                let kind = if *header {
                    Kind::TH { col_span: *col_span, row_span: *row_span }
                } else {
                    Kind::TD { col_span: *col_span, row_span: *row_span }
                };
                self.cell(tag.id, kind, parent, *column)
            }
            TagKind::List => self.element(tag.id, Kind::L, parent),
            TagKind::ListItem => self.element(tag.id, Kind::LI, parent),
            TagKind::ListLabel => self.element(tag.id, Kind::Lbl, parent),
            TagKind::ListBody => self.element(tag.id, Kind::LBody, parent),
            TagKind::Note => self.element(tag.id, Kind::Note, parent),
            // The entries of a table of contents stand in a `TOC`: the one just before, or a new one.
            TagKind::TocItem => {
                let toc = match self.elements[parent].kids.last() {
                    Some(Kid::Element(last)) if self.elements[*last].kind == Kind::TOC => *last,
                    _ => self.element(0, Kind::TOC, parent),
                };
                self.element(tag.id, Kind::TOCI, toc)
            }
            TagKind::Figure(_) | TagKind::Artifact => unreachable!("a figure or an artifact is marked as a whole"),
        };
        Some(Entered { parent })
    }

    /// Go back to the element the drawing was in before `enter`.
    pub(crate) fn leave(&mut self, entered: Option<Entered>) {
        if let Some(entered) = entered {
            self.parent = entered.parent;
        }
    }

    /// Go into a link to `url`, whose annotation is the next one numbered: its words are drawn in it. A link is drawn
    /// as a link for each part when it runs onto another line or page, and the parts stay one element, which holds an
    /// annotation for each: a `<link>` around content by its tag `id`, as a paragraph is; a link of a text's words,
    /// which has no id, when its part, to the same URL, is drawn right after the one before in the same element.
    /// Inside a figure or an artifact, the link is an element of its own under it, with no words, as its annotation
    /// still is in the structure.
    pub(crate) fn begin_link(&mut self, url: &str, id: u32) -> Option<OpenLink> {
        if !self.on {
            return None;
        }
        let parent = self.parent;
        let link = if self.sealed {
            self.element(0, Kind::Link, self.seal_owner)
        } else {
            let link = match self.last_links.get(&parent) {
                _ if id != 0 => self.element(id, Kind::Link, parent),
                Some((last_url, link)) if last_url == url => *link,
                _ => self.element(0, Kind::Link, parent),
            };
            self.parent = link;
            link
        };
        Some(OpenLink { entered: Entered { parent }, element: link, url: url.to_string() })
    }

    /// Leave the link `begin_link` went into, and number its annotation: the number goes to the annotation's data,
    /// and the link lists it after its words.
    pub(crate) fn end_link(&mut self, link: Option<OpenLink>) -> Option<LinkMark> {
        let OpenLink { entered, element, url } = link?;
        self.parent = entered.parent;
        let number = self.annotation();
        self.elements[element].kids.push(Kid::Annotation(number));
        if !self.sealed {
            self.last_links.insert(self.parent, (url, element));
        }
        Some(LinkMark { element, number })
    }

    /// The link of the attribution line, which is drawn as an artifact: an element of its own under the document,
    /// with the name it shows. It is drawn after each page's content and is not part of it, so it ends no link of the
    /// document.
    pub(crate) fn attribution_link(&mut self, words: &str) -> Option<LinkMark> {
        if !self.on {
            return None;
        }
        let element = self.new_element(Kind::Link, 0, None);
        self.elements[0].kids.push(Kid::Element(element));
        self.elements[element].words.push_str(words);
        let number = self.annotation();
        self.elements[element].kids.push(Kid::Annotation(number));
        Some(LinkMark { element, number })
    }

    /// Start drawing into a transparency group, a content stream of its own: its MCIDs count from 0 again. Returns
    /// where to go back to.
    pub(crate) fn begin_group(&mut self) -> Option<usize> {
        if !self.marking() {
            return None;
        }
        let back = self.stream;
        self.streams.push(MarkedStream { page: self.page, group: None, owners: Vec::new() });
        self.stream = self.streams.len() - 1;
        Some(back)
    }

    /// Go back to the stream the group was drawn from, and note which of the page's groups it is.
    pub(crate) fn end_group(&mut self, back: Option<usize>, group: usize) {
        if let Some(back) = back {
            self.streams[self.stream].group = Some(group);
            self.stream = back;
        }
    }

    /// The element of `id`, or a new one of `kind` under `parent`. An id of 0 is always a new one.
    fn element(&mut self, id: u32, kind: Kind, parent: usize) -> usize {
        if id != 0 && let Some(&known) = self.by_id.get(&id) {
            return known;
        }
        let index = self.new_element(kind, parent, None);
        self.elements[parent].kids.push(Kid::Element(index));
        // A new element after a link, in the element it is in, ends it.
        self.last_links.remove(&parent);
        if id != 0 {
            self.by_id.insert(id, index);
        }
        index
    }

    /// The element of a table cell, kept among the cells of its row in the order of their columns: a cell that spans
    /// rows is drawn after the rows it spans.
    fn cell(&mut self, id: u32, kind: Kind, row: usize, column: u16) -> usize {
        if let Some(&known) = self.by_id.get(&id) {
            return known;
        }
        let index = self.new_element(kind, row, Some(column));
        self.last_links.remove(&row);
        let at = self.elements[row].kids.iter().position(|kid| match kid {
            Kid::Element(cell) => self.elements[*cell].column.is_some_and(|c| c > column),
            _ => false,
        });
        let kids = &mut self.elements[row].kids;
        match at {
            Some(at) => kids.insert(at, Kid::Element(index)),
            None => kids.push(Kid::Element(index)),
        }
        self.by_id.insert(id, index);
        index
    }

    fn new_element(&mut self, kind: Kind, parent: usize, column: Option<u16>) -> usize {
        self.elements.push(Element {
            kind, parent, kids: Vec::new(), page: self.page, words: String::new(), column, body: None,
        });
        self.elements.len() - 1
    }

    fn next_mcid(&mut self, owner: usize) -> i32 {
        let stream = &mut self.streams[self.stream];
        let mcid = stream.owners.len() as i32;
        stream.owners.push(owner);
        self.elements[owner].kids.push(Kid::Marked { stream: self.stream, mcid });
        mcid
    }

    fn annotation(&mut self) -> usize {
        self.annotations += 1;
        self.annotations
    }

    /// Note the words drawn in the element the drawing is in, and in each element above it.
    fn add_words(&mut self, words: &str) {
        let mut at = self.parent;
        while at != 0 {
            let element = &mut self.elements[at];
            if !element.words.is_empty() {
                element.words.push(' ');
            }
            element.words.push_str(words.trim());
            at = element.parent;
        }
    }

    fn seal(&mut self, owner: usize, content: &mut Content, draw: impl FnOnce(&mut Content, &mut Self)) {
        let (sealed, seal_owner) = (self.sealed, self.seal_owner);
        self.sealed = true;
        self.seal_owner = owner;
        draw(content, self);
        self.sealed = sealed;
        self.seal_owner = seal_owner;
    }
}

/// A link `begin_link` went into: where the drawing was, its element and its URL.
pub(crate) struct OpenLink {
    entered: Entered,
    element: usize,
    url:     String,
}

/// The element of a link annotation, and the number `Marks` gave the annotation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LinkMark {
    pub element: usize,
    pub number:  usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(id: u32, kind: TagKind) -> Tag {
        Tag { id, kind }
    }

    fn kinds(marks: &Marks, of: usize) -> Vec<Kind> {
        marks.elements[of].kids.iter().filter_map(|kid| match kid {
            Kid::Element(e) => Some(marks.elements[*e].kind.clone()),
            _ => None,
        }).collect()
    }

    #[test]
    fn marks_that_are_off_draw_and_mark_nothing() {
        let mut marks = Marks::off();
        let mut content = Content::new();
        marks.begin_page(0);
        marks.content(&mut content, "x", |c, _| { c.move_to(0.0, 0.0); });
        marks.artifact(&mut content, |c, _| { c.line_to(1.0, 1.0); });
        assert!(marks.enter(&tag(1, TagKind::Paragraph)).is_none());
        assert_eq!(content.finish(), b"0 0 m\n1 1 l");
    }

    #[test]
    fn text_in_an_element_gets_an_mcid_and_text_outside_every_element_is_an_artifact() {
        let mut marks = Marks::tagged();
        let mut content = Content::new();
        marks.begin_page(0);
        marks.content(&mut content, "loose", |_, _| {});
        let entered = marks.enter(&tag(7, TagKind::Heading(1)));
        marks.content(&mut content, "Invoice", |_, _| {});
        marks.leave(entered);
        let bytes = String::from_utf8(content.finish()).unwrap();
        assert_eq!(bytes, "/Artifact BMC\nEMC\n/H1 <<\n  /MCID 0\n>> BDC\nEMC");
        assert_eq!(kinds(&marks, 0), [Kind::H(1)]);
        assert_eq!(marks.elements[1].words, "Invoice");
        assert_eq!(marks.streams[0].owners, [1]);
    }

    #[test]
    fn the_parts_of_a_split_text_stay_one_element() {
        let mut marks = Marks::tagged();
        let mut content = Content::new();
        for page in 0..2 {
            marks.begin_page(page);
            let entered = marks.enter(&tag(3, TagKind::Paragraph));
            marks.content(&mut content, "words", |_, _| {});
            marks.leave(entered);
        }
        assert_eq!(kinds(&marks, 0), [Kind::P]);
        assert_eq!(marks.elements[1].kids, [Kid::Marked { stream: 0, mcid: 0 }, Kid::Marked { stream: 1, mcid: 0 }]);
    }

    #[test]
    fn a_table_has_its_header_row_in_a_thead_and_its_rows_in_a_tbody_and_a_repeated_header_is_an_artifact() {
        let mut marks = Marks::tagged();
        marks.begin_page(0);
        let table = marks.enter(&tag(1, TagKind::Table));
        let head = tag(2, TagKind::TableHead);
        assert!(!marks.is_whole(&head));
        let entered = marks.enter(&head);
        marks.leave(entered);
        let row = marks.enter(&tag(3, TagKind::TableRow));
        marks.leave(row);
        marks.leave(table);
        assert_eq!(kinds(&marks, 1), [Kind::THead, Kind::TBody]);
        marks.begin_page(1);
        assert!(marks.is_whole(&head), "the header drawn again on the next page");
    }

    #[test]
    fn the_cells_of_a_row_stay_in_the_order_of_their_columns() {
        let mut marks = Marks::tagged();
        marks.begin_page(0);
        let row = marks.enter(&tag(1, TagKind::TableRow));
        for (id, column) in [(2, 1), (3, 2), (4, 0)] {
            let cell = marks.enter(&tag(id, TagKind::TableCell { header: false, col_span: 1, row_span: 1, column }));
            marks.leave(cell);
        }
        marks.leave(row);
        let cells: Vec<u16> = marks.elements[1].kids.iter().filter_map(|kid| match kid {
            Kid::Element(e) => marks.elements[*e].column,
            _ => None,
        }).collect();
        assert_eq!(cells, [0, 1, 2]);
    }

    #[test]
    fn entries_of_a_table_of_contents_stand_in_one_toc() {
        let mut marks = Marks::tagged();
        marks.begin_page(0);
        for id in [1, 2] {
            let entry = marks.enter(&tag(id, TagKind::TocItem));
            marks.leave(entry);
        }
        assert_eq!(kinds(&marks, 0), [Kind::TOC]);
        assert_eq!(kinds(&marks, 1), [Kind::TOCI, Kind::TOCI]);
    }

    #[test]
    fn a_figure_is_marked_as_a_whole_and_holds_its_link() {
        let mut marks = Marks::tagged();
        let mut content = Content::new();
        marks.begin_page(0);
        let figure = tag(0, TagKind::Figure("A map".into()));
        assert!(marks.is_whole(&figure));
        let mut link = None;
        marks.whole(&mut content, Some(&figure), |c, m| {
            m.content(c, "City", |_, _| {});
            let open = m.begin_link("https://a.example", 0);
            link = m.end_link(open);
        });
        let bytes = String::from_utf8(content.finish()).unwrap();
        assert_eq!(bytes, "/Figure <<\n  /MCID 0\n>> BDC\nEMC", "nothing inside it is marked again");
        assert_eq!(kinds(&marks, 0), [Kind::Figure("A map".into())]);
        assert_eq!(kinds(&marks, 1), [Kind::Link]);
        assert_eq!(link, Some(LinkMark { element: 2, number: 1 }));
    }

    /// Draw a link to `url` with `words` in it, in the element the drawing is in.
    fn link(marks: &mut Marks, content: &mut Content, url: &str, words: &str) -> Option<LinkMark> {
        let open = marks.begin_link(url, 0);
        marks.content(content, words, |_, _| {});
        marks.end_link(open)
    }

    #[test]
    fn the_parts_of_a_link_that_wraps_stay_one_link_with_an_annotation_for_each() {
        let mut marks = Marks::tagged();
        let mut content = Content::new();
        marks.begin_page(0);
        let p = marks.enter(&tag(1, TagKind::Paragraph));
        marks.content(&mut content, "See", |_, _| {});
        let first = link(&mut marks, &mut content, "https://a.example", "the guide").expect("a link");
        let second = link(&mut marks, &mut content, "https://a.example", "to it").expect("a link");
        marks.content(&mut content, "and", |_, _| {});
        let third = link(&mut marks, &mut content, "https://a.example", "again").expect("a link");
        let other = link(&mut marks, &mut content, "https://b.example", "elsewhere").expect("a link");
        marks.leave(p);
        assert_eq!(first.element, second.element, "the part on the next line goes on in the link");
        assert_ne!(first.number, second.number, "each part has its annotation");
        assert_eq!(marks.elements[first.element].words, "the guide to it");
        assert_ne!(third.element, first.element, "after words read outside it, a link is another one");
        assert_ne!(other.element, third.element, "a link to another URL is another one");
        assert_eq!(kinds(&marks, 1), [Kind::Link, Kind::Link, Kind::Link]);
    }

    #[test]
    fn a_figure_drawn_between_two_links_ends_the_first() {
        let mut marks = Marks::tagged();
        let mut content = Content::new();
        marks.begin_page(0);
        let figure = |alt: &str| tag(0, TagKind::Figure(alt.into()));
        let open = marks.begin_link("https://a.example", 0);
        marks.whole(&mut content, Some(&figure("First")), |_, _| {});
        let first = marks.end_link(open).expect("a link");
        marks.whole(&mut content, Some(&figure("Middle")), |_, _| {});
        let open = marks.begin_link("https://a.example", 0);
        marks.whole(&mut content, Some(&figure("Last")), |_, _| {});
        let last = marks.end_link(open).expect("a link");
        assert_ne!(first.element, last.element);
        assert_eq!(kinds(&marks, 0), [Kind::Link, Kind::Figure("Middle".into()), Kind::Link], "in the order they are drawn");
        assert_eq!(marks.elements[first.element].words, "First");
        assert_eq!(marks.elements[last.element].words, "Last");
    }

    #[test]
    fn a_link_that_runs_onto_the_next_page_goes_on_where_its_paragraph_does() {
        let mut marks = Marks::tagged();
        let mut content = Content::new();
        marks.begin_page(0);
        let p = marks.enter(&tag(1, TagKind::Paragraph));
        marks.content(&mut content, "See", |_, _| {});
        let first = link(&mut marks, &mut content, "https://a.example", "the guide").expect("a link");
        marks.leave(p);
        // The foot of the page: a note, an element of its own, which is not in the paragraph.
        let note = marks.enter(&tag(0, TagKind::Note));
        marks.content(&mut content, "1 A note", |_, _| {});
        marks.leave(note);
        marks.begin_page(1);
        let p = marks.enter(&tag(1, TagKind::Paragraph));
        let second = link(&mut marks, &mut content, "https://a.example", "to it").expect("a link");
        marks.leave(p);
        let other = marks.enter(&tag(2, TagKind::Paragraph));
        let third = link(&mut marks, &mut content, "https://a.example", "again").expect("a link");
        marks.leave(other);
        assert_eq!(first.element, second.element, "the part on the next page goes on in the link");
        assert_eq!(marks.elements[first.element].words, "the guide to it");
        assert_ne!(third.element, first.element, "a link in another paragraph is another one");
    }

    #[test]
    fn a_figure_s_description_is_what_a_link_around_it_says() {
        let mut marks = Marks::tagged();
        let mut content = Content::new();
        marks.begin_page(0);
        let open = marks.begin_link("https://a.example", 0);
        marks.whole(&mut content, Some(&tag(0, TagKind::Figure("A chart".into()))), |_, _| {});
        let mark = marks.end_link(open).expect("a link");
        assert_eq!(marks.elements[mark.element].words, "A chart");
    }

    #[test]
    fn a_transparency_group_counts_its_mcids_from_zero() {
        let mut marks = Marks::tagged();
        let mut content = Content::new();
        marks.begin_page(0);
        let p = marks.enter(&tag(1, TagKind::Paragraph));
        marks.content(&mut content, "a", |_, _| {});
        let back = marks.begin_group();
        marks.content(&mut content, "b", |_, _| {});
        marks.end_group(back, 0);
        marks.content(&mut content, "c", |_, _| {});
        marks.leave(p);
        assert_eq!(marks.streams[0].owners, [1, 1]);
        assert_eq!(marks.streams[1].owners, [1]);
        assert_eq!(marks.streams[1].group, Some(0));
    }
}
