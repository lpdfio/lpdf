from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class TableOptions:
    cols: str | None = None
    border: str | None = None
    stripe: str | None = None
    gap: str | None = None
    padding: str | None = None
    background: str | None = None
    width: str | None = None
    height: str | None = None
    repeat: str | None = None
    debug: str | None = None
