from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class BarcodeOptions:
    type: str = ""
    data: str = ""
    size: str | None = None
    width: str | None = None
    height: str | None = None
    ec: str | None = None
    hrt: str | None = None
    color: str | None = None
    background: str | None = None
    repeat: str | None = None
    debug: str | None = None
