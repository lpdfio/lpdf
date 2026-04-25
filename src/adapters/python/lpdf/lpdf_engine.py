from __future__ import annotations

import base64
import json
import re
from pathlib import Path

from .engine.engine_exception import EngineException
from .engine.engine_options import EngineOptions
from .engine.render_options import RenderOptions
from .engine.wasm_runner import WasmRunner
from .kit.document import Document


def _extract_xml_font_srcs(xml: str) -> dict[str, str]:
    srcs = {}
    for tag in re.findall(r'<font\s[^>]*>', xml):
        name = re.search(r'\bname="([^"]*)"', tag)
        ref  = re.search(r'\bref="([^"]*)"',  tag)
        src  = re.search(r'\bsrc="([^"]*)"',  tag)
        if name and src:
            key = ref.group(1) if ref else name.group(1)
            srcs[key] = src.group(1)
    return srcs


def _extract_xml_image_srcs(xml: str) -> dict[str, str]:
    srcs = {}
    for tag in re.findall(r'<image\s[^>]*>', xml):
        name = re.search(r'\bname="([^"]*)"', tag)
        ref  = re.search(r'\bref="([^"]*)"',  tag)
        src  = re.search(r'\bsrc="([^"]*)"',  tag)
        if name and src:
            key = ref.group(1) if ref else name.group(1)
            srcs[key] = src.group(1)
    return srcs


class LpdfEngine:
    def __init__(self, license_key: str, options: EngineOptions | None = None):
        self._license_key = license_key
        self._options = options or EngineOptions()
        self._fonts: dict[str, bytes] = {}
        self._images: dict[str, bytes] = {}
        self._encrypt: dict | None = None

    def load_font(self, name: str, data: bytes) -> LpdfEngine:
        self._fonts[name] = data
        return self

    def load_image(self, name: str, data: bytes) -> LpdfEngine:
        self._images[name] = data
        return self

    def set_encryption(
        self,
        user_password: str,
        owner_password: str,
        permissions: dict[str, bool] | None = None,
    ) -> LpdfEngine:
        self._encrypt = {
            "user_password":  user_password,
            "owner_password": owner_password,
            "permissions":    permissions or {},
        }
        return self

    def clear_encryption(self) -> LpdfEngine:
        self._encrypt = None
        return self

    def render_pdf(
        self,
        input: str | Document,
        options: RenderOptions | None = None,
    ) -> bytes:
        if isinstance(input, Document):
            method = "render_tree_pdf"
            input_dict = input.to_dict()
            input_str = json.dumps(input_dict, ensure_ascii=False)
        else:
            method = "render_pdf"
            input_str = input
            input_dict = None

        runner = WasmRunner(
            wasm_binary=self._options.wasm_binary or self._default_binary(),
            wasm_runner=self._options.wasm_runner or "wasmtime",
            timeout=self._options.timeout or 30,
        )

        # Font merging: engine-level options → per-call options → load_font() (wins)
        merged_fonts: dict[str, bytes] = {}
        if options and options.font_bytes:
            merged_fonts.update(options.font_bytes)
        merged_fonts.update(self._fonts)

        # Auto-load fonts declared via src= that haven't been explicitly provided.
        if input_dict is not None:
            tree_fonts = ((input_dict.get("attrs") or {}).get("tokens") or {}).get("fonts") or {}
            for fname, def_ in tree_fonts.items():
                if isinstance(def_, dict) and "src" in def_:
                    key = def_.get("ref") or fname
                    if key not in merged_fonts:
                        try:
                            with open(def_["src"], "rb") as fh:
                                merged_fonts[key] = fh.read()
                        except OSError:
                            pass
        else:
            for key, src in _extract_xml_font_srcs(input_str).items():
                if key not in merged_fonts:
                    try:
                        with open(src, "rb") as fh:
                            merged_fonts[key] = fh.read()
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

        # Image merging: per-call options → load_image() (wins)
        merged_images: dict[str, bytes] = {}
        if options and options.image_bytes:
            merged_images.update(options.image_bytes)
        merged_images.update(self._images)

        # Auto-load images declared via src= that haven't been explicitly provided.
        if input_dict is not None:
            tree_images = ((input_dict.get("attrs") or {}).get("tokens") or {}).get("images") or {}
            for iname, def_ in tree_images.items():
                if isinstance(def_, dict) and "src" in def_:
                    key = def_.get("ref") or iname
                    if key not in merged_images:
                        try:
                            with open(def_["src"], "rb") as fh:
                                merged_images[key] = fh.read()
                        except OSError:
                            pass
        else:
            for key, src in _extract_xml_image_srcs(input_str).items():
                if key not in merged_images:
                    try:
                        with open(src, "rb") as fh:
                            merged_images[key] = fh.read()
                    except OSError:
                        pass

        if merged_images:
            payload["images"] = {
                name: base64.b64encode(data).decode() for name, data in merged_images.items()
            }

        created_on = options and options.created_on
        if created_on is not None:
            payload["created_on"] = created_on

        if self._encrypt is not None:
            payload["encrypt"] = self._encrypt

        data = options and options.data
        if data is not None and method == "render_pdf":
            payload["data"] = data

        response = runner.invoke(payload)

        if "pdf" not in response:
            raise EngineException("Unexpected response from WASI process.")

        return base64.b64decode(response["pdf"])

    @staticmethod
    def kit_to_xml(
        doc: Document,
        wasm_binary: str | None = None,
        wasm_runner: str = "wasmtime",
    ) -> str:
        """Convert a Document tree (built with LpdfKit) to an lpdf XML string."""
        runner = WasmRunner(
            wasm_binary=wasm_binary or LpdfEngine._default_binary(),
            wasm_runner=wasm_runner,
        )
        payload = {
            "method": "kit_to_xml",
            "input":  json.dumps(doc.to_dict(), ensure_ascii=False),
        }
        response = runner.invoke(payload)
        if "xml" not in response:
            raise EngineException("Unexpected response from WASI process (kit_to_xml).")
        return response["xml"]

    @staticmethod
    def _default_binary() -> str:
        return str(Path(__file__).resolve().parent.parent / "resources" / "lpdf-wasi.wasm")
