from __future__ import annotations

from .node import Node


class SpanNode(Node):
    __slots__ = ("_attrs", "_nodes")

    def __init__(self, attrs: dict[str, str], nodes: list[str]):
        self._attrs = attrs
        self._nodes = nodes

    def to_dict(self) -> dict:
        return {
            "type": "span",
            "attrs": self._attrs,
            "nodes": self._nodes,
        }
