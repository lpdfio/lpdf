from __future__ import annotations

from dataclasses import dataclass

from .transform import Transform
from .clip import Clip


@dataclass(frozen=True)
class LayerOptions:
    page: str | None = None
    opacity: float | None = None
    transform: Transform | None = None
    clip: Clip | None = None
