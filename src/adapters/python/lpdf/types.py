from __future__ import annotations

from dataclasses import dataclass, fields
from typing import Dict, List, Optional, Tuple


# ── Options ───────────────────────────────────────────────────────────────────

@dataclass(frozen=True)
class StackOptions:
    gap: Optional[str] = None
    padding: Optional[str] = None
    background: Optional[str] = None
    align: Optional[str] = None
    justify: Optional[str] = None
    width: Optional[str] = None
    height: Optional[str] = None
    border: Optional[str] = None
    radius: Optional[str] = None
    debug: Optional[str] = None


@dataclass(frozen=True)
class FlankOptions:
    gap: Optional[str] = None
    padding: Optional[str] = None
    background: Optional[str] = None
    align: Optional[str] = None
    justify: Optional[str] = None
    end: Optional[str] = None
    width: Optional[str] = None
    height: Optional[str] = None
    border: Optional[str] = None
    radius: Optional[str] = None
    debug: Optional[str] = None


@dataclass(frozen=True)
class SplitOptions:
    gap: Optional[str] = None
    padding: Optional[str] = None
    background: Optional[str] = None
    align: Optional[str] = None
    equal: Optional[str] = None
    width: Optional[str] = None
    height: Optional[str] = None
    border: Optional[str] = None
    radius: Optional[str] = None
    debug: Optional[str] = None


@dataclass(frozen=True)
class ClusterOptions:
    gap: Optional[str] = None
    padding: Optional[str] = None
    background: Optional[str] = None
    align: Optional[str] = None
    justify: Optional[str] = None
    width: Optional[str] = None
    height: Optional[str] = None
    border: Optional[str] = None
    radius: Optional[str] = None
    debug: Optional[str] = None


@dataclass(frozen=True)
class GridOptions:
    cols: Optional[str] = None
    col_width: Optional[str] = None
    gap: Optional[str] = None
    equal: Optional[str] = None
    padding: Optional[str] = None
    background: Optional[str] = None
    width: Optional[str] = None
    height: Optional[str] = None
    border: Optional[str] = None
    radius: Optional[str] = None
    debug: Optional[str] = None


@dataclass(frozen=True)
class FrameOptions:
    width: Optional[str] = None
    height: Optional[str] = None
    padding: Optional[str] = None
    background: Optional[str] = None
    border: Optional[str] = None
    radius: Optional[str] = None
    align: Optional[str] = None
    debug: Optional[str] = None


@dataclass(frozen=True)
class LinkOptions:
    url: Optional[str] = None
    width: Optional[str] = None
    height: Optional[str] = None


@dataclass(frozen=True)
class TextOptions:
    font: Optional[str] = None
    font_size: Optional[str] = None
    text_align: Optional[str] = None
    color: Optional[str] = None
    bold: Optional[str] = None
    end: Optional[str] = None
    repeat: Optional[str] = None
    width: Optional[str] = None
    height: Optional[str] = None
    padding: Optional[str] = None
    background: Optional[str] = None
    border: Optional[str] = None
    radius: Optional[str] = None


@dataclass(frozen=True)
class SpanOptions:
    font: Optional[str] = None
    font_size: Optional[str] = None
    color: Optional[str] = None
    bold: Optional[str] = None
    url: Optional[str] = None
    underline: Optional[str] = None
    strike: Optional[str] = None


@dataclass(frozen=True)
class DividerOptions:
    color: Optional[str] = None
    thickness: Optional[str] = None
    direction: Optional[str] = None


@dataclass(frozen=True)
class ImgOptions:
    name: str = ""
    height: Optional[str] = None
    width: Optional[str] = None
    font: Optional[str] = None
    font_size: Optional[str] = None
    gap: Optional[str] = None
    padding: Optional[str] = None
    background: Optional[str] = None
    border: Optional[str] = None
    radius: Optional[str] = None
    repeat: Optional[str] = None
    debug: Optional[str] = None


@dataclass(frozen=True)
class BarcodeOptions:
    type: str = ""
    data: str = ""
    size: Optional[str] = None
    width: Optional[str] = None
    height: Optional[str] = None
    ec: Optional[str] = None
    hrt: Optional[str] = None
    color: Optional[str] = None
    background: Optional[str] = None
    repeat: Optional[str] = None
    debug: Optional[str] = None


@dataclass(frozen=True)
class TableOptions:
    cols: Optional[str] = None
    border: Optional[str] = None
    stripe: Optional[str] = None
    gap: Optional[str] = None
    padding: Optional[str] = None
    background: Optional[str] = None
    width: Optional[str] = None
    height: Optional[str] = None
    repeat: Optional[str] = None
    debug: Optional[str] = None


@dataclass(frozen=True)
class TheadOptions:
    background: Optional[str] = None


@dataclass(frozen=True)
class TrOptions:
    background: Optional[str] = None


@dataclass(frozen=True)
class TdOptions:
    padding: Optional[str] = None
    background: Optional[str] = None
    align: Optional[str] = None
    valign: Optional[str] = None
    border: Optional[str] = None
    radius: Optional[str] = None
    gap: Optional[str] = None
    debug: Optional[str] = None


