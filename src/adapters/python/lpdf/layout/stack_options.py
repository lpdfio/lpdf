from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class StackOptions:
    gap: str | None = None
    padding: str | None = None
    background: str | None = None
    align: str | None = None
    justify: str | None = None
    width: str | None = None
    height: str | None = None
    border: str | None = None
    radius: str | None = None
    debug: str | None = None
