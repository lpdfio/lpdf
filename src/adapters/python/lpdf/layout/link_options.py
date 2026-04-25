from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class LinkOptions:
    url: str | None = None
    width: str | None = None
    height: str | None = None
