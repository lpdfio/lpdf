from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class RenderOptions:
    """Per-call options for LpdfEngine.render_pdf()."""
    created_on: str | None = None
    font_bytes: dict[str, bytes] | None = None
    image_bytes: dict[str, bytes] | None = None
    data: dict | None = None
