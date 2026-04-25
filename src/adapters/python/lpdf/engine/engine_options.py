from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class EngineOptions:
    """Construction-time configuration for LpdfEngine."""
    wasm_binary: str | None = None
    wasm_runner: str | None = None
    timeout: int | None = None
