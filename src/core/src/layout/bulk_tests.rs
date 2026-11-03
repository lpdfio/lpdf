//! Bulk containers (`bulk`), laid out a page at a time from their records: they lay out the pages that the same
//! container lays out with its template expanded by the binding, for records that keep the rules, but for a record
//! that the layout without bulk splits across pages: bulk never splits one, and moves it whole to the next page.

use super::*;
use crate::parse::parse;
use serde_json::{json, Value};

/// A document of one A4 section with `attrs`, a header and a footer with the page numbers, its layout holding `body`.
fn document(attrs: &str, body: &str) -> String {
    format!(
        r#"<lpdf version="1"><document size="a4"><section{attrs}><layout margin="36pt">
            <region pin="top"><text font-size="9pt">Ledger</text></region>
            {body}
            <region pin="bottom"><text font-size="9pt" align="right">Page {{page}} of {{pages}}</text></region>
        </layout></section></document></lpdf>"#
    )
}

/// The pages of `xml` bound to `data`, as the engine hands them out.
fn laid_out(xml: &str, data: &Value) -> Result<Vec<RenderPage>, String> {
    let mut doc = parse(xml)?;
    crate::data::apply(&mut doc, data.clone())?;
    let sections = doc.section_layouts();
    layout_document(&sections)
}

fn tree_of(pages: &[RenderPage]) -> String {
    crate::render::pages_to_json(pages, Value::Null, Value::Null)
}

