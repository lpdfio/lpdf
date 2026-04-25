from __future__ import annotations

from ..shared.attrs_helper import node_to_dict
from .section_node import SectionNode


class Document:
    """The root document node, ready for LpdfEngine.render_pdf()."""

    __slots__ = ("_attrs", "_nodes")

    def __init__(self, attrs: dict, nodes: list[SectionNode]):
        self._attrs = attrs
        self._nodes = nodes

    def to_dict(self) -> dict:
        return {
            "version": 1,
            "type": "document",
            "attrs": self._attrs,
            "nodes": [n.to_dict() for n in self._nodes],
        }
