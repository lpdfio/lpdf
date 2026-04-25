from __future__ import annotations

from dataclasses import dataclass, fields as dc_fields


@dataclass(frozen=True)
class DocumentMeta:
    title: str | None = None
    author: str | None = None
    subject: str | None = None
    keywords: str | None = None
    creator: str | None = None

    def to_dict(self) -> dict:
        return {f.name: getattr(self, f.name) for f in dc_fields(self) if getattr(self, f.name) is not None}
