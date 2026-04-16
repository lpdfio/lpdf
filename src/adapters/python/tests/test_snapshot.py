import hashlib
import os
from pathlib import Path

import pytest

from lpdf import LpdfEngine


ROOT = Path("/app")
FIXTURES = ROOT / "test" / "fixtures"
SNAPSHOTS = ROOT / "test" / "snapshots"

EXAMPLES = [f"example{i}" for i in range(1, 12)]


@pytest.mark.parametrize("name", EXAMPLES)
def test_fixture_matches_stored_hash(name: str) -> None:
    xml = (FIXTURES / f"{name}.xml").read_text(encoding="utf-8")
    engine = LpdfEngine("test-key")
    pdf_bytes = engine.render_pdf(xml)
    sha = hashlib.sha256(pdf_bytes).hexdigest()
    snap = SNAPSHOTS / f"{name}.pdf.sha256"

    if os.environ.get("UPDATE_SNAPSHOTS") == "1":
        snap.write_text(sha)
    else:
        stored = snap.read_text().strip()
        assert sha == stored, f"Snapshot mismatch for {name}"


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
