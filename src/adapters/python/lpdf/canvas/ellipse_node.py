from __future__ import annotations

from .canvas_node import CanvasNode
from .styles import EllipseStyle


class EllipseNode(CanvasNode):
    __slots__ = ("_cx", "_cy", "_rx", "_ry", "_style")

    def __init__(self, cx: float, cy: float, rx: float, ry: float, style: EllipseStyle | None = None):
        self._cx = cx
        self._cy = cy
        self._rx = rx
        self._ry = ry
        self._style = style

    def to_dict(self) -> dict:
        attrs: dict = {
            "x": str(self._cx), "y": str(self._cy),
            "rx": str(self._rx), "ry": str(self._ry),
        }
        s = self._style
        if s is not None:
            if s.fill is not None:         attrs["fill"]         = s.fill
            if s.stroke is not None:       attrs["stroke"]       = s.stroke
            if s.stroke_width is not None: attrs["stroke-width"] = str(s.stroke_width)
            if s.stroke_dash is not None:  attrs["stroke-dash"]  = " ".join(str(v) for v in s.stroke_dash)
        return {"type": "canvas-ellipse", "attrs": attrs}
