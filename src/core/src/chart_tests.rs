use super::*;
use crate::render::{RenderPage, TagKind};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn document(layout: &str) -> String {
    format!(r#"<lpdf version="1"><document size="A4"><section><layout margin="28pt">{layout}</layout></section></document></lpdf>"#)
}

/// The pages of `xml`, with `data` bound when there is some, as a render lays them out.
fn pages(xml: &str, data: Option<&str>) -> Result<Vec<RenderPage>, String> {
    let mut doc = crate::parse::parse(xml)?;
    if let Some(json) = data {
        crate::data::apply(&mut doc, crate::data::parse(json)?)?;
    }
    crate::layout::layout_document(&doc.section_layouts())
}

fn laid_out(layout: &str) -> Vec<RenderPage> {
    pages(&document(layout), None).unwrap_or_else(|e| panic!("{e}"))
}

fn error(layout: &str) -> String {
    pages(&document(layout), None).map(|_| ()).expect_err("the document is refused")
}

fn walk<'a>(nodes: &'a [RenderNode], out: &mut Vec<&'a RenderNode>) {
    for node in nodes {
        out.push(node);
        if let RenderNode::Box(b) = node {
            walk(&b.children, out);
        }
    }
}

fn all(pages: &[RenderPage]) -> Vec<&RenderNode> {
    let mut out = Vec::new();
    for page in pages {
        walk(&page.nodes, &mut out);
    }
    out
}

/// The words the pages write, in order.
fn texts(pages: &[RenderPage]) -> Vec<String> {
    all(pages).into_iter().filter_map(|n| match n { RenderNode::Text(t) => Some(t.content.clone()), _ => None }).collect()
}

/// The rects the pages draw, such as the swatches of a legend: their box and color, in the order they are drawn.
fn bars(pages: &[RenderPage]) -> Vec<(f32, f32, f32, f32, String)> {
    all(pages)
        .into_iter()
        .filter_map(|n| match n {
            RenderNode::CanvasRect(r) => Some((r.x, r.y, r.w, r.h, r.fill.clone().unwrap_or_default())),
            _ => None,
        })
        .collect()
}

/// The bars of the plot, by color in the order each color is first drawn, and in the order they are drawn in it.
fn plot_bars(pages: &[RenderPage]) -> Vec<(f32, f32, f32, f32, String)> {
    all(pages)
        .into_iter()
        .filter_map(|n| match n {
            RenderNode::Bars(b) => Some(b.rects.iter().map(|[x, y, w, h]| (*x, *y, *w, *h, b.fill.clone())).collect::<Vec<_>>()),
            _ => None,
        })
        .flatten()
        .collect()
}

fn spec() -> ChartSpec {
    let tokens = crate::tokens::Tokens::default();
    read_spec(&crate::parse::JsonAttrs(&serde_json::json!({ "attrs": { "type": "bar" } })), &tokens).expect("a bar chart")
}

const REVENUE: &str = r#"<chart type="bar" height="160pt" prefix="$">
    <category label="Jan"><value>41200</value></category>
    <category label="Feb"><value>38900</value></category>
    <category label="Mar"><value>44100</value></category>
</chart>"#;

// ── Numbers ───────────────────────────────────────────────────────────────────

#[test]
fn a_number_is_written_with_its_marks_its_prefix_and_its_suffix() {
    let mut spec = spec();
    assert_eq!(format_number(1234567.891, 2, &spec), "1,234,567.89");
    spec.prefix = "$".into();
    spec.suffix = " ms".into();
    assert_eq!(format_number(-41200.0, 0, &spec), "-$41,200 ms");
    spec.marks = Marks { thousands: Some('.'), decimal: ',' };
    assert_eq!(format_number(1234.5, 1, &spec), "$1.234,5 ms");
    spec.marks = Marks { thousands: None, decimal: '.' };
    assert_eq!(format_number(1234.5, 1, &spec), "$1234.5 ms");
}

#[test]
fn a_number_rounds_half_up_as_it_reads() {
    assert_eq!(format_number(72.25, 1, &spec()), "72.3");
    assert_eq!(format_number(1.005, 2, &spec()), "1.01");
    assert_eq!(format_number(-0.125, 2, &spec()), "-0.13");
    assert_eq!(format_number(999.96, 1, &spec()), "1,000.0");
    assert_eq!(format_number(9.5, 0, &spec()), "10");
    assert_eq!(format_number(3.0, 2, &spec()), "3.00");
    assert_eq!(format_number(0.1 + 0.2, 1, &spec()), "0.3");
}

