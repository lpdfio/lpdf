from __future__ import annotations

from .canvas_node import CanvasNode


class ImageNode(CanvasNode):
    __slots__ = ("_x", "_y", "_w", "_h", "_name")

    def __init__(self, x: float, y: float, w: float, h: float, name: str):
        self._x = x
        self._y = y
        self._w = w
        self._h = h
        self._name = name

    def to_dict(self) -> dict:
        return {
            "type": "canvas-img",
            "attrs": {
                "x": str(self._x), "y": str(self._y),
                "w": str(self._w), "h": str(self._h),
                "name": self._name,
            },
        }
