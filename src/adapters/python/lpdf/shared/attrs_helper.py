from __future__ import annotations

from dataclasses import fields
from typing import Any


def _snake_to_kebab(name: str) -> str:
    return name.replace("_", "-")


def options_to_attrs(options: Any, *, skip: tuple[str, ...] = ()) -> dict[str, str]:
    """Convert a frozen dataclass of options into a flat dict of kebab-case string attrs."""
    if options is None:
        return {}
    attrs: dict[str, str] = {}
    for f in fields(options):
        if f.name in skip:
            continue
        value = getattr(options, f.name)
        if isinstance(value, bool):
            attrs[_snake_to_kebab(f.name)] = "true" if value else "false"
        elif isinstance(value, str):
            attrs[_snake_to_kebab(f.name)] = value
    return attrs


def node_to_dict(node: Any) -> Any:
    """Convert a node (str or object with to_dict()) to a serialisable value."""
    if isinstance(node, str):
        return node
    return node.to_dict()