@dataclass(frozen=True)
class PageOptions:
    size: Optional[str] = None
    orientation: Optional[str] = None
    margin: Optional[str] = None
    background: Optional[str] = None
    debug: Optional[str] = None


# ── Tokens + Meta ─────────────────────────────────────────────────────────────

@dataclass(frozen=True)
class LpdfMeta:
    title: Optional[str] = None
    author: Optional[str] = None
    subject: Optional[str] = None
    keywords: Optional[str] = None
    creator: Optional[str] = None

    def to_dict(self) -> dict:
        return {f.name: getattr(self, f.name) for f in fields(self) if getattr(self, f.name) is not None}


@dataclass(frozen=True)
class LpdfTokens:
    colors: Optional[Dict[str, str]] = None
    space: Optional[Dict[str, str]] = None
    grid: Optional[Dict[str, str]] = None
    border: Optional[Dict[str, str]] = None
    radius: Optional[Dict[str, str]] = None
    width: Optional[Dict[str, str]] = None
    text: Optional[Dict[str, str]] = None
    fonts: Optional[Dict[str, dict]] = None

    def to_dict(self) -> dict:
        return {f.name: getattr(self, f.name) for f in fields(self) if getattr(self, f.name) is not None}


@dataclass(frozen=True)
class DocumentOptions:
    size: Optional[str] = None
    orientation: Optional[str] = None
    margin: Optional[str] = None
    background: Optional[str] = None
    tokens: Optional[LpdfTokens] = None
    meta: Optional[LpdfMeta] = None


# ── Nodes ─────────────────────────────────────────────────────────────────────

class LpdfContainerNode:
    __slots__ = ("_type", "_attrs", "_children")

    def __init__(self, type_: str, attrs: Dict[str, str], children: List):
        self._type = type_
        self._attrs = attrs
        self._children = children

    def to_dict(self) -> dict:
        return {
            "type": self._type,
            "attrs": self._attrs,
            "children": [_node_to_dict(c) for c in self._children],
        }


class LpdfPageNode:
    __slots__ = ("_attrs", "_children")

    def __init__(self, attrs: Dict[str, str], children: List):
        self._attrs = attrs
        self._children = children

    def to_dict(self) -> dict:
        page_children = (
            [{"type": "layout", "attrs": {}, "children": [_node_to_dict(c) for c in self._children]}]
            if self._children
            else []
        )
        return {
            "type": "page",
            "attrs": self._attrs,
            "children": page_children,
        }


class LpdfTextNode:
    __slots__ = ("_attrs", "_children")

    def __init__(self, attrs: Dict[str, str], children: List):
        self._attrs = attrs
        self._children = children

    def to_dict(self) -> dict:
        return {
            "type": "text",
            "attrs": self._attrs,
            "children": [_node_to_dict(c) for c in self._children],
        }


class LpdfSpanNode:
    __slots__ = ("_attrs", "_children")

    def __init__(self, attrs: Dict[str, str], children: List[str]):
        self._attrs = attrs
        self._children = children

    def to_dict(self) -> dict:
        return {
            "type": "span",
            "attrs": self._attrs,
            "children": self._children,
        }


class LpdfDividerNode:
    __slots__ = ("_attrs",)

    def __init__(self, attrs: Dict[str, str]):
        self._attrs = attrs

    def to_dict(self) -> dict:
        return {
            "type": "divider",
            "attrs": self._attrs,
        }


class LpdfImgNode:
    __slots__ = ("_attrs",)

    def __init__(self, attrs: Dict[str, str]):
        self._attrs = attrs

    def to_dict(self) -> dict:
        return {
            "type": "img",
            "attrs": self._attrs,
        }


class LpdfBarcodeNode:
    __slots__ = ("_attrs",)

    def __init__(self, attrs: Dict[str, str]):
        self._attrs = attrs

    def to_dict(self) -> dict:
        return {
            "type": "barcode",
            "attrs": self._attrs,
        }


class LpdfDocument:
    __slots__ = ("_attrs", "_children")

    def __init__(self, attrs: dict, children: List[LpdfPageNode]):
        self._attrs = attrs
        self._children = children

    def to_dict(self) -> dict:
        return {
            "version": 1,
            "type": "document",
            "attrs": self._attrs,
            "children": [c.to_dict() for c in self._children],
        }


# ── Helpers ───────────────────────────────────────────────────────────────────

def _node_to_dict(node):
    if isinstance(node, str):
        return node
    return node.to_dict()


def _snake_to_kebab(name: str) -> str:
    return name.replace("_", "-")


def options_to_attrs(options, *, skip: Tuple[str, ...] = ()) -> Dict[str, str]:
    if options is None:
        return {}
    attrs = {}
    for f in fields(options):
        if f.name in skip:
            continue
        value = getattr(options, f.name)
        if not isinstance(value, str):
            continue
        attrs[_snake_to_kebab(f.name)] = value
    return attrs
