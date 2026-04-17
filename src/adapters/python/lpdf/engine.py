from __future__ import annotations

import base64
import json
import re
from pathlib import Path

from .exceptions import LpdfRenderError
from .options import RenderOptions
from .types import LpdfDocument
from .wasm_runner import WasmRunner


def _extract_xml_font_srcs(xml: str) -> dict[str, str]:
    srcs = {}
    for tag in re.findall(r'<font\s[^>]*>', xml):
        name = re.search(r'\bname="([^"]*)"', tag)
        src  = re.search(r'\bsrc="([^"]*)"',  tag)
        if name and src:
            srcs[name.group(1)] = src.group(1)
    return srcs


class LpdfEngine:
    def __init__(self, license_key: str, options: RenderOptions | None = None):
        self._license_key = license_key
        self._options = options or RenderOptions()
        self._fonts: dict[str, bytes] = {}
        self._images: dict[str, bytes] = {}

    def load_font(self, name: str, data: bytes) -> LpdfEngine:
        self._fonts[name] = data
        return self

    def load_image(self, name: str, data: bytes) -> LpdfEngine:
        self._images[name] = data
        return self

    def render_pdf(
        self,
        input: str | LpdfDocument,
        call_options: RenderOptions | None = None,
    ) -> bytes:
        if isinstance(input, LpdfDocument):
            method = "render_tree_pdf"
            input_dict = input.to_dict()
            input_str = json.dumps(input_dict, ensure_ascii=False)
        else:
            method = "render_pdf"
            input_str = input
            input_dict = None

        runner = WasmRunner(
            wasm_binary=call_options and call_options.wasm_binary or self._options.wasm_binary or self._default_binary(),
            wasm_runner=call_options and call_options.wasm_runner or self._options.wasm_runner or "wasmtime",
        )

        merged_fonts: dict[str, bytes] = {}
        if self._options.font_bytes:
            merged_fonts.update(self._options.font_bytes)
        if call_options and call_options.font_bytes:
            merged_fonts.update(call_options.font_bytes)
        merged_fonts.update(self._fonts)

        # Auto-load fonts declared via src= that haven't been explicitly provided.
        if input_dict is not None:
            tree_fonts = ((input_dict.get("attrs") or {}).get("tokens") or {}).get("fonts") or {}
            for name, def_ in tree_fonts.items():
                if isinstance(def_, dict) and "src" in def_ and name not in merged_fonts:
                    try:
                        with open(def_["src"], "rb") as fh:
                            merged_fonts[name] = fh.read()
                    except OSError:
                        pass
        else:
            for name, src in _extract_xml_font_srcs(input_str).items():
                if name not in merged_fonts:
                    try:
                        with open(src, "rb") as fh:
                            merged_fonts[name] = fh.read()
                    except OSError:
                        pass

        payload: dict = {
            "method": method,
            "key": self._license_key,
            "input": input_str,
        }

        if merged_fonts:
            payload["fonts"] = {
                name: base64.b64encode(data).decode() for name, data in merged_fonts.items()
            }

        merged_images: dict[str, bytes] = {}
        if self._options.image_bytes:
            merged_images.update(self._options.image_bytes)
        if call_options and call_options.image_bytes:
            merged_images.update(call_options.image_bytes)
        merged_images.update(self._images)

        if merged_images:
            payload["images"] = {
                name: base64.b64encode(data).decode() for name, data in merged_images.items()
            }

        created_on = (call_options and call_options.created_on) or self._options.created_on
        if created_on is not None:
            payload["created_on"] = created_on

        response = runner.invoke(payload)

        if "pdf" not in response:
            raise LpdfRenderError("Unexpected response from WASI process.")

        return base64.b64decode(response["pdf"])

    @staticmethod
    def _default_binary() -> str:
        return str(Path(__file__).resolve().parent.parent / "resources" / "lpdf-wasi.wasm")
