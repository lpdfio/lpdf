from __future__ import annotations

from abc import ABC, abstractmethod


class Node(ABC):
    """Abstract base class for all layout nodes."""

    @abstractmethod
    def to_dict(self) -> dict:
        ...
