from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class FieldOptions:
    label: str | None = None
    value: str | None = None
    options: str | None = None
    group: str | None = None
    checked: bool | None = None
    required: bool | None = None
    readonly: bool | None = None
    max_len: str | None = None
    action_url: str | None = None
    width: str | None = None
    height: str | None = None
    debug: bool | None = None
    data_value: str | None = None
    data_source: str | None = None
    data_if: str | None = None
    data_if_not: str | None = None
