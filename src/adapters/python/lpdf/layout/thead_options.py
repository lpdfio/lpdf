from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class TheadOptions:
    background: str | None = None
