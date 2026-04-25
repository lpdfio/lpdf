from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class SectionOptions:
    size: str | None = None
    orientation: str | None = None
    margin: str | None = None
    background: str | None = None
    debug: str | None = None
