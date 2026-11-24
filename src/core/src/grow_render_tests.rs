//! A section whose page grows (`grow`), rendered: the XML and the kit tree of one document give the same PDF, and a
//! bulk container in a section that grows lays out all of its records on the one page. The WASI crate has its own
//! `LpdfEngine`, so these run with the core library only.

use crate::{LpdfEngine, RenderEncryption};

/// How many pages `pdf` has: each has one content stream.
fn page_count(pdf: &[u8]) -> usize {
    String::from_utf8_lossy(pdf).matches("/Contents ").count()
}

#[test]
fn a_growing_section_renders_the_same_from_its_xml_and_its_kit_tree() {
    let xml = r##"<lpdf version="1"><document size="A5"><section size="80mm 40mm" grow="width height">
        <layer><rect anchor="bottom-right" x="-5pt" y="-5pt" width="20pt" height="10pt" fill="#0000ff"/></layer>
        <layout margin="10pt">
          <table cols="auto auto"><tr><td><text>The first cell of the table, long enough to pass the roll</text></td><td><text>The second cell</text></td></tr></table>
          <text>Noted<fn>The note.</fn></text>
          <region pin="bottom"><text>Footer</text></region>
        </layout>
    </section></document></lpdf>"##;
    let tree = r##"{"version":1,"type":"document","attrs":{"size":"A5"},"nodes":[{"type":"section","attrs":{"size":"80mm 40mm","grow":"width height"},"nodes":[
        {"type":"layer","attrs":{},"nodes":[{"type":"rect","attrs":{"anchor":"bottom-right","x":"-5pt","y":"-5pt","width":"20pt","height":"10pt","fill":"#0000ff"}}]},
        {"type":"layout","attrs":{"margin":"10pt"},"nodes":[
          {"type":"table","attrs":{"cols":"auto auto"},"nodes":[{"type":"tr","attrs":{},"nodes":[
            {"type":"td","attrs":{},"nodes":[{"type":"text","attrs":{},"nodes":["The first cell of the table, long enough to pass the roll"]}]},
            {"type":"td","attrs":{},"nodes":[{"type":"text","attrs":{},"nodes":["The second cell"]}]}]}]},
          {"type":"text","attrs":{},"nodes":["Noted",{"type":"fn","attrs":{},"nodes":["The note."]}]},
          {"type":"region","attrs":{"pin":"bottom"},"nodes":[{"type":"text","attrs":{},"nodes":["Footer"]}]}]}]}]}"##;
    let engine = LpdfEngine::new("");
    let from_xml = engine.render_xml(xml, None, &RenderEncryption::default()).expect("the XML renders");
    let from_tree = engine.render_json(tree, &RenderEncryption::default()).expect("the tree renders");
    assert_eq!(page_count(&from_xml), 1);
    assert!(from_xml == from_tree, "the XML and the tree render the same PDF");
}

#[test]
fn a_bulk_container_in_a_growing_section_lays_out_all_of_its_records_on_the_one_page() {
    let xml = |bulk: &str| format!(
        r#"<lpdf version="1"><document size="A5"><section size="80mm 40mm" grow="height"><layout margin="10pt">
        <column{bulk}><text data-source="rows" data-value="name" max-lines="1" ellipsis="true">-</text></column>
        </layout></section></document></lpdf>"#
    );
    let rows: Vec<String> = (1..=150).map(|n| format!(r#"{{"name":"Row {n}"}}"#)).collect();
    let data = format!(r#"{{"rows":[{}]}}"#, rows.join(","));
    let engine = LpdfEngine::new("");
    let render = |xml: String| engine.render_xml(&xml, Some(&data), &RenderEncryption::default()).expect("renders");
    let bulk = render(xml(r#" bulk="true""#));
    assert_eq!(page_count(&bulk), 1);
    assert!(bulk == render(xml("")), "the same page as the column without bulk");
}
