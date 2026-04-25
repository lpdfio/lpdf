from __future__ import annotations

from .canvas.layer_node import LayerNode
from .canvas.layer_options import LayerOptions
from .canvas.rect_node import RectNode
from .canvas.line_node import LineNode
from .canvas.ellipse_node import EllipseNode
from .canvas.circle_node import CircleNode
from .canvas.path_node import PathNode
from .canvas.text_node import CanvasTextNode
from .canvas.image_node import ImageNode
from .canvas.styles import RectStyle, LineStyle, EllipseStyle, PathStyle
from .canvas.text_style import TextStyle
from .canvas.run import Run


class LpdfCanvas:
    """Static factory for all canvas primitives."""

    @staticmethod
    def layer(nodes: list | None = None, options: LayerOptions | None = None) -> LayerNode:
        return LayerNode(nodes=nodes or [], options=options)

    @staticmethod
    def rect(
        x: float, y: float, w: float, h: float,
        style: RectStyle | None = None,
    ) -> RectNode:
        return RectNode(x, y, w, h, style)

    @staticmethod
    def line(
        x1: float, y1: float, x2: float, y2: float,
        style: LineStyle | None = None,
    ) -> LineNode:
        return LineNode(x1, y1, x2, y2, style)

    @staticmethod
    def ellipse(
        cx: float, cy: float, rx: float, ry: float,
        style: EllipseStyle | None = None,
    ) -> EllipseNode:
        return EllipseNode(cx, cy, rx, ry, style)

    @staticmethod
    def circle(
        cx: float, cy: float, r: float,
        style: EllipseStyle | None = None,
    ) -> CircleNode:
        return CircleNode(cx, cy, r, style)

    @staticmethod
    def path(d: str, style: PathStyle | None = None) -> PathNode:
        return PathNode(d, style)

    @staticmethod
    def text(
        x: float,
        y: float,
        content: str,
        style: TextStyle | None = None,
        runs: list[Run] | None = None,
    ) -> CanvasTextNode:
        return CanvasTextNode(x, y, content, style, runs)

    @staticmethod
    def image(x: float, y: float, w: float, h: float, name: str) -> ImageNode:
        return ImageNode(x, y, w, h, name)
