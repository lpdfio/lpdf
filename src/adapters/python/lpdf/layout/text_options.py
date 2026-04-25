from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class TextOptions:
    font: str | None = None
    font_size: str | None = None
    text_align: str | None = None
    color: str | None = None
    bold: str | None = None
    end: str | None = None
    repeat: str | None = None
    width: str | None = None
    height: str | None = None
    padding: str | None = None
    background: str | None = None
    border: str | None = None
    radius: str | None = None
