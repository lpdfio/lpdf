from __future__ import annotations

from .canvas_node import CanvasNode
from .styles import RectStyle


class RectNode(CanvasNode):
    __slots__ = ("_x", "_y", "_w", "_h", "_style")

    def __init__(self, x: float, y: float, w: float, h: float, style: RectStyle | None = None):
        self._x = x
        self._y = y
        self._w = w
        self._h = h
        self._style = style

    def to_dict(self) -> dict:
        attrs: dict = {
            "x": str(self._x), "y": str(self._y),
            "w": str(self._w), "h": str(self._h),
        }
        s = self._style
        if s is not None:
            if s.fill is not None:         attrs["fill"]         = s.fill
            if s.stroke is not None:       attrs["stroke"]       = s.stroke
            if s.stroke_width is not None: attrs["stroke-width"] = str(s.stroke_width)
            if s.stroke_dash is not None:  attrs["stroke-dash"]  = " ".join(str(v) for v in s.stroke_dash)
            if s.border_radius is not None: attrs["radius"]      = str(s.border_radius)
        return {"type": "canvas-rect", "attrs": attrs}