/// Lay out `body` in a section with `attrs`, bound to `data`, with `bulk="true"` on its container and without, and
/// return the pages of the bulk one, after checking that both are the same.
fn same_pages(case: &str, attrs: &str, body: &str, data: &Value) -> Vec<RenderPage> {
    bulk::PARTS.with(|parts| parts.set(0));
    let bulk = laid_out(&document(attrs, body), data).unwrap_or_else(|e| panic!("{case}: {e}"));
    let parts = bulk::PARTS.with(Cell::get);
    let expanded = laid_out(&document(attrs, &body.replace(r#" bulk="true""#, "")), data).unwrap_or_else(|e| panic!("{case}: {e}"));
    assert!(bulk.len() > 3, "{case}: the records should run over several pages, not {}", bulk.len());
    // Every page but the last of them or two takes a part of the records off the bulk container.
    assert!(parts + 2 >= bulk.len(), "{case}: {parts} parts cut from the records of {} pages", bulk.len());
    assert_eq!(bulk.len(), expanded.len(), "{case}: pages");
    for (i, (b, e)) in bulk.iter().zip(&expanded).enumerate() {
        let (b, e) = (tree_of(std::slice::from_ref(b)), tree_of(std::slice::from_ref(e)));
        assert!(b == e, "{case}: page {} differs\nbulk:     {b}\nexpanded: {e}", i + 1);
    }
    bulk
}

/// A value of a few words, now and then of more than a line, for a text that takes up to two.
fn description(i: usize) -> String {
    match i % 9 {
        0 => "Travel, Berlin, two nights at the conference hotel with breakfast and the airport train both ways".to_string(),
        4 => format!("Office supplies, order {i}, paper, toner and a box of pens for the second floor"),
        7 => String::new(),
        _ => format!("Item {i}"),
    }
}

/// `n` records of a ledger: a date, a description, an amount, and a note on every fifth.
fn ledger(n: usize) -> Value {
    let entries: Vec<Value> = (0..n)
        .map(|i| {
            let mut entry = json!({ "date": format!("2026-01-{:02}", i % 28 + 1), "description": description(i), "amount": format!("${}.00", i * 7) });
            if i % 5 == 0 {
                entry["note"] = json!(format!("Checked {i}"));
            }
            entry
        })
        .collect();
    json!({ "total": "$1,204,330.00", "entries": entries })
}

const TITLE: &str = r#"<text font-size="16pt" bold="true" paginate="keep-next" max-lines="1">Ledger 2026</text>"#;
const TOTAL: &str = r#"<text align="right" bold="true" data-value="total" max-lines="1">$0.00</text>"#;

#[test]
fn a_bulk_table_lays_out_the_pages_of_its_rows_expanded() {
    let table = r##"<table cols="70pt 5fr 2fr" stripe="#f2f2f2" border="0.5pt #999999" row-border="0.5pt #cccccc" gap="2pt" padding="4pt" bulk="true">
        <thead background="#333333">
            <td padding="4pt"><text bold="true" color="#ffffff" max-lines="1">Date</text></td>
            <td padding="4pt"><text bold="true" color="#ffffff" max-lines="1">Description</text></td>
            <td padding="4pt"><text bold="true" color="#ffffff" align="right" max-lines="1">Amount</text></td>
        </thead>
        <tr data-source="entries">
            <td padding="4pt"><text data-value="date" max-lines="1" ellipsis="true">-</text></td>
            <td padding="4pt"><text data-value="description" max-lines="2" ellipsis="true">-</text></td>
            <td padding="4pt"><text data-value="amount" align="right" max-lines="1" ellipsis="true">-</text></td>
        </tr>
        <tr><td colspan="2" padding="4pt"><text bold="true" max-lines="1">Total</text></td><td padding="4pt"><text data-value="total" align="right" max-lines="1">-</text></td></tr>
    </table>"##;
    same_pages("a table", "", &format!("{TITLE}{table}{TOTAL}"), &ledger(700));
}

#[test]
fn a_bulk_table_without_a_header_row_lays_out_the_pages_of_its_rows_expanded() {
    let table = r#"<table cols="1fr 1fr" bulk="true">
        <tr data-source="entries">
            <td><text data-value="date" max-lines="1" ellipsis="true">-</text></td>
            <td><text data-value="note" data-if="note" max-lines="1" ellipsis="true">-</text></td>
        </tr>
    </table>"#;
    same_pages("a table without a header row", "", &format!("{TITLE}{table}"), &ledger(900));
}

#[test]
fn a_bulk_list_numbers_and_lays_out_its_items_as_expanded() {
    // From 95, the numbers grow a digit, and from 995 they are wider than their marker.
    let list = r#"<list type="number" start="95" gap="3pt" bulk="true">
        <li><text max-lines="1">Opening balance</text></li>
        <li data-source="entries">
            <text data-value="description" max-lines="2" ellipsis="true">-</text>
            <text data-if="note" data-value="note" font-size="8pt" max-lines="1" ellipsis="true">-</text>
        </li>
        <li><text max-lines="1">Closing balance</text></li>
    </list>"#;
    same_pages("a number list", "", &format!("{TITLE}{list}{TOTAL}"), &ledger(1000));
}

#[test]
fn a_bulk_bullet_list_lays_out_its_items_as_expanded() {
    let list = r#"<list bullet="dash" bulk="true"><li data-source="entries"><text data-value="date" max-lines="1" ellipsis="true">-</text></li></list>"#;
    same_pages("a bullet list", "", list, &ledger(400));
}

#[test]
fn a_bulk_grid_lays_out_its_cells_as_expanded() {
    let grid = r##"<grid cols="3" gap="6pt" bulk="true">
        <text max-lines="1">Addresses</text>
        <column data-source="entries" padding="6pt" border="0.5pt #999999">
            <text bold="true" data-value="date" max-lines="1" ellipsis="true">-</text>
            <text data-value="description" max-lines="2" ellipsis="true">-</text>
            <text data-if="note" data-value="note" max-lines="1" ellipsis="true">-</text>
        </column>
        <text max-lines="1">End</text>
    </grid>"##;
    same_pages("a grid", "", &format!("{TITLE}{grid}{TOTAL}"), &ledger(500));
}

#[test]
fn a_bulk_grid_of_labels_of_one_height_lays_out_its_cells_as_expanded() {
    let grid = r#"<grid col-width="150pt" gap="4pt" bulk="true">
        <column data-source="entries" height="72pt" padding="8pt">
            <text bold="true" data-value="date" max-lines="1" ellipsis="true">-</text>
            <text data-value="description" max-lines="2" ellipsis="true">-</text>
        </column>
    </grid>"#;
    same_pages("a grid of labels", "", grid, &ledger(600));
}

#[test]
fn a_bulk_column_lays_out_its_texts_as_expanded() {
    let column = r#"<column gap="2pt" padding="6pt" bulk="true">
        <text bold="true" max-lines="1">Events</text>
        <text data-source="entries" data-value="description" max-lines="3" ellipsis="true">-</text>
    </column>"#;
    same_pages("a column", "", &format!("{TITLE}{column}{TOTAL}"), &ledger(1200));
}

#[test]
fn a_bulk_table_in_columns_around_an_inset_lays_out_its_rows_as_expanded() {
    let table = r#"<table cols="1fr 2fr" bulk="true">
        <thead><td><text bold="true" max-lines="1">Date</text></td><td><text bold="true" max-lines="1">Amount</text></td></thead>
        <tr data-source="entries">
            <td><text data-value="date" max-lines="1" ellipsis="true">-</text></td>
            <td><text data-value="description" max-lines="2" ellipsis="true">-</text></td>
        </tr>
    </table>"#;
    let inset = r#"<region pin="inset" col="2" top="120pt" height="90pt"><text>A note beside the rows</text></region>"#;
    same_pages("a table in columns", r#" columns="2""#, &format!("{TITLE}{inset}{table}"), &ledger(800));
}

/// The words of every text drawn on `page`, in drawing order.
fn texts_of(page: &RenderPage) -> Vec<String> {
    fn walk(nodes: &[RenderNode], out: &mut Vec<String>) {
        for node in nodes {
            match node {
                RenderNode::Text(text) => out.push(text.content.clone()),
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

/// How many records have their words, `r{i}-{k}`, drawn on more than one of `pages`.
fn split_records(pages: &[RenderPage]) -> usize {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut split = HashSet::new();
    for (i, page) in pages.iter().enumerate() {
        for word in texts_of(page).iter().flat_map(|text| text.split_whitespace()) {
            let Some((record, _)) = word.strip_prefix('r').and_then(|w| w.split_once('-')) else { continue };
            if *seen.entry(record.to_string()).or_insert(i) != i {
                split.insert(record.to_string());
            }
        }
    }
    split.len()
}

/// Lay out `body` with `bulk="true"` and without, and check that bulk moves whole to the next page the records that
/// the layout without it splits across pages.
fn moved_whole(case: &str, body: &str, data: &Value) {
    let bulk = laid_out(&document("", body), data).unwrap_or_else(|e| panic!("{case}: {e}"));
    let expanded = laid_out(&document("", &body.replace(r#" bulk="true""#, "")), data).unwrap_or_else(|e| panic!("{case}: {e}"));
    assert!(split_records(&expanded) > 0, "{case}: without bulk, no record splits, so the case shows nothing");
    assert_eq!(split_records(&bulk), 0, "{case}: with bulk");
}

#[test]
fn a_bulk_list_moves_an_item_of_two_texts_whole_to_the_next_page() {
    let list = r#"<list bullet="dash" bulk="true"><li data-source="events">
        <text data-value="what" max-lines="1">-</text>
        <text data-value="who" font-size="8pt" max-lines="1">-</text>
    </li></list>"#;
    let events: Vec<Value> = (0..400).map(|i| json!({ "what": format!("r{i}-1"), "who": format!("r{i}-2") })).collect();
    moved_whole("a list", list, &json!({ "events": events }));
}

#[test]
fn a_bulk_column_moves_a_text_of_four_lines_or_more_whole_to_the_next_page() {
    // The layout without bulk splits a text of four lines or more, keeping two on either page.
    let column = r#"<column bulk="true">
        <text data-source="events" data-value="line" max-lines="6">-</text>
    </column>"#;
    let events: Vec<Value> = (0..120).map(|i| json!({ "line": (1..=(4 + i % 3) * 12).map(|k| format!("r{i}-{k}")).collect::<Vec<_>>().join(" ") })).collect();
    moved_whole("a column", column, &json!({ "events": events }));
}

const ROWS: &str = r#"<table cols="1fr 1fr" bulk="true">
    <thead><td><text bold="true">Date</text></td><td><text bold="true">Amount</text></td></thead>
    <tr data-source="entries"><td><text data-value="date">-</text></td><td><text data-value="amount">-</text></td></tr>
</table>"#;

#[test]
fn every_page_of_a_bulk_document_shows_the_total_of_its_pages() {
    let pages = laid_out(&document("", ROWS), &ledger(3000)).unwrap();
    let total = pages.len();
    assert!(total > 50, "3,000 rows fill {total} pages");
    for (i, page) in pages.iter().enumerate() {
        let footer = format!("Page {} of {total}", i + 1);
        assert!(texts_of(page).contains(&footer), "page {} shows {:?}, not {footer:?}", i + 1, texts_of(page));
    }
}

#[test]
fn a_bulk_table_with_no_records_lays_out_its_header_row_as_expanded() {
    let body = format!("{TITLE}{ROWS}{TOTAL}");
    let data = json!({ "total": "$0.00", "entries": [] });
    let bulk = laid_out(&document("", &body), &data).unwrap();
    let expanded = laid_out(&document("", &body.replace(r#" bulk="true""#, "")), &data).unwrap();
    assert_eq!(tree_of(&bulk), tree_of(&expanded));
    assert!(texts_of(&bulk[0]).contains(&"Date".to_string()));
}

#[test]
fn a_bulk_table_rendered_without_data_lays_out_its_template_once() {
    let mut doc = parse(&document("", ROWS)).unwrap();
    let pages = layout_document(&doc.section_layouts()).unwrap();
    assert_eq!(pages.len(), 1);
    let texts = texts_of(&pages[0]);
    assert_eq!(texts.iter().filter(|text| *text == "-").count(), 2, "{texts:?}");
}


