from __future__ import annotations

from .types import (
    ClusterOptions,
    DividerOptions,
    DocumentOptions,
    FlankOptions,
    FrameOptions,
    GridOptions,
    ImgOptions,
    BarcodeOptions,
    LinkOptions,
    LpdfContainerNode,
    LpdfDividerNode,
    LpdfDocument,
    LpdfImgNode,
    LpdfBarcodeNode,
    LpdfPageNode,
    LpdfSpanNode,
    LpdfTextNode,
    PageOptions,
    SpanOptions,
    SplitOptions,
    StackOptions,
    TableOptions,
    TdOptions,
    TextOptions,
    TheadOptions,
    TrOptions,
    options_to_attrs,
)


# ── Container helpers ─────────────────────────────────────────────────────────

def stack(nodes: list | None = None, options: StackOptions | None = None) -> LpdfContainerNode:
    return LpdfContainerNode("stack", options_to_attrs(options), nodes or [])


def flank(nodes: list | None = None, options: FlankOptions | None = None) -> LpdfContainerNode:
    return LpdfContainerNode("flank", options_to_attrs(options), nodes or [])


def split(nodes: list | None = None, options: SplitOptions | None = None) -> LpdfContainerNode:
    return LpdfContainerNode("split", options_to_attrs(options), nodes or [])


def cluster(nodes: list | None = None, options: ClusterOptions | None = None) -> LpdfContainerNode:
    return LpdfContainerNode("cluster", options_to_attrs(options), nodes or [])


def grid(nodes: list | None = None, options: GridOptions | None = None) -> LpdfContainerNode:
    return LpdfContainerNode("grid", options_to_attrs(options), nodes or [])


def frame(nodes: list | None = None, options: FrameOptions | None = None) -> LpdfContainerNode:
    return LpdfContainerNode("frame", options_to_attrs(options), nodes or [])


def link(nodes: list | None = None, options: LinkOptions | None = None) -> LpdfContainerNode:
    return LpdfContainerNode("link", options_to_attrs(options), nodes or [])


# ── Leaf helpers ──────────────────────────────────────────────────────────────

def text(nodes: list | None = None, options: TextOptions | None = None) -> LpdfTextNode:
    children = nodes or []
    for i, child in enumerate(children):
        if not isinstance(child, (str, LpdfSpanNode)):
            raise TypeError(
                f"text() child at index {i} must be a string or LpdfSpanNode, got {type(child).__name__}"
            )
    return LpdfTextNode(options_to_attrs(options), children)


def span(nodes: list | None = None, options: SpanOptions | None = None) -> LpdfSpanNode:
    children = nodes or []
    for i, child in enumerate(children):
        if not isinstance(child, str):
            raise TypeError(
                f"span() child at index {i} must be a string, got {type(child).__name__}"
            )
    return LpdfSpanNode(options_to_attrs(options), children)


def divider(options: DividerOptions | None = None) -> LpdfDividerNode:
    return LpdfDividerNode(options_to_attrs(options))


def img(options: ImgOptions) -> LpdfImgNode:
    return LpdfImgNode(options_to_attrs(options))


def barcode(options: BarcodeOptions) -> LpdfBarcodeNode:
    return LpdfBarcodeNode(options_to_attrs(options))


# ── Table helpers ─────────────────────────────────────────────────────────────

def table(nodes: list | None = None, options: TableOptions | None = None) -> LpdfContainerNode:
    return LpdfContainerNode("table", options_to_attrs(options), nodes or [])


def thead(nodes: list | None = None, options: TheadOptions | None = None) -> LpdfContainerNode:
    return LpdfContainerNode("thead", options_to_attrs(options), nodes or [])


def tr(nodes: list | None = None, options: TrOptions | None = None) -> LpdfContainerNode:
    return LpdfContainerNode("tr", options_to_attrs(options), nodes or [])


def td(nodes: list | None = None, options: TdOptions | None = None) -> LpdfContainerNode:
    return LpdfContainerNode("td", options_to_attrs(options), nodes or [])


# ── Page + document ───────────────────────────────────────────────────────────

def page(nodes: list | None = None, options: PageOptions | None = None) -> LpdfPageNode:
    return LpdfPageNode(options_to_attrs(options), nodes or [])


def document(nodes: list | None = None, options: DocumentOptions | None = None) -> LpdfDocument:
    attrs = options_to_attrs(options, skip=("tokens", "meta"))
    if options is not None:
        if options.tokens is not None:
            attrs["tokens"] = options.tokens.to_dict()
        if options.meta is not None:
            attrs["meta"] = options.meta.to_dict()
    return LpdfDocument(attrs, nodes or [])
