from __future__ import annotations

from ..shared.page_scope import PageScope
from .canvas_node import CanvasNode
from .clip import Clip
from .layer_options import LayerOptions


class LayerNode(CanvasNode):
    """A canvas layer containing canvas primitives."""

    __slots__ = ("_nodes", "_options")

    def __init__(self, nodes: list, options: LayerOptions | None = None):
        self._nodes = nodes
        self._options = options

    def to_dict(self) -> dict:
        attrs: dict = {}
        o = self._options
        if o is not None:
            if o.page is not None:
                attrs["page"] = o.page
            if o.opacity is not None:
                attrs["opacity"] = str(o.opacity)
            if o.transform is not None:
                attrs["transform"] = o.transform.matrix
            if o.clip is not None:
                c = o.clip
                clip: dict = {"x": c.x, "y": c.y, "w": c.w, "h": c.h}
                if c.border_radius != 0.0:
                    clip["borderRadius"] = c.border_radius
                attrs["clip"] = clip
        return {
            "type": "canvas-layer",
            "attrs": attrs,
            "nodes": [n.to_dict() for n in self._nodes],
        }
