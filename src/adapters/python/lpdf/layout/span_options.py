from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class SpanOptions:
    font: str | None = None
    font_size: str | None = None
    color: str | None = None
    bold: str | None = None
    url: str | None = None
    underline: str | None = None
    strike: str | None = None
