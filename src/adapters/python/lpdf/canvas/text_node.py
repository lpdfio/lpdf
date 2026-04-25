from __future__ import annotations

from .canvas_node import CanvasNode
from .text_style import TextStyle
from .run import Run


class CanvasTextNode(CanvasNode):
    """Canvas text node at an absolute coordinate position."""

    __slots__ = ("_x", "_y", "_content", "_style", "_runs")

    def __init__(
        self,
        x: float,
        y: float,
        content: str,
        style: TextStyle | None = None,
        runs: list[Run] | None = None,
    ):
        self._x = x
        self._y = y
        self._content = content
        self._style = style
        self._runs = runs or []

    def to_dict(self) -> dict:
        attrs: dict = {"x": str(self._x), "y": str(self._y)}
        s = self._style
        if s is not None:
            if s.font is not None:        attrs["font"]        = s.font
            if s.size is not None:        attrs["font-size"]   = str(s.size)
            if s.color is not None:       attrs["color"]       = s.color
            if s.align is not None:       attrs["align"]       = str(s.align)
            if s.line_height is not None: attrs["line-height"] = str(s.line_height)
            if s.width is not None:       attrs["w"]           = str(s.width)
        node: dict = {"type": "canvas-text", "text": self._content, "attrs": attrs}
        if self._runs:
            node["runs"] = [r.to_dict() for r in self._runs]
        return node