#[test]
fn a_number_that_rounds_to_zero_has_no_sign() {
    assert_eq!(format_number(-0.004, 2, &spec()), "0.00");
    assert_eq!(format_number(-0.0, 0, &spec()), "0");
}

#[test]
fn a_value_is_a_plain_number_and_keeps_the_decimals_it_was_written_with() {
    assert_eq!(read_number("41200"), Ok(Some(Number { value: 41200.0, decimals: 0 })));
    assert_eq!(read_number(" -3.50 "), Ok(Some(Number { value: -3.5, decimals: 2 })));
    assert_eq!(read_number(".25"), Ok(Some(Number { value: 0.25, decimals: 2 })));
    assert_eq!(read_number(""), Ok(None));
    assert_eq!(read_number("  "), Ok(None));
}

#[test]
fn a_value_with_marks_a_currency_a_percent_sign_or_an_exponent_is_refused() {
    for text in ["41,200", "$41200", "12%", "1e5", "NaN", "inf", "-", "."] {
        let error = read_number(text).expect_err(text);
        assert!(error.contains(&format!("'{text}' is not a plain number")), "{error}");
    }
}

#[test]
fn a_number_of_the_data_is_written_without_an_exponent() {
    let numbers: serde_json::Value = serde_json::from_str("[1e-7, 3781398, 0.12, -2.3, 1e21]").unwrap();
    let written: Vec<String> = numbers.as_array().unwrap().iter().map(|n| plain_number(n.as_number().unwrap())).collect();
    assert_eq!(written, ["0.0000001", "3781398", "0.12", "-2.3", "1000000000000000000000"]);
}

// ── The axis ──────────────────────────────────────────────────────────────────

#[test]
fn the_axis_starts_at_zero_and_runs_in_about_five_round_steps() {
    let axis = Axis::fit(38900.0, 44100.0, 300.0, 10.0);
    assert_eq!((axis.lo, axis.hi, axis.step), (0.0, 50000.0, 10000.0));
    let axis = Axis::fit(0.0, 51800.0, 300.0, 10.0);
    assert_eq!((axis.lo, axis.hi, axis.step), (0.0, 60000.0, 10000.0));
    let axis = Axis::fit(0.0, 0.7, 300.0, 10.0);
    assert_eq!((axis.lo, axis.step, axis.decimals), (0.0, 0.2, 1));
    assert!((axis.hi - 0.8).abs() < 1e-9, "{axis:?}");
}

#[test]
fn a_negative_value_takes_the_axis_below_zero() {
    let axis = Axis::fit(-2.3, 4.1, 300.0, 10.0);
    assert_eq!((axis.lo, axis.hi, axis.step), (-4.0, 6.0, 2.0));
}

#[test]
fn an_axis_of_nothing_but_zero_runs_from_zero_to_one() {
    let axis = Axis::fit(0.0, 0.0, 300.0, 10.0);
    assert_eq!((axis.lo, axis.hi), (0.0, 1.0));
}

#[test]
fn steps_that_would_crowd_take_a_larger_round_step() {
    let roomy = Axis::fit(0.0, 50000.0, 300.0, 14.0);
    let crowded = Axis::fit(0.0, 50000.0, 30.0, 14.0);
    assert_eq!(roomy.step, 10000.0);
    assert_eq!(crowded.step, 50000.0, "{crowded:?}");
}

