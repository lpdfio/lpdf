from __future__ import annotations

from dataclasses import dataclass

from .text_align import TextAlign


@dataclass(frozen=True)
class TextStyle:
    font: str | None = None
    size: float | None = None
    color: str | None = None
    align: TextAlign | None = None
    line_height: float | None = None
    width: float | None = None
