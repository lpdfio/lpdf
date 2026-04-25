from __future__ import annotations

from ..shared.attrs_helper import node_to_dict


class SectionLayout:
    """A layout block inside a section — wraps layout nodes."""

    __slots__ = ("_nodes",)

    def __init__(self, nodes: list):
        self._nodes = nodes

    def to_dict(self) -> dict:
        return {
            "type": "layout",
            "nodes": [node_to_dict(n) for n in self._nodes],
        }
