from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class RegionOptions:
    page: str | None = None
    w: str | None = None
    debug: str | None = None
