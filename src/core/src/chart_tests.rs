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

/// A chart of `categories` categories of `series` values each, bound from the data, with `attrs` on it.
fn at_the_limits(attrs: &str, categories: usize, series: usize) -> (String, String) {
    let xml = document(&format!(r#"<chart {attrs} height="400pt" value-labels="true">
        <series data-source="series" data-label="name"/>
        <category data-source="categories" data-label="name"><value data-source="values" data-value="v">0</value></category>
        </chart>"#));
    let values: Vec<serde_json::Value> = (0..series).map(|n| serde_json::json!({ "v": n as f64 * 1.5 })).collect();
    let data = serde_json::json!({
        "series": (0..series).map(|n| serde_json::json!({ "name": format!("Series {n}") })).collect::<Vec<_>>(),
        "categories": (0..categories).map(|n| serde_json::json!({ "name": format!("C{n}"), "values": values })).collect::<Vec<_>>(),
    });
    (xml, data.to_string())
}

#[test]
fn a_chart_at_the_limits_renders() {
    let kinds = [r#"type="bar""#, r#"type="line""#, r#"type="area" stacked="true""#];
    for attrs in kinds {
        for (categories, series) in [(MAX_CATEGORIES, 1), (100, MAX_SERIES)] {
            let (xml, data) = at_the_limits(attrs, categories, series);
            let pages = pages(&xml, Some(&data)).unwrap_or_else(|e| panic!("{attrs}, {categories} by {series}: {e}"));
            let doc = crate::parse::parse(&xml).unwrap_or_else(|e| panic!("{e}"));
            let bytes = crate::pdf::render_pdf(
                &pages, &doc.fonts, &crate::pdf::FontRegistry::new(), &crate::pdf::ImageRegistry::new(), &doc.meta,
                doc.paints(), None, false, &doc.output, &[], None,
            )
            .unwrap_or_else(|e| panic!("{attrs}, {categories} by {series}: {e}"));
            assert!(bytes.starts_with(b"%PDF"));
        }
    }
}


// ── Lines, areas and sparklines ───────────────────────────────────────────────

/// The lines and the areas the pages draw, in the order they are drawn: how each is painted, its color and its runs.
fn traces(pages: &[RenderPage]) -> Vec<(PolylinePaint, String, Vec<Vec<[f32; 2]>>)> {
    all(pages)
        .into_iter()
        .filter_map(|n| match n {
            RenderNode::Polylines(p) => Some((p.paint, p.color.clone(), p.runs.clone())),
            _ => None,
        })
        .collect()
}

fn lines_of(pages: &[RenderPage]) -> Vec<(f32, String, Vec<Vec<[f32; 2]>>)> {
    traces(pages)
        .into_iter()
        .filter_map(|(paint, color, runs)| match paint {
            PolylinePaint::Stroke { width } => Some((width, color, runs)),
            PolylinePaint::Fill { .. } => None,
        })
        .collect()
}

fn areas_of(pages: &[RenderPage]) -> Vec<(String, Vec<Vec<[f32; 2]>>)> {
    traces(pages)
        .into_iter()
        .filter_map(|(paint, color, runs)| matches!(paint, PolylinePaint::Fill { .. }).then_some((color, runs)))
        .collect()
}

const VISITS: &str = r#"<chart type="line" height="160pt">
    <series label="This year" color="primary"/>
    <series label="Last year" color="text-muted"/>
    <category label="W1"><value>120</value><value>100</value></category>
    <category label="W2"><value>135</value><value>98</value></category>
    <category label="W3"><value>128</value><value/></category>
    <category label="W4"><value>150</value><value>110</value></category>
</chart>"#;

#[test]
fn a_line_axis_runs_a_round_step_under_its_lowest_value_and_one_over_its_highest() {
    let axis = Axis::fit_line(98.0, 104.0, None, None, 300.0, 10.0);
    assert_eq!((axis.lo, axis.hi, axis.step), (96.0, 106.0, 2.0));
}

#[test]
fn a_line_axis_does_not_cross_zero_where_its_values_do_not() {
    let counts = Axis::fit_line(0.0, 37.0, None, None, 300.0, 10.0);
    assert_eq!((counts.lo, counts.hi), (0.0, 40.0), "{counts:?}");
    let losses = Axis::fit_line(-37.0, -2.0, None, None, 300.0, 10.0);
    assert_eq!(losses.hi, 0.0, "{losses:?}");
    let both = Axis::fit_line(-3.0, 4.0, None, None, 300.0, 10.0);
    assert!(both.lo < -3.0 && both.hi > 4.0, "{both:?}");
}

#[test]
fn min_and_max_only_widen_a_line_axis_and_min_of_zero_brings_it_to_zero() {
    let axis = Axis::fit_line(98.0, 104.0, Some(0.0), None, 300.0, 10.0);
    assert_eq!(axis.lo, 0.0, "{axis:?}");
    let axis = Axis::fit_line(98.0, 104.0, Some(100.0), Some(102.0), 300.0, 10.0);
    assert_eq!((axis.lo, axis.hi), (96.0, 106.0), "min and max inside the values change nothing: {axis:?}");
    let words = texts(&laid_out(r#"<chart type="line" height="160pt" min="0"><category><value>98</value></category>
        <category><value>104</value></category></chart>"#));
    assert_eq!(words.first().map(String::as_str), Some("0"), "{words:?}");
}

#[test]
fn a_line_of_equal_values_runs_across_the_middle_of_its_axis() {
    let axis = Axis::fit_line(100.0, 100.0, None, None, 300.0, 10.0);
    assert!((axis.lo + axis.hi) / 2.0 == 100.0 && axis.lo < 100.0, "{axis:?}");
    let zero = Axis::fit_line(0.0, 0.0, None, None, 300.0, 10.0);
    assert_eq!((zero.lo, zero.hi), (0.0, 1.0));
}

#[test]
fn a_short_chart_with_values_either_side_of_zero_keeps_both_on_its_axis() {
    let pages = laid_out(r#"<chart type="bar" height="20pt"><category><value>-5</value></category><category><value>5</value></category></chart>"#);
    let words = texts(&pages);
    assert_eq!(words, ["-5", "0", "5"], "{words:?}");
    assert_eq!(plot_bars(&pages).len(), 2);
}

#[test]
fn a_line_chart_draws_a_line_for_each_series_through_the_middle_of_its_categories() {
    let pages = laid_out(VISITS);
    let lines = lines_of(&pages);
    assert_eq!(lines.len(), 3, "the two lines, and last year's value alone in W4 as a dot: {lines:?}");
    assert!(areas_of(&pages).is_empty());
    let this_year = &lines[0].2;
    assert_eq!(this_year.len(), 1);
    assert_eq!(this_year[0].len(), 4);
    // The points are evenly spaced, and the highest value is the highest point.
    let xs: Vec<f32> = this_year[0].iter().map(|p| p[0]).collect();
    assert!(((xs[1] - xs[0]) - (xs[3] - xs[2])).abs() < 0.01, "{xs:?}");
    assert!(this_year[0][3][1] < this_year[0][0][1], "150 is above 120");
    let words = texts(&pages);
    for expected in ["W1", "W4", "This year", "Last year"] {
        assert!(words.contains(&expected.to_string()), "{expected} in {words:?}");
    }
    // The axis fits the values, from 90 to 160, and doesn't start at zero.
    assert!(!words.contains(&"0".to_string()), "{words:?}");
}

/// The lengths of the runs of each line the pages draw, a line's dots after it.
fn run_lengths(pages: &[RenderPage]) -> Vec<Vec<usize>> {
    lines_of(pages).iter().map(|line| line.2.iter().map(Vec::len).collect()).collect()
}

#[test]
fn a_missing_value_breaks_a_line_and_a_value_alone_between_two_is_a_dot() {
    let pages = laid_out(VISITS);
    assert_eq!(run_lengths(&pages), [vec![4], vec![2], vec![1]]);
    let widths: Vec<f32> = lines_of(&pages).iter().map(|line| line.0).collect();
    assert_eq!(widths, [LINE_WIDTH, LINE_WIDTH, LINE_WIDTH * DOT_SCALE]);
}

#[test]
fn null_and_a_missing_key_in_the_data_break_a_line() {
    let xml = document(r#"<chart type="line" height="160pt">
        <category data-source="weeks" label="W1" data-label="week"><value data-value="visits">0</value></category></chart>"#);
    let data = r#"{ "weeks": [{ "week": "W1", "visits": 3 }, { "week": "W2", "visits": null }, { "week": "W3", "visits": 5 },
        { "week": "W4" }, { "week": "W5", "visits": 6 }, { "week": "W6", "visits": 7 }] }"#;
    let pages = pages(&xml, Some(data)).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(run_lengths(&pages), [vec![2], vec![1, 1]]);
}

#[test]
fn an_area_fills_each_run_down_to_zero_under_its_line() {
    let pages = laid_out(r#"<chart type="area" height="160pt"><category><value>98</value></category>
        <category><value>104</value></category><category><value/></category><category><value>100</value></category></chart>"#);
    let words = texts(&pages);
    assert_eq!(words.first().map(String::as_str), Some("0"), "an area's axis keeps zero: {words:?}");
    let areas = areas_of(&pages);
    let lines = lines_of(&pages);
    assert_eq!((areas.len(), lines.len()), (1, 2), "the area, its line, and the dot of 100 alone after the gap");
    let runs = &areas[0].1;
    assert_eq!(runs.len(), 2, "the missing value breaks the area too: {runs:?}");
    // Each run goes along its points, then down to zero and back: the last two points are on the zero line.
    let zero = runs[0][runs[0].len() - 1][1];
    assert!(runs.iter().all(|run| run[run.len() - 2][1] == zero && run[run.len() - 1][1] == zero), "{runs:?}");
    assert!(runs[0][0][1] < zero);
    // The fill comes before the line, so the line is drawn over it.
    let order: Vec<bool> = traces(&pages).iter().map(|t| matches!(t.0, PolylinePaint::Fill { .. })).collect();
    assert_eq!(order, [true, false, false]);
}

#[test]
fn a_stacked_area_fills_each_series_down_to_the_one_under_it() {
    let pages = laid_out(r#"<chart type="area" stacked="true" height="160pt" legend="none"><series/><series/>
        <category><value>10</value><value>5</value></category>
        <category><value>20</value><value/></category>
        <category><value>15</value><value>10</value></category></chart>"#);
    let areas = areas_of(&pages);
    let lines = lines_of(&pages);
    assert_eq!((areas.len(), lines.len()), (2, 2));
    // The second band's top is the first band's top where its value is missing, which counts as zero.
    let first_top = &lines[0].2[0];
    let second_top = &lines[1].2[0];
    assert_eq!(second_top.len(), 3, "a stacked area doesn't break: {second_top:?}");
    assert_eq!(second_top[1], first_top[1]);
    assert!(second_top[0][1] < first_top[0][1]);
    // The second band runs along its top, then back along the first band's top.
    let band = &areas[1].1[0];
    let back: Vec<[f32; 2]> = first_top.iter().rev().copied().collect();
    assert_eq!(&band[3..], &back[..]);
    // The first band runs back along zero.
    let zero = areas[0].1[0][3][1];
    assert!(areas[0].1[0][3..].iter().all(|p| p[1] == zero));
}

#[test]
fn a_line_writes_every_label_that_fits_and_else_every_2nd_3rd_or_5th() {
    let days: String = (1..=30).map(|d| format!(r#"<category label="Nov {d}"><value>{}</value></category>"#, d % 7)).collect();
    let words = texts(&laid_out(&format!(r#"<chart type="line" height="160pt" width="300pt">{days}</chart>"#)));
    let labels: Vec<&String> = words.iter().filter(|w| w.starts_with("Nov")).collect();
    assert_eq!(labels, ["Nov 1", "Nov 6", "Nov 11", "Nov 16", "Nov 21", "Nov 26"], "{words:?}");
    let points = lines_of(&laid_out(&format!(r#"<chart type="line" height="160pt" width="300pt">{days}</chart>"#)));
    assert_eq!(points[0].2[0].len(), 30, "every point keeps its place");

    let few = texts(&laid_out(r#"<chart type="line" height="160pt" width="300pt"><category label="Jan"><value>1</value></category>
        <category label="Feb"><value>2</value></category><category label="Mar"><value>3</value></category></chart>"#));
    assert!(["Jan", "Feb", "Mar"].iter().all(|m| few.contains(&m.to_string())), "{few:?}");
}

#[test]
fn a_line_label_too_wide_on_its_own_wraps_and_ends_in_an_ellipsis() {
    let long = "A rather long category name that has to wrap onto more lines than a label takes";
    let categories: String = (0..12).map(|n| format!(r#"<category label="{long}"><value>{n}</value></category>"#)).collect();
    let words = texts(&laid_out(&format!(r#"<chart type="line" height="200pt" width="300pt">{categories}</chart>"#)));
    let first = words.iter().position(|w| w.starts_with("A rather")).expect("the first label");
    assert!(words[first + LABEL_LINES - 1].ends_with(ELLIPSIS), "{words:?}");
}

#[test]
fn the_labels_of_a_line_stay_inside_the_chart() {
    let categories: String = (1..=10).map(|n| format!(r#"<category label="Week {n} of the year"><value>{n}</value></category>"#)).collect();
    let pages = laid_out(&format!(r#"<chart type="line" height="160pt" width="200pt">{categories}</chart>"#));
    let left = 28.0;
    let right = 28.0 + 200.0;
    for node in all(&pages) {
        if let RenderNode::Text(t) = node {
            let w = text_width(&t.font, &t.content, t.size);
            let (from, to) = match t.text_align.as_str() {
                "center" => (t.x - w / 2.0, t.x + w / 2.0),
                "right" => (t.x - w, t.x),
                _ => (t.x, t.x + w),
            };
            assert!(from >= left - 0.01 && to <= right + 0.01, "'{}' from {from} to {to}", t.content);
        }
    }
}

#[test]
fn a_value_label_goes_under_a_point_lower_than_the_points_beside_it() {
    let pages = laid_out(r#"<chart type="line" height="150pt" prefix="$" decimals="2" value-labels="true">
        <category label="Mon"><value>98.40</value></category>
        <category label="Tue"><value>101.15</value></category>
        <category label="Wed"><value>99.80</value></category>
        <category label="Thu"><value>103.95</value></category>
        <category label="Fri"><value>102.30</value></category></chart>"#);
    let points = &lines_of(&pages)[0].2[0];
    let label_y = |content: &str| {
        all(&pages).into_iter().find_map(|n| match n {
            RenderNode::Text(t) if t.content == content => Some(t.y),
            _ => None,
        }).unwrap_or_else(|| panic!("no label {content}"))
    };
    let under = ["$98.40", "$99.80", "$102.30"].map(|label| label_y(label));
    for (label_top, point) in under.iter().zip([points[0], points[2], points[4]]) {
        assert!(*label_top > point[1], "a low point, the first and the last among them, has its label under it");
    }
    for (label, point) in [("$101.15", points[1]), ("$103.95", points[3])] {
        assert!(label_y(label) < point[1], "{label} is over its point");
    }
}

#[test]
fn a_low_point_keeps_its_label_over_it_where_under_would_cover_another_series_label() {
    let pages = laid_out(r#"<chart type="line" height="150pt" suffix="%" decimals="1" value-labels="true">
        <series label="North"/><series label="South"/>
        <category label="Q2"><value>3.1</value><value>-3.25</value></category>
        <category label="Q3"><value>-0.6</value><value>-2</value></category>
        <category label="Q4"><value>1.9</value><value>0.5</value></category></chart>"#);
    let label_y = |content: &str| {
        all(&pages).into_iter().find_map(|n| match n {
            RenderNode::Text(t) if t.content == content => Some(t.y),
            _ => None,
        }).unwrap_or_else(|| panic!("no label {content}"))
    };
    let north = &lines_of(&pages)[0].2[0];
    assert!(label_y("-0.6%") < north[1][1], "under its point it would cover South's -2.0%, so it is over it");
}

#[test]
fn a_low_point_at_the_bottom_of_the_plot_has_its_label_under_the_plot() {
    let pages = laid_out(r#"<chart type="area" height="150pt" value-labels="true" legend="none">
        <category label="A"><value>4</value></category>
        <category label="B"><value>0</value></category>
        <category label="C"><value>3</value></category></chart>"#);
    let label = |content: &str| {
        all(&pages).into_iter().find_map(|n| match n {
            RenderNode::Text(t) if t.content == content && t.text_align == "center" => Some((t.y, t.size)),
            _ => None,
        }).unwrap_or_else(|| panic!("no label {content}"))
    };
    let (zero, size) = label("0");
    let bottom = areas_of(&pages)[0].1[0].iter().map(|p| p[1]).fold(f32::MIN, f32::max);
    assert!(zero > bottom, "the label of a low point on the plot's bottom is under it");
    assert!(zero + size <= label("B").0, "and over the category's label");
}

#[test]
fn a_chart_with_no_low_point_keeps_no_room_under_its_plot() {
    let flat = r#"<chart type="line" height="150pt" value-labels="true">
        <category label="A"><value>2</value></category><category label="B"><value>2</value></category></chart>"#;
    // The lowest gridline is the plot's bottom.
    let plot_bottom = |xml: &str| {
        all(&laid_out(xml)).into_iter().filter_map(|n| match n {
            RenderNode::Line(l) => Some(l.y1),
            _ => None,
        }).fold(f32::MIN, f32::max)
    };
    let without = plot_bottom(&flat.replace(r#" value-labels="true""#, ""));
    assert!((plot_bottom(flat) - without).abs() < 0.01, "the plot reaches as low with its labels as without");
    let dipping = flat.replacen("<value>2</value>", "<value>3</value>", 1);
    assert!(plot_bottom(&dipping) < without - 1.0, "a low point's label takes a line of room under the plot");
}

#[test]
fn a_plot_moved_right_by_wider_steps_still_ends_at_the_right_edge() {
    // At this size the steps the plot is laid out for are whole numbers, and the last axis's write halves: wider.
    let pages = laid_out(r#"<chart type="bar" width="340pt" height="124pt">
        <category label="Northern region"><value>1.2</value></category>
        <category label="South"><value>2.6</value></category>
        <category label="East"><value>2.0</value></category>
        <category label="West"><value>0.7</value></category></chart>"#);
    let right = 28.0 + 340.0;
    for node in all(&pages) {
        if let RenderNode::Line(l) = node {
            assert!(l.x2 <= right + 0.01, "a gridline ends at {} past {right}", l.x2);
        }
    }
    let (x, _, w, _, _) = *plot_bars(&pages).last().expect("bars");
    assert!(x + w <= right + 0.01, "the last bar ends at {} past {right}", x + w);
}

#[test]
fn value_labels_are_over_the_points_and_a_stacked_area_writes_its_totals() {
    let line = texts(&laid_out(r#"<chart type="line" height="160pt" value-labels="true" decimals="1">
        <category><value>4</value></category><category><value>6.25</value></category></chart>"#));
    assert!(line.contains(&"4.0".to_string()) && line.contains(&"6.3".to_string()), "{line:?}");
    let stacked = texts(&laid_out(r#"<chart type="area" stacked="true" height="160pt" value-labels="true" legend="none">
        <series/><series/><category><value>10</value><value>7</value></category><category><value>21</value><value/></category></chart>"#));
    assert!(stacked.contains(&"17".to_string()) && stacked.contains(&"21".to_string()), "{stacked:?}");
    assert!(!stacked.contains(&"7".to_string()), "a stack writes its total only: {stacked:?}");
}

#[test]
fn stroke_width_sets_the_width_of_every_line() {
    let widths = |xml: &str| lines_of(&laid_out(xml)).into_iter().map(|l| l.0).collect::<Vec<_>>();
    assert_eq!(widths(VISITS), [LINE_WIDTH, LINE_WIDTH, LINE_WIDTH * DOT_SCALE]);
    assert_eq!(widths(&VISITS.replace(r#"type="line""#, r#"type="line" stroke-width="3pt""#)), [3.0, 3.0, 3.0 * DOT_SCALE]);
    let area = r#"<chart type="area" height="160pt" stroke-width="0"><category><value>1</value></category><category><value>2</value></category></chart>"#;
    let pages = laid_out(area);
    assert!(lines_of(&pages).is_empty() && areas_of(&pages).len() == 1, "zero draws no line, and the area still fills");
}

const SPARKLINE: &str = r#"<chart type="sparkline" height="14pt" width="80pt">
    <category><value>5</value></category><category><value>9</value></category>
    <category><value>2</value></category><category><value>7</value></category>
</chart>"#;

#[test]
fn a_sparkline_draws_one_line_across_its_box_and_no_axis_labels_or_legend() {
    let pages = laid_out(SPARKLINE);
    assert!(texts(&pages).is_empty());
    assert!(all(&pages).iter().all(|n| !matches!(n, RenderNode::Line(_))), "no gridlines");
    let lines = lines_of(&pages);
    assert_eq!(lines.len(), 1);
    let (width, color, runs) = &lines[0];
    assert_eq!((*width, color.as_str()), (SPARKLINE_WIDTH, PALETTE[0]));
    let points = &runs[0];
    let inset = SPARKLINE_WIDTH / 2.0;
    assert!((points[0][0] - (28.0 + inset)).abs() < 0.01 && (points[3][0] - (28.0 + 80.0 - inset)).abs() < 0.01, "{points:?}");
    // The highest value at the top of the box, the lowest at the bottom.
    assert!((points[1][1] - (28.0 + inset)).abs() < 0.01 && (points[2][1] - (28.0 + 14.0 - inset)).abs() < 0.01, "{points:?}");
}

#[test]
fn min_and_max_widen_a_sparkline() {
    let pages = laid_out(&SPARKLINE.replace(r#"width="80pt""#, r#"width="80pt" min="0" max="18""#));
    let points = &lines_of(&pages)[0].2[0];
    // 9, the highest, is halfway up from 0 to 18.
    assert!((points[1][1] - (28.0 + 7.0)).abs() < 0.01, "{points:?}");
}

#[test]
fn a_sparkline_takes_its_own_color_and_not_the_one_around_it() {
    let around = laid_out(&format!(r##"<column color="#ff0000">{SPARKLINE}</column>"##));
    assert_eq!(lines_of(&around)[0].1, PALETTE[0]);
    let own = laid_out(&SPARKLINE.replace(r#"width="80pt""#, r##"width="80pt" color="#00aa00""##));
    assert_eq!(lines_of(&own)[0].1, "#00aa00");
}

#[test]
fn a_sparkline_in_a_table_reads_its_values_from_each_row() {
    let xml = document(r#"<table cols="2fr 80pt"><tr data-source="products"><td><text data-value="name">Product</text></td>
        <td><chart type="sparkline" height="14pt"><category data-source="weeks"><value data-value="sales">0</value></category></chart></td>
        </tr></table>"#);
    let data = r#"{ "products": [
        { "name": "Lamp", "weeks": [{ "sales": 3 }, { "sales": 5 }, { "sales": 4 }] },
        { "name": "Desk", "weeks": [{ "sales": 9 }, { "sales": null }, { "sales": 2 }, { "sales": 6 }] }
    ] }"#;
    let pages = pages(&xml, Some(data)).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(run_lengths(&pages), [vec![3], vec![2], vec![1]]);
}

#[test]
fn the_dots_of_a_sparkline_stay_inside_its_box() {
    let pages = laid_out(r#"<chart type="sparkline" height="14pt" width="80pt"><category><value>1</value></category>
        <category><value/></category><category><value>3</value></category><category><value/></category>
        <category><value>2</value></category></chart>"#);
    let lines = lines_of(&pages);
    assert_eq!(run_lengths(&pages), [vec![1, 1, 1]]);
    let dot = SPARKLINE_WIDTH * DOT_SCALE;
    assert_eq!(lines[0].0, dot);
    let points: Vec<[f32; 2]> = lines[0].2.iter().map(|run| run[0]).collect();
    assert!(points.iter().all(|[x, y]| *x >= 28.0 + dot / 2.0 - 0.01 && *x <= 28.0 + 80.0 - dot / 2.0 + 0.01
        && *y >= 28.0 + dot / 2.0 - 0.01 && *y <= 28.0 + 14.0 - dot / 2.0 + 0.01), "{points:?}");
}

#[test]
fn a_chart_of_a_type_it_does_not_have_is_refused() {
    let error = error(r#"<chart type="scatter" height="100pt"><category><value>1</value></category></chart>"#);
    assert!(error.contains("invalid chart type 'scatter': expected bar, line, area, pie, donut or sparkline"), "{error}");
}

#[test]
fn direction_on_a_line_an_area_or_a_sparkline_is_refused() {
    for (kind, named) in [("line", "a line chart"), ("area", "an area chart"), ("sparkline", "a sparkline")] {
        let error = error(&format!(r#"<chart type="{kind}" direction="vertical" height="100pt"><category><value>1</value></category></chart>"#));
        assert!(error.contains(&format!("direction is for a bar chart, and this is {named}")), "{error}");
    }
}

#[test]
fn stacked_on_a_line_or_a_sparkline_is_refused() {
    for (kind, named) in [("line", "a line chart"), ("sparkline", "a sparkline")] {
        let error = error(&format!(r#"<chart type="{kind}" stacked="false" height="100pt"><category><value>1</value></category></chart>"#));
        assert!(error.contains(&format!("stacked is for a bar or an area chart, and this is {named}")), "{error}");
    }
}

#[test]
fn a_series_in_a_sparkline_is_refused_where_it_is_written() {
    let xml = error(r#"<chart type="sparkline" height="14pt"><series color="primary"/><category><value>1</value></category></chart>"#);
    assert!(xml.contains("<chart> at line 1") && xml.contains("a sparkline has one series, and takes no <series>"), "{xml}");
    let tree = serde_json::json!({ "type": "chart", "attrs": { "type": "sparkline", "height": "14pt" }, "nodes": [
        { "type": "series", "attrs": {} },
    ]});
    let json = serde_json::json!({ "version": 1, "type": "document", "nodes": [{ "type": "section", "nodes": [{ "type": "layout", "nodes": [tree] }] }] });
    let Err(error) = crate::parse::parse_tree(&json.to_string()) else { panic!("refused") };
    assert!(error.contains("a sparkline has one series, and takes no <series>"), "{error}");
}

#[test]
fn a_negative_value_in_a_stacked_area_is_refused() {
    let error = error(r#"<chart type="area" height="100pt" stacked="true"><category label="Jan"><value>-5</value></category></chart>"#);
    assert!(error.contains("category 'Jan' has the value -5, and a stacked chart can't hold a negative value"), "{error}");
}

#[test]
fn a_negative_stroke_width_is_refused() {
    let error = error(r#"<chart type="line" height="100pt" stroke-width="-1pt"><category><value>1</value></category></chart>"#);
    assert!(error.contains("invalid stroke-width '-1pt': expected a length of zero or more"), "{error}");
}

// ── Pies and donuts ───────────────────────────────────────────────────────────

/// The slices the pages draw, one node for each color, in the order they are drawn.
fn sectors(pages: &[RenderPage]) -> Vec<&RenderSectors> {
    all(pages).into_iter().filter_map(|n| match n { RenderNode::Sectors(s) => Some(s), _ => None }).collect()
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

/// Where the expenses go: a chart 160pt square, so its circle is the whole of it when it has no legend.
fn expenses(kind: &str, attrs: &str) -> String {
    expenses_in(kind, "160pt", attrs)
}

/// The expenses in a chart `width` wide, 160pt high, whose legend has the room for each slice's label and share.
fn expenses_in(kind: &str, width: &str, attrs: &str) -> String {
    format!(r#"<chart type="{kind}" width="{width}" height="160pt" {attrs}>
        <category label="Rent"><value>2400</value></category>
        <category label="Salaries"><value>8100</value></category>
        <category label="Travel"><value>600</value></category>
    </chart>"#)
}

const TURN: f32 = std::f32::consts::TAU;

#[test]
fn a_pie_draws_a_slice_for_each_category_clockwise_from_the_top_as_wide_as_its_share() {
    let pages = laid_out(&expenses("pie", r#"legend="none""#));
    let sets = sectors(&pages);
    assert_eq!(sets.len(), 3, "a color each");
    let colors: Vec<&str> = sets.iter().map(|s| s.fill.as_str()).collect();
    assert_eq!(colors, &PALETTE[..3]);
    let slices: Vec<[f32; 2]> = sets.iter().flat_map(|s| s.slices.clone()).collect();
    assert!(near(slices[0][0], 0.0) && near(slices[2][1], TURN), "{slices:?}");
    assert!(near(slices[0][1], slices[1][0]) && near(slices[1][1], slices[2][0]), "each starts where the one before ends");
    for (slice, share) in slices.iter().zip([2400.0, 8100.0, 600.0]) {
        assert!(near((slice[1] - slice[0]) / TURN, share / 11100.0), "{slice:?}");
    }
    assert!(sets.iter().all(|s| near(s.outer, 80.0) && near(s.inner, 0.0)), "the largest circle the box holds");
    assert!(sets.iter().all(|s| near(s.center[0], 28.0 + 80.0) && near(s.center[1], 28.0 + 80.0)));
}

#[test]
fn a_pie_has_no_axis_and_a_slice_of_the_whole_is_a_turn() {
    let pages = laid_out(r#"<chart type="pie" width="100pt" height="100pt" legend="none"><category label="All"><value>5</value></category></chart>"#);
    assert!(texts(&pages).is_empty(), "no steps, no labels under the slices");
    let sets = sectors(&pages);
    assert_eq!(sets.len(), 1);
    assert!(near(sets[0].slices[0][0], 0.0) && near(sets[0].slices[0][1], TURN));
}

#[test]
fn the_circle_is_the_largest_the_plot_holds_and_is_centered_in_it() {
    let pages = laid_out(r#"<chart type="pie" width="300pt" height="120pt" legend="none"><category><value>1</value></category></chart>"#);
    let set = sectors(&pages)[0];
    assert!(near(set.outer, 60.0) && near(set.center[0], 28.0 + 150.0) && near(set.center[1], 28.0 + 60.0));
    let pages = laid_out(r#"<chart type="pie" width="100pt" height="120pt" legend="none"><category><value>1</value></category></chart>"#);
    assert!(near(sectors(&pages)[0].outer, 50.0));
}

#[test]
fn a_slice_takes_the_color_of_its_category_and_else_the_next_of_the_palette() {
    let pages = laid_out(r##"<chart type="pie" height="100pt" legend="none">
        <category><value>1</value></category>
        <category color="#112233"><value>1</value></category>
        <category><value>1</value></category>
    </chart>"##);
    let colors: Vec<&str> = sectors(&pages).iter().map(|s| s.fill.as_str()).collect();
    assert_eq!(colors, [PALETTE[0], "#112233", PALETTE[2]]);
}

#[test]
fn slices_of_one_color_are_one_node_and_the_palette_starts_over_past_the_eighth() {
    let categories: String = (0..10).map(|n| format!("<category><value>{}</value></category>", n + 1)).collect();
    let pages = laid_out(&format!(r#"<chart type="pie" height="100pt" legend="none">{categories}</chart>"#));
    let sets = sectors(&pages);
    assert_eq!(sets.len(), 8);
    assert_eq!(sets[0].slices.len(), 2, "the first and the ninth");
    assert_eq!(sets[1].slices.len(), 2, "the second and the tenth");
}

#[test]
fn the_legend_of_a_pie_is_at_the_right_and_lists_each_slice_with_its_color_and_share() {
    let pages = laid_out(&expenses_in("pie", "400pt", ""));
    assert_eq!(texts(&pages), ["Rent 22%", "Salaries 73%", "Travel 5%"]);
    let swatches: Vec<String> = bars(&pages).into_iter().map(|b| b.4).collect();
    assert_eq!(swatches, &PALETTE[..3]);
    let set = sectors(&pages)[0];
    let swatch_x = bars(&pages)[0].0;
    assert!(swatch_x >= set.center[0] + set.outer, "the legend is beside the circle, at the right");
    assert!(near(set.outer, 80.0), "the circle keeps the room the legend leaves it");
}

#[test]
fn legend_places_the_legend_of_a_pie_and_none_drops_it() {
    let none = laid_out(&expenses("pie", r#"legend="none""#));
    assert!(texts(&none).is_empty() && bars(&none).is_empty());
    let bottom = laid_out(&expenses_in("donut", "400pt", r#"legend="bottom""#));
    let set = sectors(&bottom)[0];
    assert!(bars(&bottom).iter().all(|b| b.1 >= set.center[1] + set.outer - 0.01), "under the circle");
    let top = laid_out(&expenses_in("donut", "400pt", r#"legend="top""#));
    let set = sectors(&top)[0];
    assert!(bars(&top).iter().all(|b| b.1 + b.3 <= set.center[1] - set.outer + 0.01), "over the circle");
    assert!(set.outer < 80.0, "the legend takes its room from the circle");
}

#[test]
fn a_share_is_a_whole_percent_unless_decimals_says_otherwise_and_takes_the_decimal_mark() {
    let pages = laid_out(&expenses_in("pie", "400pt", r#"decimals="1" thousands-mark="" decimal-mark=",""#));
    assert_eq!(texts(&pages), ["Rent 21,6%", "Salaries 73,0%", "Travel 5,4%"]);
}

#[test]
fn prefix_and_suffix_are_not_written_around_a_share() {
    let pages = laid_out(&expenses_in("pie", "400pt", r#"prefix="$" suffix=" USD""#));
    assert_eq!(texts(&pages), ["Rent 22%", "Salaries 73%", "Travel 5%"]);
}

#[test]
fn a_label_too_wide_for_the_legend_is_shortened_and_keeps_its_share() {
    let pages = laid_out(r#"<chart type="pie" width="240pt" height="100pt">
        <category label="A very long name for the only slice of this pie"><value>5</value></category></chart>"#);
    let words = texts(&pages);
    assert_eq!(words.len(), 1);
    assert!(words[0].starts_with("A ") && words[0].ends_with("... 100%"), "{words:?}");
}

#[test]
fn a_zero_or_missing_value_draws_no_slice() {
    let pages = laid_out(r#"<chart type="pie" width="300pt" height="100pt">
        <category label="A"><value>5</value></category>
        <category label="B"><value>0</value></category>
        <category label="C"><value/></category>
        <category label="D"><value>5</value></category>
    </chart>"#);
    assert_eq!(texts(&pages), ["A 50%", "B 0%", "C", "D 50%"], "a missing value has no share");
    let colors: Vec<&str> = sectors(&pages).iter().map(|s| s.fill.as_str()).collect();
    assert_eq!(colors, [PALETTE[0], PALETTE[3]]);
}

#[test]
fn a_donut_has_a_hole_of_six_tenths_of_its_size_unless_hole_says_otherwise() {
    let default = laid_out(&expenses("donut", r#"legend="none""#));
    assert!(sectors(&default).iter().all(|s| near(s.inner, 48.0) && near(s.outer, 80.0)));
    for (hole, inner) in [("0.1", 8.0), ("0.5", 40.0), ("0.8", 64.0), ("0.9", 72.0)] {
        let pages = laid_out(&expenses("donut", &format!(r#"legend="none" hole="{hole}""#)));
        assert!(sectors(&pages).iter().all(|s| near(s.inner, inner)), "{hole}");
    }
    let pie = laid_out(&expenses("pie", r#"legend="none""#));
    assert!(sectors(&pie).iter().all(|s| s.inner == 0.0));
}

#[test]
fn hole_text_is_written_as_given_in_the_middle_of_the_hole() {
    let pages = laid_out(&expenses("donut", r#"legend="none" hole-text="$11,100""#));
    let node = all(&pages).into_iter().find_map(|n| match n { RenderNode::Text(t) => Some(t), _ => None }).expect("a text");
    assert_eq!(node.content, "$11,100");
    assert_eq!(node.text_align, "center");
    assert!(near(node.x, 28.0 + 80.0), "centered on the hole");
    assert!(node.y > 28.0 + 80.0 - node.size && node.y < 28.0 + 80.0, "and on its middle: {}", node.y);
}

#[test]
fn a_hole_text_too_wide_for_the_hole_wraps_and_then_ends_in_an_ellipsis() {
    let words = "Total spent over the whole of the year, rent and salaries and travel and all the rest of it";
    let pages = laid_out(&expenses("donut", &format!(r#"legend="none" hole-text="{words}""#)));
    let lines = texts(&pages);
    assert!((2..=3).contains(&lines.len()), "{lines:?}");
    assert!(lines.last().is_some_and(|line| line.ends_with(ELLIPSIS)), "{lines:?}");
    let centers: Vec<f32> = all(&pages).into_iter().filter_map(|n| match n { RenderNode::Text(t) => Some(t.x), _ => None }).collect();
    assert!(centers.iter().all(|x| near(*x, 28.0 + 80.0)));
}

#[test]
fn a_hole_with_no_room_for_a_line_of_text_writes_none() {
    let pages = laid_out(&expenses("donut", r#"legend="none" hole="0.1" hole-text="Total""#));
    assert!(texts(&pages).is_empty());
}

#[test]
fn data_hole_text_sets_the_text_of_the_hole_from_the_data_and_hole_text_is_its_fallback() {
    let xml = document(r#"<chart type="donut" width="160pt" height="160pt" legend="none" hole-text="Total" data-hole-text="total">
        <category data-source="expenses" label="Rent" data-label="name"><value data-value="amount">1</value></category></chart>"#);
    let data = r#"{ "total": "$11,100", "expenses": [{ "name": "Rent", "amount": 2400 }, { "name": "Pay", "amount": 8100 }] }"#;
    assert_eq!(texts(&pages(&xml, Some(data)).unwrap_or_else(|e| panic!("{e}"))), ["$11,100"]);
    assert_eq!(texts(&pages(&xml, Some(r#"{ "expenses": [{ "name": "Rent", "amount": 2400 }] }"#)).unwrap_or_else(|e| panic!("{e}"))), ["Total"]);
    assert_eq!(texts(&pages(&xml, None).unwrap_or_else(|e| panic!("{e}"))), ["Total"]);
}

#[test]
fn a_data_hole_text_that_is_not_a_string_is_refused_by_its_path() {
    let xml = document(r#"<chart type="donut" height="100pt" data-hole-text="total"><category><value>1</value></category></chart>"#);
    let error = pages(&xml, Some(r#"{ "total": 11100 }"#)).map(|_| ()).expect_err("refused");
    assert!(error.contains(r#"data-hole-text="total": expected a string, found a number"#), "{error}");
}

#[test]
fn a_pie_and_a_donut_read_their_categories_from_the_data() {
    let xml = document(r##"<chart type="donut" width="400pt" height="160pt" hole-text="Total">
        <category data-source="expenses" label="Rent" data-label="name" color="#737373" data-color="color">
            <value data-value="amount">2400</value></category></chart>"##);
    let data = r##"{ "expenses": [
        { "name": "Rent", "amount": 2400, "color": "#1763cf" },
        { "name": "Salaries", "amount": 8100, "color": "#2f9e44" },
        { "name": "Travel", "amount": 600, "color": "#e8590c" } ] }"##;
    let pages = pages(&xml, Some(data)).unwrap_or_else(|e| panic!("{e}"));
    let colors: Vec<&str> = sectors(&pages).iter().map(|s| s.fill.as_str()).collect();
    assert_eq!(colors, ["#1763cf", "#2f9e44", "#e8590c"]);
    assert_eq!(texts(&pages), ["Rent 22%", "Salaries 73%", "Travel 5%", "Total"]);
}

#[test]
fn without_data_a_pie_renders_as_written() {
    let xml = document(r#"<chart type="pie" width="300pt" height="100pt">
        <category data-source="expenses" label="Rent" data-label="name"><value data-value="amount">2400</value></category></chart>"#);
    let pages = pages(&xml, None).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(texts(&pages), ["Rent 100%"]);
    assert_eq!(sectors(&pages).len(), 1);
}

#[test]
fn a_pie_moves_with_its_chart_to_the_next_page() {
    let lines: String = (1..=56).map(|n| format!("<text>Line {n}</text>")).collect();
    let pages = laid_out(&format!("{lines}{}", expenses("donut", r#"legend="none""#)));
    assert_eq!(pages.len(), 2);
    let set = sectors(&pages[1..])[0];
    assert!(near(set.center[1], 28.0 + 80.0), "{:?}", set.center);
}

#[test]
fn a_negative_value_in_a_pie_or_a_donut_is_refused_and_names_the_category() {
    for (kind, named) in [("pie", "a pie chart"), ("donut", "a donut chart")] {
        let error = error(&format!(r#"<chart type="{kind}" height="100pt"><category label="Rent"><value>5</value></category><category label="Refund"><value>-2</value></category></chart>"#));
        assert!(error.contains(&format!("category 'Refund' has the value -2, and {named} can't hold a negative value")), "{error}");
    }
}

#[test]
fn values_that_add_up_to_zero_are_refused() {
    for (kind, named) in [("pie", "a pie chart"), ("donut", "a donut chart")] {
        for values in ["<category><value>0</value></category>", "<category><value/></category>", ""] {
            let error = error(&format!(r#"<chart type="{kind}" height="100pt">{values}</chart>"#));
            assert!(error.contains(&format!("the values of {named} add up to zero")), "{error}");
        }
    }
    let xml = document(r#"<chart type="pie" height="100pt"><category data-source="rows"><value data-value="v">1</value></category></chart>"#);
    let error = pages(&xml, Some(r#"{ "rows": [{ "v": 0 }, { "v": 0 }] }"#)).map(|_| ()).expect_err("refused");
    assert!(error.contains("the values of a pie chart add up to zero"), "{error}");
}

#[test]
fn a_series_in_a_pie_or_a_donut_is_refused_where_it_is_written() {
    for (kind, named) in [("pie", "a pie chart"), ("donut", "a donut chart")] {
        let error = error(&format!(r#"<chart type="{kind}" height="100pt"><series label="One"/><category><value>1</value></category></chart>"#));
        assert!(error.contains(&format!("{named} has one series, and takes no <series>")), "{error}");
    }
}

#[test]
fn direction_and_stacked_on_a_pie_or_a_donut_are_refused() {
    for (kind, named) in [("pie", "a pie chart"), ("donut", "a donut chart")] {
        let error_direction = error(&format!(r#"<chart type="{kind}" direction="vertical" height="100pt"><category><value>1</value></category></chart>"#));
        assert!(error_direction.contains(&format!("direction is for a bar chart, and this is {named}")), "{error_direction}");
        let error_stacked = error(&format!(r#"<chart type="{kind}" stacked="true" height="100pt"><category><value>1</value></category></chart>"#));
        assert!(error_stacked.contains(&format!("stacked is for a bar or an area chart, and this is {named}")), "{error_stacked}");
    }
}

#[test]
fn hole_and_hole_text_on_any_kind_but_a_donut_are_refused() {
    for (kind, named) in [
        ("bar", "a bar chart"), ("line", "a line chart"), ("area", "an area chart"), ("pie", "a pie chart"),
        ("sparkline", "a sparkline"),
    ] {
        for attr in [r#"hole="0.5""#, r#"hole-text="Total""#, r#"data-hole-text="total""#] {
            let error = error(&format!(r#"<chart type="{kind}" {attr} height="100pt"><category><value>1</value></category></chart>"#));
            let name = attr.split('=').next().unwrap_or_default();
            assert!(error.contains(&format!("{name} is for a donut chart, and this is {named}")), "{attr}: {error}");
        }
    }
}

#[test]
fn a_hole_outside_one_tenth_to_nine_tenths_is_refused() {
    for hole in ["0.05", "0.95", "1", "0", "60%", "wide"] {
        let error = error(&format!(r#"<chart type="donut" hole="{hole}" height="100pt"><category><value>1</value></category></chart>"#));
        assert!(error.contains("hole must be a number from 0.1 to 0.9"), "{hole}: {error}");
    }
}

#[test]
fn a_pie_at_the_limit_of_categories_renders() {
    for kind in ["pie", "donut"] {
        let hole_text = if kind == "donut" { r#"hole-text="Total""# } else { "" };
        let xml = document(&format!(r#"<chart type="{kind}" height="400pt" {hole_text}>
            <category data-source="categories" data-label="name"><value data-value="v">1</value></category></chart>"#));
        let data = serde_json::json!({
            "categories": (0..MAX_CATEGORIES).map(|n| serde_json::json!({ "name": format!("C{n}"), "v": n + 1 })).collect::<Vec<_>>(),
        }).to_string();
        let pages = pages(&xml, Some(&data)).unwrap_or_else(|e| panic!("{kind}: {e}"));
        let doc = crate::parse::parse(&xml).unwrap_or_else(|e| panic!("{e}"));
        let bytes = crate::pdf::render_pdf(
            &pages, &doc.fonts, &crate::pdf::FontRegistry::new(), &crate::pdf::ImageRegistry::new(), &doc.meta,
            doc.paints(), None, false, &doc.output, &[], None,
        )
        .unwrap_or_else(|e| panic!("{kind}: {e}"));
        assert!(bytes.starts_with(b"%PDF"));
    }
}

#[test]
fn a_pie_in_a_tagged_document_is_a_figure_with_its_alt() {
    let xml = r#"<lpdf version="1"><document size="A4"><meta title="T" lang="en"/><output pdfua="1"/><section><layout>
        <chart type="donut" height="100pt" alt="Expenses by kind" hole-text="Total"><category label="Rent"><value>5</value></category></chart>
        </layout></section></document></lpdf>"#;
    let pages = pages(xml, None).unwrap_or_else(|e| panic!("{e}"));
    let RenderNode::Box(chart) = &pages[0].nodes[0] else { panic!("a box") };
    assert_eq!(chart.tag.as_ref().map(|t| &t.kind), Some(&TagKind::Figure("Expenses by kind".into())));
}

// ── Arcs ──────────────────────────────────────────────────────────────────────

fn curves(radius: f32, from: f32, to: f32) -> Vec<([f32; 2], [f32; 2], [f32; 2])> {
    let mut out = Vec::new();
    arc_curves(radius, from, to, |c1, c2, end| out.push((c1, c2, end)));
    out
}

#[test]
fn a_quarter_turn_is_one_bezier_with_the_handle_of_a_circle() {
    let [(c1, c2, end)] = curves(100.0, 0.0, std::f32::consts::FRAC_PI_2)[..] else { panic!("one curve") };
    // From the top, 12 o'clock, to the right, 3 o'clock, with y down from the center.
    assert!(near(c1[0], 55.23) && near(c1[1], -100.0), "{c1:?}");
    assert!(near(c2[0], 100.0) && near(c2[1], -55.23), "{c2:?}");
    assert!(near(end[0], 100.0) && near(end[1], 0.0), "{end:?}");
}

#[test]
fn an_arc_is_cut_into_curves_of_a_quarter_turn_at_most_and_ends_where_it_says() {
    assert_eq!(curves(10.0, 0.0, TURN).len(), 4);
    assert_eq!(curves(10.0, 0.5, 0.6).len(), 1);
    let three = curves(10.0, 0.0, TURN * 0.75);
    assert_eq!(three.len(), 3);
    let end = arc_point(10.0, TURN * 0.75);
    assert!(near(three[2].2[0], end[0]) && near(three[2].2[1], end[1]));
    let back = curves(10.0, TURN * 0.75, 0.0);
    assert_eq!(back.len(), 3, "an arc can run back, as the inner edge of a slice does");
    let start = arc_point(10.0, 0.0);
    assert!(near(back[2].2[0], start[0]) && near(back[2].2[1], start[1]));
    for (_, _, point) in three.iter().chain(back.iter()) {
        assert!((point[0].hypot(point[1]) - 10.0).abs() < 0.01, "{point:?} is on the circle");
    }
}

#[test]
fn the_sine_and_the_cosine_of_an_arc_agree_with_the_library_to_a_millionth() {
    for step in 0..=720 {
        let angle = step as f32 * TURN / 720.0;
        let (sin, cos) = sin_cos(angle);
        assert!((sin - f64::from(angle).sin()).abs() < 1e-6, "sin {angle}");
        assert!((cos - f64::from(angle).cos()).abs() < 1e-6, "cos {angle}");
    }
    let (sin, cos) = sin_cos(std::f32::consts::FRAC_PI_2);
    assert!((sin - 1.0).abs() < 1e-6 && cos.abs() < 1e-6, "a quarter turn");
}
