import pytest

from lpdf import (
    LpdfKit, LpdfLayout,
    StackOptions, FlankOptions, GridOptions, TextOptions, SpanOptions,
    DividerOptions, SectionOptions, DocumentOptions, DocumentMeta, DocumentTokens,
)


# ── LpdfLayout container nodes ────────────────────────────────────────────────

def test_stack_empty():
    node = LpdfLayout.stack()
    d = node.to_dict()
    assert d == {"type": "stack", "attrs": {}, "nodes": []}


def test_stack_with_options():
    node = LpdfLayout.stack(options=StackOptions(gap="10pt", padding="5pt"))
    d = node.to_dict()
    assert d["attrs"] == {"gap": "10pt", "padding": "5pt"}


def test_snake_to_kebab_conversion():
    node = LpdfLayout.text(["hello"], options=TextOptions(font_size="12pt", text_align="center"))
    d = node.to_dict()
    assert d["attrs"] == {"font-size": "12pt", "text-align": "center"}


def test_grid_col_width():
    node = LpdfLayout.grid(options=GridOptions(cols="3", col_width="100pt"))
    d = node.to_dict()
    assert d["attrs"] == {"cols": "3", "col-width": "100pt"}


def test_container_types():
    for method, name in [
        ("stack", "stack"), ("flank", "flank"), ("split", "split"),
        ("cluster", "cluster"), ("grid", "grid"), ("frame", "frame"), ("link", "link"),
    ]:
        d = getattr(LpdfLayout, method)().to_dict()
        assert d["type"] == name


def test_text_with_string_children():
    node = LpdfLayout.text(["Hello", " world"])
    d = node.to_dict()
    assert d == {"type": "text", "attrs": {}, "nodes": ["Hello", " world"]}


def test_text_with_span_children():
    s = LpdfLayout.span(["bold text"], options=SpanOptions(bold="true"))
    node = LpdfLayout.text(["Normal ", s])
    d = node.to_dict()
    assert d["nodes"][0] == "Normal "
    assert d["nodes"][1] == {"type": "span", "attrs": {"bold": "true"}, "nodes": ["bold text"]}


def test_divider():
    node = LpdfLayout.divider(options=DividerOptions(color="red", thickness="2pt"))
    d = node.to_dict()
    assert d == {"type": "divider", "attrs": {"color": "red", "thickness": "2pt"}}


# ── LpdfKit wrappers ──────────────────────────────────────────────────────────

def test_section():
    layout = LpdfKit.layout(nodes=[LpdfLayout.text(["Hello"])])
    section = LpdfKit.section(nodes=[layout], options=SectionOptions(size="a4", margin="28pt"))
    d = section.to_dict()
    assert d["type"] == "section"
    assert d["attrs"] == {"size": "a4", "margin": "28pt"}
    assert len(d["nodes"]) == 1


def test_layout_wrapper():
    layout = LpdfKit.layout(nodes=[LpdfLayout.text(["hi"])])
    d = layout.to_dict()
    assert d["type"] == "layout"
    assert "nodes" in d


def test_canvas_wrapper():
    canvas = LpdfKit.canvas()
    d = canvas.to_dict()
    assert d["type"] == "canvas"
    assert d["nodes"] == []


def test_document_includes_version():
    doc = LpdfKit.document()
    d = doc.to_dict()
    assert d["version"] == 1
    assert d["type"] == "document"


def test_document_with_meta_and_tokens():
    doc = LpdfKit.document(
        options=DocumentOptions(
            size="a4",
            meta=DocumentMeta(title="Test", author="Me"),
            tokens=DocumentTokens(colors={"primary": "#000"}),
        ),
    )
    d = doc.to_dict()
    assert d["attrs"]["size"] == "a4"
    assert d["attrs"]["meta"] == {"title": "Test", "author": "Me"}
    assert d["attrs"]["tokens"] == {"colors": {"primary": "#000"}}


def test_none_options_skipped():
    node = LpdfLayout.text(["hi"], options=TextOptions(font="Arial"))
    d = node.to_dict()
    assert d["attrs"] == {"font": "Arial"}
    assert "font-size" not in d["attrs"]


def test_section_uses_section_type_not_page():
    """Regression: serialised type must be 'section', not 'page'."""
    section = LpdfKit.section()
    assert section.to_dict()["type"] == "section"


def test_nodes_key_not_children():
    """Regression: serialised key must be 'nodes', not 'children'."""
    node = LpdfLayout.stack(nodes=[LpdfLayout.text(["x"])])
    d = node.to_dict()
    assert "nodes" in d
    assert "children" not in d
