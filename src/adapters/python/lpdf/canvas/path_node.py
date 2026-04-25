from __future__ import annotations

from .canvas_node import CanvasNode
from .styles import PathStyle


class PathNode(CanvasNode):
    __slots__ = ("_d", "_style")

    def __init__(self, d: str, style: PathStyle | None = None):
        self._d = d
        self._style = style

    def to_dict(self) -> dict:
        attrs: dict = {"d": self._d}
        s = self._style
        if s is not None:
            if s.fill is not None:              attrs["fill"]         = s.fill
            if s.stroke is not None:            attrs["stroke"]       = s.stroke
            if s.stroke_width is not None:      attrs["stroke-width"] = str(s.stroke_width)
            if s.stroke_dash is not None:       attrs["stroke-dash"]  = " ".join(str(v) for v in s.stroke_dash)
            if s.fill_rule_evenodd is not None: attrs["fill-rule"]    = "evenodd" if s.fill_rule_evenodd else "nonzero"
            if s.line_cap is not None:          attrs["line-cap"]     = str(s.line_cap)
            if s.line_join is not None:         attrs["line-join"]    = str(s.line_join)
        return {"type": "canvas-path", "attrs": attrs}
