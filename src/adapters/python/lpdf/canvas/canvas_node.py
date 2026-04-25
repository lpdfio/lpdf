from __future__ import annotations

from abc import ABC, abstractmethod


class CanvasNode(ABC):
    """Abstract base class for all canvas nodes."""

    @abstractmethod
    def to_dict(self) -> dict:
        ...
