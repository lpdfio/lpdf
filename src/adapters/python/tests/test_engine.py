import base64
import json
from unittest.mock import patch, MagicMock

import pytest

from lpdf import LpdfEngine, RenderOptions, document, page, text, PageOptions, DocumentOptions, LpdfMeta
from lpdf.types import LpdfTokens


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


def _mock_kit_to_xml_run(xml: str = "<lpdf version=\"1\"/>"):
    response = json.dumps({"xml": xml}).encode()
    mock_result = MagicMock()
    mock_result.returncode = 0
    mock_result.stdout = response
    mock_result.stderr = b""
    return mock_result


class TestKitToXml:
    @patch("lpdf.wasm_runner.subprocess.run")
    def test_sends_kit_to_xml_method(self, mock_run):
        mock_run.return_value = _mock_kit_to_xml_run()

        doc = document(nodes=[page(nodes=[])], options=DocumentOptions())
        LpdfEngine.kit_to_xml(doc)

        payload = json.loads(mock_run.call_args.kwargs["input"] if "input" in mock_run.call_args.kwargs else mock_run.call_args[1]["input"])
        assert payload["method"] == "kit_to_xml"

    @patch("lpdf.wasm_runner.subprocess.run")
    def test_returns_xml_string(self, mock_run):
        expected_xml = '<?xml version="1.0" encoding="UTF-8"?>\n<lpdf version="1"/>'
        mock_run.return_value = _mock_kit_to_xml_run(expected_xml)

        doc = document(nodes=[page(nodes=[])], options=DocumentOptions())
        result = LpdfEngine.kit_to_xml(doc)

        assert result == expected_xml

    @patch("lpdf.wasm_runner.subprocess.run")
    def test_input_contains_serialised_document(self, mock_run):
        mock_run.return_value = _mock_kit_to_xml_run()

        doc = document(nodes=[page(nodes=[text(["Hello"])])], options=DocumentOptions())
        LpdfEngine.kit_to_xml(doc)

        payload = json.loads(mock_run.call_args.kwargs["input"] if "input" in mock_run.call_args.kwargs else mock_run.call_args[1]["input"])
        inner = json.loads(payload["input"])
        assert inner["version"] == 1
        assert inner["type"] == "document"

    @patch("lpdf.wasm_runner.subprocess.run")
    def test_raises_on_error_response(self, mock_run):
        from lpdf.exceptions import LpdfRenderError
        error_response = json.dumps({"error": "invalid kit JSON"}).encode()
        mock_result = MagicMock()
        mock_result.returncode = 0
        mock_result.stdout = error_response
        mock_result.stderr = b""
        mock_run.return_value = mock_result

        doc = document(nodes=[page(nodes=[])], options=DocumentOptions())
        with pytest.raises(LpdfRenderError, match="invalid kit JSON"):
            LpdfEngine.kit_to_xml(doc)


class TestEncryption:
    @patch("lpdf.wasm_runner.subprocess.run")
    def test_encrypt_payload_sent(self, mock_run):
        mock_run.return_value = _mock_subprocess_run()

        engine = LpdfEngine("key")
        engine.set_encryption("", "s3cr3t", {"print": False})
        engine.render_pdf("<doc/>")

        payload = json.loads(mock_run.call_args.kwargs["input"] if "input" in mock_run.call_args.kwargs else mock_run.call_args[1]["input"])
        assert "encrypt" in payload
        assert payload["encrypt"]["user_password"] == ""
        assert payload["encrypt"]["owner_password"] == "s3cr3t"
        assert payload["encrypt"]["permissions"] == {"print": False}

    @patch("lpdf.wasm_runner.subprocess.run")
    def test_clear_encryption_removes_payload(self, mock_run):
        mock_run.return_value = _mock_subprocess_run()

        engine = LpdfEngine("key")
        engine.set_encryption("user", "owner")
        engine.clear_encryption()
        engine.render_pdf("<doc/>")

        payload = json.loads(mock_run.call_args.kwargs["input"] if "input" in mock_run.call_args.kwargs else mock_run.call_args[1]["input"])
        assert "encrypt" not in payload

    @patch("lpdf.wasm_runner.subprocess.run")
    def test_encrypt_defaults_to_empty_permissions(self, mock_run):
        mock_run.return_value = _mock_subprocess_run()

        engine = LpdfEngine("key")
        engine.set_encryption("", "owner")
        engine.render_pdf("<doc/>")

        payload = json.loads(mock_run.call_args.kwargs["input"] if "input" in mock_run.call_args.kwargs else mock_run.call_args[1]["input"])
        assert payload["encrypt"]["permissions"] == {}


