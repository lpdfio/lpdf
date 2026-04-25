from __future__ import annotations

import math
from dataclasses import dataclass


@dataclass(frozen=True)
class Transform:
    """Affine transform matrix [a, b, c, d, e, f] (SVG/PDF convention)."""
    matrix: list[float]

    @staticmethod
    def rotate(degrees: float, cx: float = 0.0, cy: float = 0.0) -> Transform:
        """Rotate by degrees clockwise around (cx, cy)."""
        rad = math.radians(degrees)
        cos = math.cos(rad)
        sin = math.sin(rad)
        e = cx - cx * cos + cy * sin
        f = cy - cx * sin - cy * cos
        return Transform([cos, sin, -sin, cos, e, f])

    @staticmethod
    def scale(sx: float, sy: float) -> Transform:
        return Transform([sx, 0.0, 0.0, sy, 0.0, 0.0])

    @staticmethod
    def translate(tx: float, ty: float) -> Transform:
        return Transform([1.0, 0.0, 0.0, 1.0, tx, ty])
