from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class GridOptions:
    cols: str | None = None
    col_width: str | None = None
    gap: str | None = None
    equal: str | None = None
    padding: str | None = None
    background: str | None = None
    width: str | None = None
    height: str | None = None
    border: str | None = None
    radius: str | None = None
    debug: str | None = None
