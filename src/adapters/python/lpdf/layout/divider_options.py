from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class DividerOptions:
    color: str | None = None
    thickness: str | None = None
    direction: str | None = None
