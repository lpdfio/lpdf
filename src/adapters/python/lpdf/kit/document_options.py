from __future__ import annotations

from dataclasses import dataclass

from .document_meta import DocumentMeta
from .document_tokens import DocumentTokens


@dataclass(frozen=True)
class DocumentOptions:
    size: str | None = None
    orientation: str | None = None
    margin: str | None = None
    background: str | None = None
    tokens: DocumentTokens | None = None
    meta: DocumentMeta | None = None
