from __future__ import annotations


class SectionNode:
    """A section node. Children are SectionLayout and/or SectionCanvas in paint order."""

    __slots__ = ("_attrs", "_nodes")

    def __init__(self, attrs: dict[str, str], nodes: list):
        self._attrs = attrs
        self._nodes = nodes

    def to_dict(self) -> dict:
        return {
            "type": "section",
            "attrs": self._attrs,
            "nodes": [n.to_dict() for n in self._nodes],
        }
