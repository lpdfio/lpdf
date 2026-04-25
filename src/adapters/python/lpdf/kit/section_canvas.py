from __future__ import annotations


class SectionCanvas:
    """A canvas block inside a section — wraps canvas layers."""

    __slots__ = ("_layers",)

    def __init__(self, layers: list):
        self._layers = layers

    def to_dict(self) -> dict:
        return {
            "type": "canvas",
            "nodes": [layer.to_dict() for layer in self._layers],
        }
