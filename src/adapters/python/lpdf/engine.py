from __future__ import annotations

import base64
import json
import struct
from pathlib import Path
from typing import Optional

from .options import RenderOptions
from .types import LpdfDocument
from .wasm_runner import WasmRunner


class LpdfEngine:
    def __init__(self, license_key: str, options: RenderOptions | None = None):
        self._license_key = license_key
        self._options = options or RenderOptions()
        self._fonts: dict[str, bytes] = {}

    def load_font(self, name: str, data: bytes) -> LpdfEngine:
        self._fonts[name] = data
        return self

    def render_pdf(
        self,
        input: str | LpdfDocument,
        call_options: RenderOptions | None = None,
    ) -> bytes:
        if isinstance(input, LpdfDocument):
            method = "render_tree_pdf"
            input_str = json.dumps(input.to_dict(), ensure_ascii=False)
        else:
            method = "render_pdf"
            input_str = input

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

        payload: dict = {
            "method": method,
            "key": self._license_key,
            "input": input_str,
        }

        if merged_fonts:
            payload["fonts"] = {
                name: base64.b64encode(data).decode() for name, data in merged_fonts.items()
            }

        # Extract glyph advance widths from each font binary and pass to the WASI
        # engine so the Rust layout pass measures custom-font text accurately.
        metrics: dict = {}
        for name, data in merged_fonts.items():
            w = LpdfEngine.extract_font_widths(data)
            if w is not None:
                metrics[name] = w
        if metrics:
            payload["metrics"] = metrics

        created_on = (call_options and call_options.created_on) or self._options.created_on
        if created_on is not None:
            payload["created_on"] = created_on

        response = runner.invoke(payload)

        if "pdf" not in response:
            raise RuntimeError("Unexpected response from WASI process.")

        return base64.b64decode(response["pdf"])

    @staticmethod
    def extract_font_widths(data: bytes) -> Optional[dict]:
        """Parse head/hhea/cmap/hmtx tables from a TrueType or OpenType font binary
        and return per-glyph advance widths for printable ASCII (code points 32-126),
        normalised to 1/1000 em units — the format expected by the Rust layout engine.

        Returns None if the font cannot be parsed (WOFF/WOFF2/unsupported cmap format).
        """
        if len(data) < 12:
            return None

        def u16(off: int) -> int:
            return struct.unpack_from(">H", data, off)[0]

        def u32(off: int) -> int:
            return struct.unpack_from(">I", data, off)[0]

        def i16(off: int) -> int:
            return struct.unpack_from(">h", data, off)[0]

        # ── sfnt table directory ──────────────────────────────────────────────
        num_tables = u16(4)
        tables: dict[str, int] = {}
        for i in range(num_tables):
            b = 12 + i * 16
            if b + 16 > len(data):
                return None
            tag = data[b : b + 4].decode("latin-1")
            tables[tag] = u32(b + 8)

        if not all(k in tables for k in ("head", "cmap", "hmtx", "hhea")):
            return None

        # ── units-per-em (head, offset 18) ───────────────────────────────────
        upm = u16(tables["head"] + 18)
        if upm == 0:
            return None

        # ── numOfLongHorMetrics (hhea, offset 34) ────────────────────────────
        num_hmetrics = u16(tables["hhea"] + 34)
        if num_hmetrics == 0:
            return None

        # ── glyph advance width from hmtx ────────────────────────────────────
        hmtx_base = tables["hmtx"]

        def get_advance(glyph_id: int) -> int:
            idx = min(glyph_id, num_hmetrics - 1)
            return u16(hmtx_base + idx * 4)

        # ── find Unicode BMP cmap subtable ────────────────────────────────────
        cmap_base = tables["cmap"]
        num_enc_tbls = u16(cmap_base + 2)
        subtable_off = -1
        best_priority = 999
        for i in range(num_enc_tbls):
            b = cmap_base + 4 + i * 8
            platform_id = u16(b)
            encoding_id = u16(b + 2)
            off = cmap_base + u32(b + 4)
            if platform_id == 3 and encoding_id == 1 and best_priority > 0:
                subtable_off = off
                best_priority = 0
            elif platform_id == 0 and best_priority > 1:
                subtable_off = off
                best_priority = 1

        if subtable_off < 0:
            return None

        # ── parse cmap format 4 ───────────────────────────────────────────────
        if u16(subtable_off) != 4:
            return None

        seg_count = u16(subtable_off + 6) >> 1
        end_codes_off   = subtable_off + 14
        start_codes_off = end_codes_off   + seg_count * 2 + 2  # +2 for reservedPad
        id_delta_off    = start_codes_off + seg_count * 2
        id_range_off    = id_delta_off    + seg_count * 2

        def get_glyph_id(cp: int) -> int:
            for s in range(seg_count):
                end = u16(end_codes_off + s * 2)
                if cp > end:
                    continue
                start = u16(start_codes_off + s * 2)
                if cp < start:
                    return 0
                delta     = i16(id_delta_off + s * 2)
                range_off = u16(id_range_off + s * 2)
                if range_off == 0:
                    return (cp + delta) & 0xFFFF
                glyph_off = id_range_off + s * 2 + range_off + (cp - start) * 2
                glyph_id  = u16(glyph_off)
                return 0 if glyph_id == 0 else (glyph_id + delta) & 0xFFFF
            return 0

        # ── sample ASCII range (32–126) ───────────────────────────────────────
        ascii_widths: list[int] = []
        total = 0
        count = 0
        for cp in range(32, 127):
            glyph_id = get_glyph_id(cp)
            adv = get_advance(glyph_id) if glyph_id > 0 else get_advance(0)
            w = round(adv * 1000 / upm)
            ascii_widths.append(w)
            if w > 0:
                total += w
                count += 1

        default = round(total / count) if count > 0 else 500
        return {"default": default, "ascii": ascii_widths}

    @staticmethod
    def _default_binary() -> str:
        return str(Path(__file__).resolve().parent.parent / "resources" / "lpdf-wasi.wasm")
