from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class Clip:
    x: float
    y: float
    w: float
    h: float
    border_radius: float = 0.0
