import pytest

from lpdf import LpdfEngine

from .snapshot_helper import EXAMPLES, FIXTURES, compare_or_update


@pytest.mark.parametrize("name", EXAMPLES)
def test_fixture_matches_stored_hash(name: str) -> None:
    xml = (FIXTURES / f"{name}.xml").read_text(encoding="utf-8")
    engine = LpdfEngine("test-key")
    pdf_bytes = engine.render_pdf(xml)
    compare_or_update(name, pdf_bytes)


def test_output_is_pdf() -> None:
    xml = (FIXTURES / "example1.xml").read_text(encoding="utf-8")
    pdf_bytes = LpdfEngine("test-key").render_pdf(xml)
    assert pdf_bytes[:5] == b"%PDF-"


def test_custom_font_does_not_throw() -> None:
    xml = (FIXTURES / "example1.xml").read_text(encoding="utf-8")
    engine = LpdfEngine("test-key")
    engine.load_font("TestFont", b"")
    pdf_bytes = engine.render_pdf(xml)
    assert pdf_bytes[:5] == b"%PDF-"
