from __future__ import annotations

from .shared.attrs_helper import options_to_attrs
from .layout.container_node import ContainerNode
from .layout.text_node import TextNode
from .layout.span_node import SpanNode
from .layout.divider_node import DividerNode
from .layout.img_node import ImgNode
from .layout.barcode_node import BarcodeNode
from .layout.field_node import FieldNode
from .layout.region_node import RegionNode
from .layout.pin import Pin
from .layout.field_type import FieldType
from .layout.stack_options import StackOptions
from .layout.flank_options import FlankOptions
from .layout.split_options import SplitOptions
from .layout.cluster_options import ClusterOptions
from .layout.grid_options import GridOptions
from .layout.frame_options import FrameOptions
from .layout.link_options import LinkOptions
from .layout.text_options import TextOptions
from .layout.span_options import SpanOptions
from .layout.divider_options import DividerOptions
from .layout.img_options import ImgOptions
from .layout.barcode_options import BarcodeOptions
from .layout.field_options import FieldOptions
from .layout.region_options import RegionOptions
from .layout.table_options import TableOptions
from .layout.thead_options import TheadOptions
from .layout.tr_options import TrOptions
from .layout.td_options import TdOptions


class LpdfLayout:
    """Static factory for all layout nodes."""

    # ── containers ──────────────────────────────────────────────────────────

    @staticmethod
    def stack(nodes: list | None = None, options: StackOptions | None = None) -> ContainerNode:
        return ContainerNode("stack", options_to_attrs(options), nodes or [])

    @staticmethod
    def flank(nodes: list | None = None, options: FlankOptions | None = None) -> ContainerNode:
        return ContainerNode("flank", options_to_attrs(options), nodes or [])

    @staticmethod
    def split(nodes: list | None = None, options: SplitOptions | None = None) -> ContainerNode:
        return ContainerNode("split", options_to_attrs(options), nodes or [])

    @staticmethod
    def cluster(nodes: list | None = None, options: ClusterOptions | None = None) -> ContainerNode:
        return ContainerNode("cluster", options_to_attrs(options), nodes or [])

    @staticmethod
    def grid(nodes: list | None = None, options: GridOptions | None = None) -> ContainerNode:
        return ContainerNode("grid", options_to_attrs(options), nodes or [])

    @staticmethod
    def frame(nodes: list | None = None, options: FrameOptions | None = None) -> ContainerNode:
        return ContainerNode("frame", options_to_attrs(options), nodes or [])

    @staticmethod
    def link(nodes: list | None = None, options: LinkOptions | None = None) -> ContainerNode:
        return ContainerNode("link", options_to_attrs(options), nodes or [])

    # ── table ────────────────────────────────────────────────────────────────

    @staticmethod
    def table(nodes: list | None = None, options: TableOptions | None = None) -> ContainerNode:
        return ContainerNode("table", options_to_attrs(options), nodes or [])

    @staticmethod
    def thead(nodes: list | None = None, options: TheadOptions | None = None) -> ContainerNode:
        return ContainerNode("thead", options_to_attrs(options), nodes or [])

    @staticmethod
    def tr(nodes: list | None = None, options: TrOptions | None = None) -> ContainerNode:
        return ContainerNode("tr", options_to_attrs(options), nodes or [])

    @staticmethod
    def td(nodes: list | None = None, options: TdOptions | None = None) -> ContainerNode:
        return ContainerNode("td", options_to_attrs(options), nodes or [])

    # ── text ─────────────────────────────────────────────────────────────────

    @staticmethod
    def text(nodes: list | None = None, options: TextOptions | None = None) -> TextNode:
        """nodes may be strings or SpanNode instances."""
        return TextNode(options_to_attrs(options), nodes or [])

    @staticmethod
    def span(nodes: list[str] | None = None, options: SpanOptions | None = None) -> SpanNode:
        """nodes must be plain strings."""
        return SpanNode(options_to_attrs(options), nodes or [])

    # ── leaves ───────────────────────────────────────────────────────────────

    @staticmethod
    def divider(options: DividerOptions | None = None) -> DividerNode:
        return DividerNode(options_to_attrs(options))

    @staticmethod
    def img(options: ImgOptions | None = None) -> ImgNode:
        return ImgNode(options_to_attrs(options))

    @staticmethod
    def barcode(options: BarcodeOptions | None = None) -> BarcodeNode:
        return BarcodeNode(options_to_attrs(options))

    @staticmethod
    def field(
        field_type: str | FieldType,
        name: str,
        options: FieldOptions | None = None,
    ) -> FieldNode:
        attrs: dict[str, str] = {"type": str(field_type), "name": name}
        attrs.update(options_to_attrs(options))
        return FieldNode(attrs)

    @staticmethod
    def region(
        pin: str | Pin,
        nodes: list | None = None,
        options: RegionOptions | None = None,
    ) -> RegionNode:
        attrs: dict[str, str] = {"pin": str(pin)}
        attrs.update(options_to_attrs(options))
        return RegionNode(attrs, nodes or [])
