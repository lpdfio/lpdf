from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class TrOptions:
    background: str | None = None
