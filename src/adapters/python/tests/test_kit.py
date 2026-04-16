import pytest

from lpdf import (
    stack, flank, split, cluster, grid, frame, link,
    text, span, divider, page, document,
    StackOptions, FlankOptions, GridOptions, TextOptions, SpanOptions,
    DividerOptions, PageOptions, DocumentOptions, LpdfMeta, LpdfTokens,
    LpdfSpanNode,
)


def test_stack_empty():
    node = stack()
    d = node.to_dict()
    assert d == {"type": "stack", "attrs": {}, "children": []}


def test_stack_with_options():
    node = stack(options=StackOptions(gap="10pt", padding="5pt"))
    d = node.to_dict()
    assert d["attrs"] == {"gap": "10pt", "padding": "5pt"}


def test_snake_to_kebab_conversion():
    node = text(["hello"], options=TextOptions(font_size="12pt", text_align="center"))
    d = node.to_dict()
    assert d["attrs"] == {"font-size": "12pt", "text-align": "center"}


def test_grid_col_width():
    node = grid(options=GridOptions(cols="3", col_width="100pt"))
    d = node.to_dict()
    assert d["attrs"] == {"cols": "3", "col-width": "100pt"}


def test_container_types():
    for builder, name in [
        (stack, "stack"), (flank, "flank"), (split, "split"),
        (cluster, "cluster"), (grid, "grid"), (frame, "frame"), (link, "link"),
    ]:
        d = builder().to_dict()
        assert d["type"] == name


def test_text_with_string_children():
    node = text(["Hello", " world"])
    d = node.to_dict()
    assert d == {"type": "text", "attrs": {}, "children": ["Hello", " world"]}


def test_text_with_span_children():
    s = span(["bold text"], options=SpanOptions(bold="true"))
    node = text(["Normal ", s])
    d = node.to_dict()
    assert d["children"][0] == "Normal "
    assert d["children"][1] == {"type": "span", "attrs": {"bold": "true"}, "children": ["bold text"]}


def test_text_rejects_invalid_children():
    with pytest.raises(TypeError, match="text\\(\\) child at index 0"):
        text([123])


def test_span_rejects_invalid_children():
    with pytest.raises(TypeError, match="span\\(\\) child at index 0"):
        span([123])


def test_divider():
    node = divider(options=DividerOptions(color="red", thickness="2pt"))
    d = node.to_dict()
    assert d == {"type": "divider", "attrs": {"color": "red", "thickness": "2pt"}}


def test_page():
    node = page(
        nodes=[text(["Hello"])],
        options=PageOptions(size="a4", margin="28pt"),
    )
    d = node.to_dict()
    assert d["type"] == "page"
    assert d["attrs"] == {"size": "a4", "margin": "28pt"}
    assert len(d["children"]) == 1


def test_document_includes_version():
    doc = document()
    d = doc.to_dict()
    assert d["version"] == 1
    assert d["type"] == "document"


def test_document_with_meta_and_tokens():
    doc = document(
        options=DocumentOptions(
            size="a4",
            meta=LpdfMeta(title="Test", author="Me"),
            tokens=LpdfTokens(colors={"primary": "#000"}),
        ),
    )
    d = doc.to_dict()
    assert d["attrs"]["size"] == "a4"
    assert d["attrs"]["meta"] == {"title": "Test", "author": "Me"}
    assert d["attrs"]["tokens"] == {"colors": {"primary": "#000"}}


def test_none_options_skipped():
    node = text(["hi"], options=TextOptions(font="Arial"))
    d = node.to_dict()
    assert d["attrs"] == {"font": "Arial"}
    assert "font-size" not in d["attrs"]