class TestLoadImage:
    @patch("lpdf.wasm_runner.subprocess.run")
    def test_image_sent_in_payload(self, mock_run):
        mock_run.return_value = _mock_subprocess_run()

        engine = LpdfEngine("key")
        engine.load_image("logo", b"\x89PNG")
        engine.render_pdf("<doc/>")

        payload = json.loads(mock_run.call_args.kwargs["input"] if "input" in mock_run.call_args.kwargs else mock_run.call_args[1]["input"])
        assert "images" in payload
        assert payload["images"]["logo"] == base64.b64encode(b"\x89PNG").decode()

    @patch("lpdf.wasm_runner.subprocess.run")
    def test_load_image_via_render_options(self, mock_run):
        mock_run.return_value = _mock_subprocess_run()

        engine = LpdfEngine("key", options=RenderOptions(image_bytes={"bg": b"imgdata"}))
        engine.render_pdf("<doc/>")

        payload = json.loads(mock_run.call_args.kwargs["input"] if "input" in mock_run.call_args.kwargs else mock_run.call_args[1]["input"])
        assert "images" in payload
        assert payload["images"]["bg"] == base64.b64encode(b"imgdata").decode()

    @patch("lpdf.wasm_runner.subprocess.run")
    def test_load_image_instance_takes_precedence_over_options(self, mock_run):
        mock_run.return_value = _mock_subprocess_run()

        engine = LpdfEngine("key", options=RenderOptions(image_bytes={"logo": b"opts-version"}))
        engine.load_image("logo", b"loaded-version")
        engine.render_pdf("<doc/>")

        payload = json.loads(mock_run.call_args.kwargs["input"] if "input" in mock_run.call_args.kwargs else mock_run.call_args[1]["input"])
        assert payload["images"]["logo"] == base64.b64encode(b"loaded-version").decode()


class TestAssetSrcExtraction:
    def test_xml_font_src_uses_ref_as_key(self):
        from lpdf.engine import _extract_xml_font_srcs
        xml = '<lpdf><assets><font name="body" ref="my-body" src="/fonts/MyFont.ttf"/></assets></lpdf>'
        srcs = _extract_xml_font_srcs(xml)
        assert "my-body" in srcs
        assert srcs["my-body"] == "/fonts/MyFont.ttf"
        assert "body" not in srcs

    def test_xml_font_src_falls_back_to_name(self):
        from lpdf.engine import _extract_xml_font_srcs
        xml = '<lpdf><assets><font name="body" src="/fonts/MyFont.ttf"/></assets></lpdf>'
        srcs = _extract_xml_font_srcs(xml)
        assert "body" in srcs
        assert srcs["body"] == "/fonts/MyFont.ttf"

    def test_xml_image_src_uses_ref_as_key(self):
        from lpdf.engine import _extract_xml_image_srcs
        xml = '<lpdf><assets><image name="logo" ref="my-logo" src="/img/logo.png"/></assets></lpdf>'
        srcs = _extract_xml_image_srcs(xml)
        assert "my-logo" in srcs
        assert srcs["my-logo"] == "/img/logo.png"
        assert "logo" not in srcs

    def test_xml_image_src_falls_back_to_name(self):
        from lpdf.engine import _extract_xml_image_srcs
        xml = '<lpdf><assets><image name="logo" src="/img/logo.png"/></assets></lpdf>'
        srcs = _extract_xml_image_srcs(xml)
        assert "logo" in srcs
        assert srcs["logo"] == "/img/logo.png"

    @patch("lpdf.wasm_runner.subprocess.run")
    def test_render_xml_auto_loads_image_src(self, mock_run, tmp_path):
        mock_run.return_value = _mock_subprocess_run()
        img_file = tmp_path / "logo.png"
        img_file.write_bytes(b"\x89PNG")
        xml = f'<lpdf><assets><image name="logo" src="{img_file}"/></assets><document size="a4"><pages><page/></pages></document></lpdf>'
        engine = LpdfEngine("key")
        engine.render_pdf(xml)
        payload = json.loads(mock_run.call_args.kwargs["input"] if "input" in mock_run.call_args.kwargs else mock_run.call_args[1]["input"])
        assert "images" in payload
        assert payload["images"]["logo"] == base64.b64encode(b"\x89PNG").decode()
