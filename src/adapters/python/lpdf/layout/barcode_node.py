from __future__ import annotations

from .node import Node


class BarcodeNode(Node):
    __slots__ = ("_attrs",)

    def __init__(self, attrs: dict[str, str]):
        self._attrs = attrs

    def to_dict(self) -> dict:
        return {
            "type": "barcode",
            "attrs": self._attrs,
        }
