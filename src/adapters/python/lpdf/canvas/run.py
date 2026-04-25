from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class Run:
    """A rich-text run for CanvasTextNode."""
    text: str
    font: str | None = None
    size: float | None = None
    color: str | None = None

    def to_dict(self) -> dict:
        attrs: dict = {}
        if self.font is not None:
            attrs["font"] = self.font
        if self.size is not None:
            attrs["font-size"] = str(self.size)
        if self.color is not None:
            attrs["color"] = self.color
        return {"text": self.text, "attrs": attrs}
