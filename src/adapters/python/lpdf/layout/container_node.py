from __future__ import annotations

from ..shared.attrs_helper import node_to_dict
from .node import Node


class ContainerNode(Node):
    __slots__ = ("_type", "_attrs", "_nodes")

    def __init__(self, type_: str, attrs: dict[str, str], nodes: list):
        self._type = type_
        self._attrs = attrs
        self._nodes = nodes

    def to_dict(self) -> dict:
        return {
            "type": self._type,
            "attrs": self._attrs,
            "nodes": [node_to_dict(n) for n in self._nodes],
        }
