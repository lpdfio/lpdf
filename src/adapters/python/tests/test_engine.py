import base64
import json
from unittest.mock import patch, MagicMock

import pytest

from lpdf import LpdfEngine, RenderOptions, document, page, text, PageOptions, DocumentOptions, LpdfMeta


def _mock_subprocess_run(pdf_bytes: bytes = b"%PDF-1.4 test"):
    response = json.dumps({"pdf": base64.b64encode(pdf_bytes).decode()}).encode()
    mock_result = MagicMock()
    mock_result.returncode = 0
    mock_result.stdout = response
    mock_result.stderr = b""
    return mock_result


class TestRenderXml:
    @patch("lpdf.wasm_runner.subprocess.run")
    def test_render_xml_string(self, mock_run):
        mock_run.return_value = _mock_subprocess_run()

        engine = LpdfEngine("test-key")
        result = engine.render_pdf("<document><page><text>Hello</text></page></document>")

        assert result == b"%PDF-1.4 test"
        call_args = mock_run.call_args
        payload = json.loads(call_args.kwargs["input"] if "input" in call_args.kwargs else call_args[1]["input"])
        assert payload["method"] == "render_pdf"
        assert payload["key"] == "test-key"


class TestRenderTree:
    @patch("lpdf.wasm_runner.subprocess.run")
    def test_render_tree(self, mock_run):
        mock_run.return_value = _mock_subprocess_run()

        doc = document(
            nodes=[page(nodes=[text(["Hello"])], options=PageOptions(size="a4"))],
            options=DocumentOptions(meta=LpdfMeta(title="Test")),
        )
        engine = LpdfEngine("test-key")
        result = engine.render_pdf(doc)

        assert result == b"%PDF-1.4 test"
        call_args = mock_run.call_args
        payload = json.loads(call_args.kwargs["input"] if "input" in call_args.kwargs else call_args[1]["input"])
        assert payload["method"] == "render_tree_pdf"


class TestFontMerging:
    @patch("lpdf.wasm_runner.subprocess.run")
    def test_load_font_takes_precedence(self, mock_run):
        mock_run.return_value = _mock_subprocess_run()

        constructor_opts = RenderOptions(font_bytes={"Arial": b"constructor"})
        call_opts = RenderOptions(font_bytes={"Arial": b"call"})

        engine = LpdfEngine("key", options=constructor_opts)
        engine.load_font("Arial", b"loaded")
        engine.render_pdf("<doc/>", call_options=call_opts)

        payload = json.loads(mock_run.call_args.kwargs["input"] if "input" in mock_run.call_args.kwargs else mock_run.call_args[1]["input"])
        assert payload["fonts"]["Arial"] == base64.b64encode(b"loaded").decode()

    @patch("lpdf.wasm_runner.subprocess.run")
    def test_call_options_over_constructor(self, mock_run):
        mock_run.return_value = _mock_subprocess_run()

        constructor_opts = RenderOptions(font_bytes={"Arial": b"constructor"})
        call_opts = RenderOptions(font_bytes={"Arial": b"call"})

        engine = LpdfEngine("key", options=constructor_opts)
        engine.render_pdf("<doc/>", call_options=call_opts)

        payload = json.loads(mock_run.call_args.kwargs["input"] if "input" in mock_run.call_args.kwargs else mock_run.call_args[1]["input"])
        assert payload["fonts"]["Arial"] == base64.b64encode(b"call").decode()


class TestCreatedOn:
    @patch("lpdf.wasm_runner.subprocess.run")
    def test_created_on_passthrough(self, mock_run):
        mock_run.return_value = _mock_subprocess_run()

        engine = LpdfEngine("key", options=RenderOptions(created_on="2024-01-01T00:00:00Z"))
        engine.render_pdf("<doc/>")

        payload = json.loads(mock_run.call_args.kwargs["input"] if "input" in mock_run.call_args.kwargs else mock_run.call_args[1]["input"])
        assert payload["created_on"] == "2024-01-01T00:00:00Z"

    @patch("lpdf.wasm_runner.subprocess.run")
    def test_call_options_created_on_overrides(self, mock_run):
        mock_run.return_value = _mock_subprocess_run()

        engine = LpdfEngine("key", options=RenderOptions(created_on="2024-01-01"))
        engine.render_pdf("<doc/>", call_options=RenderOptions(created_on="2025-01-01"))

        payload = json.loads(mock_run.call_args.kwargs["input"] if "input" in mock_run.call_args.kwargs else mock_run.call_args[1]["input"])
        assert payload["created_on"] == "2025-01-01"
