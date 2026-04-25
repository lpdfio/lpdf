from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class FrameOptions:
    width: str | None = None
    height: str | None = None
    padding: str | None = None
    background: str | None = None
    border: str | None = None
    radius: str | None = None
    align: str | None = None
    debug: str | None = None
