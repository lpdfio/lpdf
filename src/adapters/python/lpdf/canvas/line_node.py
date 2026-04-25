from __future__ import annotations

from .canvas_node import CanvasNode
from .styles import LineStyle


class LineNode(CanvasNode):
    __slots__ = ("_x1", "_y1", "_x2", "_y2", "_style")

    def __init__(self, x1: float, y1: float, x2: float, y2: float, style: LineStyle | None = None):
        self._x1 = x1
        self._y1 = y1
        self._x2 = x2
        self._y2 = y2
        self._style = style

    def to_dict(self) -> dict:
        attrs: dict = {
            "x1": str(self._x1), "y1": str(self._y1),
            "x2": str(self._x2), "y2": str(self._y2),
        }
        s = self._style
        if s is not None:
            if s.stroke is not None:       attrs["stroke"]       = s.stroke
            if s.stroke_width is not None: attrs["stroke-width"] = str(s.stroke_width)
            if s.stroke_dash is not None:  attrs["stroke-dash"]  = " ".join(str(v) for v in s.stroke_dash)
            if s.line_cap is not None:     attrs["line-cap"]     = str(s.line_cap)
            if s.line_join is not None:    attrs["line-join"]    = str(s.line_join)
        return {"type": "canvas-line", "attrs": attrs}
