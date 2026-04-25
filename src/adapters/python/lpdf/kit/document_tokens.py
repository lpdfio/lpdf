from __future__ import annotations

from dataclasses import dataclass, fields as dc_fields
from typing import Any


@dataclass(frozen=True)
class DocumentTokens:
    colors: dict[str, str] | None = None
    space: dict[str, str] | None = None
    grid: dict[str, str] | None = None
    border: dict[str, str] | None = None
    radius: dict[str, str] | None = None
    width: dict[str, str] | None = None
    text: dict[str, str] | None = None
    fonts: dict[str, Any] | None = None

    def to_dict(self) -> dict:
        return {f.name: getattr(self, f.name) for f in dc_fields(self) if getattr(self, f.name) is not None}