#[test]
fn min_and_max_only_widen_the_axis() {
    let pages = laid_out(r#"<chart type="bar" height="160pt" min="-50" max="100"><category><value>120</value></category></chart>"#);
    let steps = texts(&pages);
    assert_eq!(steps.first().map(String::as_str), Some("-50"), "{steps:?}");
    assert!(steps.contains(&"150".to_string()), "a value past max still fits: {steps:?}");
    let pages = laid_out(r#"<chart type="bar" height="160pt" min="50"><category><value>120</value></category></chart>"#);
    assert_eq!(texts(&pages).first().map(String::as_str), Some("0"), "a min above zero leaves the bars at zero");
}

// ── Drawing ───────────────────────────────────────────────────────────────────

#[test]
fn a_bar_chart_writes_its_steps_in_full_and_a_label_under_each_bar() {
    let pages = laid_out(REVENUE);
    let words = texts(&pages);
    for expected in ["$0", "$10,000", "$50,000", "Jan", "Feb", "Mar"] {
        assert!(words.contains(&expected.to_string()), "{expected} in {words:?}");
    }
    let bars = plot_bars(&pages);
    assert_eq!(bars.len(), 3);
    assert!(bars.iter().all(|b| b.4 == PALETTE[0]), "one series takes the palette's first color: {bars:?}");
    // Feb is the shortest, Mar the tallest, and all stand on the same baseline.
    let bottoms: Vec<f32> = bars.iter().map(|b| b.1 + b.3).collect();
    assert!(bottoms.iter().all(|b| (b - bottoms[0]).abs() < 0.01), "{bars:?}");
    assert!(bars[1].3 < bars[0].3 && bars[0].3 < bars[2].3, "{bars:?}");
}

#[test]
fn a_chart_of_one_series_has_no_legend_and_one_of_two_has_one_at_the_bottom() {
    let one = texts(&laid_out(r#"<chart type="bar" height="120pt"><series label="Revenue"/><category label="Q1"><value>1</value></category></chart>"#));
    assert!(!one.contains(&"Revenue".to_string()), "{one:?}");
    let two = laid_out(r#"<chart type="bar" height="120pt"><series label="Revenue"/><series label="Costs"/>
        <category label="Q1"><value>1</value><value>2</value></category></chart>"#);
    let words = texts(&two);
    assert!(words.ends_with(&["Revenue".to_string(), "Costs".to_string()]) || words.contains(&"Costs".to_string()), "{words:?}");
    // The legend's swatches are under the bars.
    let swatches: Vec<_> = bars(&two).into_iter().filter(|b| (b.2 - 8.8).abs() < 0.01).collect();
    let lowest_bar = plot_bars(&two).iter().map(|b| b.1 + b.3).fold(0.0, f32::max);
    assert_eq!(swatches.len(), 2);
    assert!(swatches.iter().all(|s| s.1 > lowest_bar), "{swatches:?}");
}

#[test]
fn the_legend_goes_where_legend_puts_it() {
    let chart = |legend: &str| {
        laid_out(&format!(r#"<chart type="bar" height="120pt" legend="{legend}"><series label="A"/><series label="B"/>
            <category><value>1</value><value>2</value></category></chart>"#))
    };
    let swatch = |pages: &[RenderPage]| bars(pages).into_iter().find(|b| (b.2 - 8.8).abs() < 0.01);
    let top = swatch(&chart("top")).expect("a swatch");
    let right = swatch(&chart("right")).expect("a swatch");
    let highest_bar = plot_bars(&chart("top")).iter().map(|b| b.1).fold(f32::MAX, f32::min);
    assert!(top.1 < highest_bar, "a top legend is above the bars");
    assert!(right.0 > 400.0, "a right legend is at the right: {right:?}");
    assert!(swatch(&chart("none")).is_none());
}

#[test]
fn the_palette_starts_over_past_the_eighth_series() {
    let series: String = (1..=9).map(|n| format!(r#"<series label="S{n}"/>"#)).collect();
    let values: String = (1..=9).map(|n| format!("<value>{n}</value>")).collect();
    let pages = laid_out(&format!(r#"<chart type="bar" height="120pt" legend="none">{series}<category>{values}</category></chart>"#));
    let mut bars = plot_bars(&pages);
    bars.sort_by(|a, b| a.0.total_cmp(&b.0));
    let colors: Vec<String> = bars.into_iter().map(|b| b.4).collect();
    assert_eq!(colors[..8], PALETTE.map(str::to_string));
    assert_eq!(colors[8], PALETTE[0]);
}

#[test]
fn a_color_of_a_series_or_of_the_category_of_a_chart_of_one_series_colors_its_bars() {
    let pages = laid_out(r##"<chart type="bar" height="120pt"><series color="#ff0000"/><category><value>1</value></category>
        <category color="#00ff00"><value>2</value></category></chart>"##);
    let colors: Vec<String> = plot_bars(&pages).into_iter().map(|b| b.4).collect();
    assert_eq!(colors, ["#ff0000", "#00ff00"]);
}

#[test]
fn stacked_bars_stack_and_the_axis_runs_to_the_largest_total() {
    let pages = laid_out(r#"<chart type="bar" stacked="true" height="160pt" value-labels="true" legend="none">
        <series/><series/><category><value>52000</value><value>41000</value></category></chart>"#);
    let bars = plot_bars(&pages);
    assert_eq!(bars.len(), 2);
    assert!((bars[0].1 - (bars[1].1 + bars[1].3)).abs() < 0.01, "the second sits on the first: {bars:?}");
    let words = texts(&pages);
    assert!(words.contains(&"100,000".to_string()) && words.contains(&"93,000".to_string()), "{words:?}");
}

#[test]
fn a_negative_value_has_a_zero_line_and_its_label_on_its_side() {
    let pages = laid_out(r#"<chart type="bar" direction="horizontal" height="120pt" suffix="%" decimals="2" value-labels="true">
        <category label="xs"><value>0.12</value></category><category label="svg"><value>-2.3</value></category></chart>"#);
    let words = texts(&pages);
    assert!(words.contains(&"0.12%".to_string()) && words.contains(&"-2.30%".to_string()), "{words:?}");
    let zero_lines = all(&pages).into_iter().filter(|n| matches!(n, RenderNode::Line(l) if (l.thickness - ZERO_LINE).abs() < 0.01)).count();
    assert_eq!(zero_lines, 1);
    let bars = plot_bars(&pages);
    assert!(bars[1].0 + bars[1].2 <= bars[0].0 + 0.01, "the negative bar ends where the positive one starts: {bars:?}");
}

#[test]
fn value_labels_keep_the_decimals_a_value_was_written_with() {
    let words = texts(&laid_out(r#"<chart type="bar" height="120pt" value-labels="true"><category><value>4.10</value></category>
        <category><value>1234</value></category></chart>"#));
    assert!(words.contains(&"4.10".to_string()) && words.contains(&"1,234".to_string()), "{words:?}");
}

#[test]
fn a_label_wider_than_its_room_wraps_and_then_ends_in_an_ellipsis() {
    let long = "A rather long category name that has to wrap onto more lines than a label takes";
    let words = texts(&laid_out(&format!(r#"<chart type="bar" height="160pt" width="200pt"><category label="{long}"><value>1</value></category>
        <category label="B"><value>2</value></category><category label="C"><value>3</value></category></chart>"#)));
    let first = words.iter().position(|w| w.starts_with("A rather")).expect("the label's first line");
    let lines = &words[first..first + LABEL_LINES];
    assert!(lines.iter().all(|line| long.contains(line.trim_end_matches(ELLIPSIS))), "{lines:?}");
    assert!(lines[LABEL_LINES - 1].ends_with(ELLIPSIS), "{lines:?}");
    assert_eq!(words[first + LABEL_LINES], "B", "{words:?}");
}

#[test]
fn the_gaps_set_the_room_between_bars_and_categories() {
    let chart = |gaps: &str| plot_bars(&laid_out(&format!(r#"<chart type="bar" height="120pt" width="300pt" {gaps}>
        <series/><series/><category><value>1</value><value>2</value></category><category><value>3</value><value>4</value></category></chart>"#)));
    let mut bars = chart(r#"bar-gap="10pt" category-gap="20pt""#);
    bars.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert!((bars[1].0 - (bars[0].0 + bars[0].2) - 10.0).abs() < 0.01, "{bars:?}");
    assert!((bars[2].0 - (bars[1].0 + bars[1].2) - 20.0).abs() < 0.01, "{bars:?}");
}

#[test]
fn a_chart_is_as_tall_as_its_height_and_fills_its_width() {
    let pages = laid_out(r#"<chart type="bar" height="160pt"><category><value>1</value></category></chart>"#);
    let RenderNode::Box(chart) = &pages[0].nodes[0] else { panic!("a box") };
    assert_eq!((chart.height, chart.width), (160.0, 595.28 - 56.0));
}

#[test]
fn a_chart_that_does_not_fit_moves_whole_to_the_next_page() {
    let lines: String = (1..=56).map(|n| format!("<text>Line {n}</text>")).collect();
    let pages = laid_out(&format!("{lines}{REVENUE}"));
    assert_eq!(pages.len(), 2);
    assert!(texts(&pages[1..]).contains(&"Jan".to_string()));
    assert!(!texts(&pages[..1]).contains(&"$0".to_string()), "nothing of it stays on the first page");
}

#[test]
fn a_chart_is_a_figure_with_its_alt_in_a_tagged_document() {
    let xml = r#"<lpdf version="1"><document><meta title="T" lang="en"/><output pdfua="1"/><section><layout>
        <chart type="bar" height="100pt" alt="Revenue per month"><category><value>1</value></category></chart>
        </layout></section></document></lpdf>"#;
    let pages = pages(xml, None).unwrap_or_else(|e| panic!("{e}"));
    let RenderNode::Box(chart) = &pages[0].nodes[0] else { panic!("a box") };
    assert_eq!(chart.tag.as_ref().map(|t| &t.kind), Some(&TagKind::Figure("Revenue per month".into())));
}

#[test]
fn a_chart_without_alt_is_refused_in_a_pdf_ua_document() {
    let xml = r#"<lpdf version="1"><document><meta title="T" lang="en"/><output pdfua="1"/><section><layout>
        <chart type="bar" height="100pt"><category><value>1</value></category></chart></layout></section></document></lpdf>"#;
    let mut doc = crate::parse::parse(xml).unwrap();
    let layouts = doc.section_layouts();
    let error = crate::pdfua::check(doc.output.pdfua, &doc.meta, &layouts).unwrap_err();
    assert!(error.contains("a chart has no alt"), "{error}");
}

// ── Refused ───────────────────────────────────────────────────────────────────

#[test]
fn a_category_with_the_wrong_count_of_values_is_refused_where_it_is_written() {
    let error = error(r#"<chart type="bar" height="100pt"><series/><series/>
        <category label="Q1"><value>1</value></category></chart>"#);
    assert!(error.contains("<chart> at line 1") && error.contains("category 'Q1' has 1 value, and the chart has 2 series"), "{error}");
}

#[test]
fn a_value_that_is_not_a_plain_number_is_refused_where_it_is_written() {
    let error = error(r#"<chart type="bar" height="100pt"><category><value>41,200</value></category></chart>"#);
    assert!(error.contains("<value> at line 1") && error.contains("'41,200' is not a plain number"), "{error}");
}

#[test]
fn the_two_marks_the_same_are_refused() {
    let error = error(r#"<chart type="bar" height="100pt" thousands-mark="," decimal-mark=","/>"#);
    assert!(error.contains("thousands-mark and decimal-mark are both ','"), "{error}");
}

#[test]
fn a_mark_of_more_than_one_character_is_refused() {
    let error = error(r#"<chart type="bar" height="100pt" decimal-mark=".."/>"#);
    assert!(error.contains("invalid decimal-mark '..': expected one character"), "{error}");
}

#[test]
fn a_negative_value_in_a_stacked_chart_is_refused() {
    let error = error(r#"<chart type="bar" height="100pt" stacked="true"><category label="Q1"><value>-5</value></category></chart>"#);
    assert!(error.contains("category 'Q1' has the value -5, and a stacked chart can't hold a negative value"), "{error}");
}

#[test]
fn a_value_past_what_a_chart_holds_is_refused_with_the_limit() {
    let huge = format!("17{}", "0".repeat(307));
    let past = error(&format!(r#"<chart type="bar" height="100pt"><category label="Q1"><value>{huge}</value></category></chart>"#));
    assert!(past.contains("category 'Q1' has the value 1.7e308, and a chart holds values of 1,000,000,000,000,000 at \
        most, either side of zero"), "{past}");
    let negative = error(r#"<chart type="bar" height="100pt"><category><value>-1000000000000001</value></category></chart>"#);
    assert!(negative.contains("category 1 has the value -1.000000000000001e15"), "{negative}");
    let at_the_limit = r#"<chart type="bar" height="100pt" value-labels="true"><category><value>1000000000000000</value></category></chart>"#;
    assert!(texts(&laid_out(at_the_limit)).contains(&"1,000,000,000,000,000".to_string()));
}

#[test]
fn a_value_of_the_data_past_what_a_chart_holds_is_refused() {
    let xml = document(r#"<chart type="bar" height="120pt"><category label="Q1"><value data-value="amount">0</value></category></chart>"#);
    let error = pages(&xml, Some(r#"{ "amount": 1.7e308 }"#)).map(|_| ()).expect_err("the document is refused");
    assert!(error.contains("category 'Q1' has the value 1.7e308"), "{error}");
}

#[test]
fn a_stack_whose_total_is_past_what_a_chart_holds_is_refused() {
    let values: String = (0..20).map(|_| "<value>100000000000000</value>").collect();
    let series: String = (0..20).map(|_| "<series/>").collect();
    let error = error(&format!(r#"<chart type="bar" height="100pt" stacked="true">{series}<category label="Q1">{values}</category></chart>"#));
    assert!(error.contains("the values of category 'Q1' total 2e15, and a chart holds values of 1,000,000,000,000,000"), "{error}");
}

#[test]
fn a_min_or_a_max_past_what_a_chart_holds_is_refused() {
    let error = error(r#"<chart type="bar" height="100pt" max="10000000000000000"><category><value>1</value></category></chart>"#);
    assert!(error.contains("max '1e16' is past what a chart holds"), "{error}");
}

/// Every text and rect the pages draw lies between `top` and `bottom`.
fn drawn_between(pages: &[RenderPage], top: f32, bottom: f32) -> bool {
    let texts = all(pages).into_iter().filter_map(|n| match n { RenderNode::Text(t) => Some(t.y), _ => None });
    let rects = bars(pages).into_iter().map(|b| b.1);
    texts.chain(rects).all(|y| y >= top - 0.01 && y <= bottom + 0.01)
}

#[test]
fn a_legend_keeps_to_the_rows_that_fit_and_ends_in_an_ellipsis() {
    let series: String = (1..=60).map(|n| format!(r#"<series label="Series {n}"/>"#)).collect();
    let values: String = (1..=60).map(|_| "<value>1</value>").collect();
    let pages = laid_out(&format!(r#"<chart type="bar" height="80pt">{series}<category>{values}</category></chart>"#));
    // The chart's box starts at the layout's margin.
    assert!(drawn_between(&pages, 28.0, 28.0 + 80.0), "the legend stays in the chart's box");
    let words = texts(&pages);
    assert!(words.contains(&"Series 1".to_string()) && !words.contains(&"Series 60".to_string()), "{words:?}");
    assert_eq!(words.iter().filter(|w| *w == ELLIPSIS).count(), 1, "{words:?}");
}

#[test]
fn a_legend_at_the_right_keeps_to_the_lines_that_fit() {
    let series: String = (1..=30).map(|n| format!(r#"<series label="S{n}"/>"#)).collect();
    let values: String = (1..=30).map(|_| "<value>1</value>").collect();
    let pages = laid_out(&format!(r#"<chart type="bar" height="100pt" legend="right">{series}<category>{values}</category></chart>"#));
    assert!(drawn_between(&pages, 28.0, 28.0 + 100.0), "the legend stays in the chart's box");
    let words = texts(&pages);
    assert!(words.contains(&"S1".to_string()) && !words.contains(&"S30".to_string()), "{words:?}");
    assert_eq!(words.iter().filter(|w| *w == ELLIPSIS).count(), 1, "{words:?}");
}

#[test]
fn a_horizontal_chart_whose_categories_are_thinner_than_a_line_writes_no_labels() {
    let categories: String = (1..=40).map(|n| format!(r#"<category label="Item {n}"><value>{n}</value></category>"#)).collect();
    let pages = laid_out(&format!(r#"<chart type="bar" direction="horizontal" height="120pt">{categories}</chart>"#));
    let words = texts(&pages);
    assert!(!words.iter().any(|w| w.starts_with("Item")), "{words:?}");
    assert_eq!(plot_bars(&pages).len(), 40);
    assert!(drawn_between(&pages, 28.0, 28.0 + 120.0));
}

#[test]
fn a_value_of_more_than_one_part_is_refused() {
    let tree = serde_json::json!({ "type": "chart", "attrs": { "type": "bar", "height": "100pt" }, "nodes": [
        { "type": "category", "attrs": {}, "nodes": [{ "type": "value", "attrs": {}, "nodes": ["120000", "95000"] }] },
    ]});
    let json = serde_json::json!({ "version": 1, "type": "document", "nodes": [{ "type": "section", "nodes": [{ "type": "layout", "nodes": [tree] }] }] });
    let Err(error) = crate::parse::parse_tree(&json.to_string()) else { panic!("refused") };
    assert!(error.contains("a value holds one number, and this one has 2: give each number a value of its own"), "{error}");
}

#[test]
fn a_chart_without_a_height_is_refused() {
    let error = error(r#"<chart type="bar"/>"#);
    assert!(error.contains("height"), "{error}");
}

#[test]
fn a_part_of_a_chart_outside_one_is_refused() {
    assert!(error("<series/>").contains("<series> at line 1, col"), "{}", error("<series/>"));
    assert!(error("<series/>").contains("belongs inside a <chart>"));
    let error = error(r#"<column><category><value>1</value></category></column>"#);
    assert!(error.contains("belongs inside a <chart>"), "{error}");
}

#[test]
fn a_series_after_a_category_is_refused() {
    let tree = serde_json::json!({ "type": "chart", "attrs": { "type": "bar", "height": "100pt" }, "nodes": [
        { "type": "category", "attrs": {}, "nodes": [{ "type": "value", "attrs": {}, "nodes": ["1"] }] },
        { "type": "series", "attrs": {} },
    ]});
    let json = serde_json::json!({ "version": 1, "type": "document", "nodes": [{ "type": "section", "nodes": [{ "type": "layout", "nodes": [tree] }] }] });
    let Err(error) = crate::parse::parse_tree(&json.to_string()) else { panic!("refused") };
    assert!(error.contains("a <series> goes before the categories"), "{error}");
}

#[test]
fn more_categories_than_a_chart_holds_are_refused() {
    let categories: String = (0..=MAX_CATEGORIES).map(|_| "<category><value>1</value></category>").collect();
    let error = error(&format!(r#"<chart type="bar" height="100pt">{categories}</chart>"#));
    assert!(error.contains("a chart holds 10,000 categories at most, and this one has 10,001"), "{error}");
}

// ── Data ──────────────────────────────────────────────────────────────────────

const FROM_DATA: &str = r#"<chart type="bar" height="180pt">
    <series data-source="builds" label="Build" data-label="name" data-color="color"/>
    <category data-source="fixtures" label="xs" data-label="name">
        <value data-source="counts" data-value="instructions">0</value>
    </category>
</chart>"#;

const BUILDS: &str = r##"{
    "builds": [{ "name": "v0.37.0", "color": "#111111" }, { "name": "branch", "color": "#222222" }],
    "fixtures": [
        { "name": "xs", "counts": [{ "instructions": 3781398 }, { "instructions": 3790112 }] },
        { "name": 2026, "counts": [{ "instructions": 9120455 }, { "instructions": null }] }
    ]
}"##;

#[test]
fn data_source_repeats_series_categories_and_values() {
    let pages = pages(&document(FROM_DATA), Some(BUILDS)).unwrap_or_else(|e| panic!("{e}"));
    let words = texts(&pages);
    for expected in ["v0.37.0", "branch", "xs", "2026"] {
        assert!(words.contains(&expected.to_string()), "{expected} in {words:?}");
    }
    let colors: Vec<String> = plot_bars(&pages).into_iter().map(|b| b.4).collect();
    assert_eq!(colors, ["#111111", "#111111", "#222222"], "null is no value, so no bar");
}

#[test]
fn without_data_a_chart_renders_as_written() {
    let pages = pages(&document(FROM_DATA), None).unwrap_or_else(|e| panic!("{e}"));
    let words = texts(&pages);
    assert!(words.contains(&"xs".to_string()), "{words:?}");
    assert!(plot_bars(&pages).is_empty(), "the one value is 0, which draws no bar");
}

#[test]
fn data_if_and_data_if_not_keep_or_drop_a_category() {
    let xml = document(r#"<chart type="bar" height="120pt">
        <category label="Kept" data-if="show"><value>1</value></category>
        <category label="Dropped" data-if-not="show"><value>2</value></category></chart>"#);
    let words = texts(&pages(&xml, Some(r#"{ "show": true }"#)).unwrap_or_else(|e| panic!("{e}")));
    assert!(words.contains(&"Kept".to_string()) && !words.contains(&"Dropped".to_string()), "{words:?}");
}

#[test]
fn a_value_of_the_data_that_is_not_a_number_is_refused_by_its_path() {
    let xml = document(r#"<chart type="bar" height="120pt"><category><value data-value="amount">0</value></category></chart>"#);
    let error = pages(&xml, Some(r#"{ "amount": "41,200" }"#)).map(|_| ()).expect_err("the document is refused");
    assert!(error.contains(r#"<value data-value="amount">"#) && error.contains("'41,200' is not a plain number"), "{error}");
}

#[test]
fn the_wrong_count_of_values_from_the_data_is_refused() {
    let data = r#"{ "builds": [{ "name": "a" }, { "name": "b" }], "fixtures": [{ "name": "xs", "counts": [{ "instructions": 1 }] }] }"#;
    let error = pages(&document(FROM_DATA), Some(data)).map(|_| ()).expect_err("the document is refused");
    assert!(error.contains("category 'xs' has 1 value, and the chart has 2 series"), "{error}");
}

#[test]
fn data_that_repeats_more_categories_or_series_than_a_chart_holds_is_refused_by_its_path() {
    let fixtures: Vec<serde_json::Value> =
        (0..=MAX_CATEGORIES).map(|_| serde_json::json!({ "counts": [{ "instructions": 1 }] })).collect();
    let data = serde_json::json!({ "builds": [{ "name": "a" }], "fixtures": fixtures }).to_string();
    let error = pages(&document(FROM_DATA), Some(&data)).map(|_| ()).expect_err("the document is refused");
    assert!(error.contains("a chart holds 10,000 categories at most; the data of 'fixtures' gives 10,001"), "{error}");

    let builds: Vec<serde_json::Value> = (0..=MAX_SERIES).map(|n| serde_json::json!({ "name": n.to_string() })).collect();
    let data = serde_json::json!({ "builds": builds, "fixtures": [] }).to_string();
    let error = pages(&document(FROM_DATA), Some(&data)).map(|_| ()).expect_err("the document is refused");
    assert!(error.contains("a chart holds 100 series at most; the data of 'builds' gives 101"), "{error}");
}

#[test]
fn data_label_on_an_element_that_has_no_label_is_refused() {
    let error = error(r#"<text data-label="name">A</text>"#);
    assert!(error.contains("'data-label' does not apply to this element"), "{error}");
}

// ── Limits ────────────────────────────────────────────────────────────────────

/// A chart of `categories` categories of `series` values each, bound from the data.
fn at_the_limits(categories: usize, series: usize) -> (String, String) {
    let xml = document(r#"<chart type="bar" height="400pt" value-labels="true">
        <series data-source="series" data-label="name"/>
        <category data-source="categories" data-label="name"><value data-source="values" data-value="v">0</value></category>
        </chart>"#);
    let values: Vec<serde_json::Value> = (0..series).map(|n| serde_json::json!({ "v": n as f64 * 1.5 })).collect();
    let data = serde_json::json!({
        "series": (0..series).map(|n| serde_json::json!({ "name": format!("Series {n}") })).collect::<Vec<_>>(),
        "categories": (0..categories).map(|n| serde_json::json!({ "name": format!("C{n}"), "values": values })).collect::<Vec<_>>(),
    });
    (xml, data.to_string())
}

#[test]
fn a_chart_at_the_limits_renders() {
    for (categories, series) in [(MAX_CATEGORIES, 1), (100, MAX_SERIES)] {
        let (xml, data) = at_the_limits(categories, series);
        let pages = pages(&xml, Some(&data)).unwrap_or_else(|e| panic!("{categories} by {series}: {e}"));
        let doc = crate::parse::parse(&xml).unwrap_or_else(|e| panic!("{e}"));
        let bytes = crate::pdf::render_pdf(
            &pages, &doc.fonts, &crate::pdf::FontRegistry::new(), &crate::pdf::ImageRegistry::new(), &doc.meta, doc.paints(),
            None, false, &doc.output, &[], None,
        )
        .unwrap_or_else(|e| panic!("{categories} by {series}: {e}"));
        assert!(bytes.starts_with(b"%PDF"));
    }
}
