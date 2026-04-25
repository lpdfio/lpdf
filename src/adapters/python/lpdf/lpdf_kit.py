from __future__ import annotations

from .kit.document import Document
from .kit.document_options import DocumentOptions
from .kit.section_node import SectionNode
from .kit.section_options import SectionOptions
from .kit.section_layout import SectionLayout
from .kit.section_canvas import SectionCanvas
from .shared.attrs_helper import options_to_attrs


def _document_attrs(options: DocumentOptions | None) -> dict:
    if options is None:
        return {}
    attrs: dict = {}
    for field in ("size", "orientation", "margin", "background"):
        val = getattr(options, field)
        if val is not None:
            attrs[field] = val
    if options.tokens is not None:
        attrs["tokens"] = options.tokens.to_dict()
    if options.meta is not None:
        attrs["meta"] = options.meta.to_dict()
    return attrs


class LpdfKit:
    """Factory for high-level document, section, layout, and canvas container nodes."""

    @staticmethod
    def document(
        sections: list[SectionNode] | None = None,
        options: DocumentOptions | None = None,
    ) -> Document:
        return Document(_document_attrs(options), sections or [])

    @staticmethod
    def section(
        nodes: list[SectionLayout | SectionCanvas] | None = None,
        options: SectionOptions | None = None,
    ) -> SectionNode:
        return SectionNode(options_to_attrs(options), nodes or [])

    @staticmethod
    def layout(nodes: list | None = None) -> SectionLayout:
        return SectionLayout(nodes or [])

    @staticmethod
    def canvas(layers: list | None = None) -> SectionCanvas:
        return SectionCanvas(layers or [])
