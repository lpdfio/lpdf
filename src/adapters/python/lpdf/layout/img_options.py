from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class ImgOptions:
    name: str = ""
    height: str | None = None
    width: str | None = None
    font: str | None = None
    font_size: str | None = None
    gap: str | None = None
    padding: str | None = None
    background: str | None = None
    border: str | None = None
    radius: str | None = None
    repeat: str | None = None
    debug: str | None = None
