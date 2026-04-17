from __future__ import annotations

from dataclasses import dataclass
from typing import Dict, Optional


@dataclass(frozen=True)
class RenderOptions:
    wasm_binary: Optional[str] = None
    wasm_runner: Optional[str] = None
    created_on: Optional[str] = None
    font_bytes: Optional[Dict[str, bytes]] = None
    image_bytes: Optional[Dict[str, bytes]] = None
