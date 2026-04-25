from __future__ import annotations

from dataclasses import dataclass

from .line_cap import LineCap
from .line_join import LineJoin


@dataclass(frozen=True)
class RectStyle:
    fill: str | None = None
    stroke: str | None = None
    stroke_width: float | None = None
    stroke_dash: list[float] | None = None
    border_radius: float | None = None


@dataclass(frozen=True)
class LineStyle:
    stroke: str | None = None
    stroke_width: float | None = None
    stroke_dash: list[float] | None = None
    line_cap: LineCap | None = None
    line_join: LineJoin | None = None


@dataclass(frozen=True)
class EllipseStyle:
    fill: str | None = None
    stroke: str | None = None
    stroke_width: float | None = None
    stroke_dash: list[float] | None = None


@dataclass(frozen=True)
class PathStyle:
    fill: str | None = None
    stroke: str | None = None
    stroke_width: float | None = None
    stroke_dash: list[float] | None = None
    fill_rule_evenodd: bool | None = None
    line_cap: LineCap | None = None
    line_join: LineJoin | None = None
