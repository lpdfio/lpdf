from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class TdOptions:
    padding: str | None = None
    background: str | None = None
    align: str | None = None
    valign: str | None = None
    border: str | None = None
    radius: str | None = None
    gap: str | None = None
    debug: str | None = None
