"""
tomlplus — Rust-backed Python bindings for the TOML+ configuration format.

Public API is identical to the pure-Python `tomlplus` package; everything
underneath is now the Rust core via PyO3.
"""

from __future__ import annotations
from pathlib import Path
from typing import Any, Union

from . import _native as _n
from ._native import (
    TOMLPlusDocument,
    Annotation,
    TOMLPlusError,
    ParseError,
    ValidationError,
    VariableError,
)

# Python-package version (separate from the underlying Rust crate version).
__version__ = "2.0.0-rc.1"

__all__ = [
    "load",
    "loads",
    "load_validated",
    "loads_validated",
    "dumps",
    "validate",
    "validate_all",
    "TOMLPlusDocument",
    "Annotation",
    "TOMLPlusError",
    "ParseError",
    "ValidationError",
    "VariableError",
    "__version__",
]


def loads(source: str) -> TOMLPlusDocument:
    """Parse a TOML+ string."""
    return _n.parse(source)


def load(path: Union[str, Path]) -> TOMLPlusDocument:
    """Parse a TOML+ file."""
    return loads(Path(path).read_text(encoding="utf-8"))


def loads_validated(source: str) -> TOMLPlusDocument:
    """Parse and validate a TOML+ string."""
    doc = loads(source)
    _n.validate(doc)
    return doc


def load_validated(path: Union[str, Path]) -> TOMLPlusDocument:
    """Parse and validate a TOML+ file."""
    doc = load(path)
    _n.validate(doc)
    return doc


def validate(doc: TOMLPlusDocument) -> None:
    """Validate `doc` — raises `ValidationError` on the first failure."""
    _n.validate(doc)


def validate_all(doc: TOMLPlusDocument) -> list[ValidationError]:
    """Validate `doc` — returns all errors as a list."""
    return _n.validate_all(doc)


def dumps(data: Any) -> str:
    """Serialise a `TOMLPlusDocument` or a plain dict back to TOML+ text."""
    return _n.dumps(data)
