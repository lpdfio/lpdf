from __future__ import annotations

from ..shared.attrs_helper import node_to_dict
from .node import Node


class TextNode(Node):
    __slots__ = ("_attrs", "_nodes")

    def __init__(self, attrs: dict[str, str], nodes: list):
        self._attrs = attrs
        self._nodes = nodes

    def to_dict(self) -> dict:
        return {
            "type": "text",
            "attrs": self._attrs,
            "nodes": [node_to_dict(n) for n in self._nodes],
        }
