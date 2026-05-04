"""Foretias v1 — Configuration.

.. deprecated:: Use ``foretias_p2p.PyTimeFamilyServer`` (Rust) instead.
"""

from __future__ import annotations

import warnings

warnings.warn(
    "foretias.config is deprecated. Use foretias (Rust-backed) instead.",
    DeprecationWarning,
    stacklevel=2,
)

import os
from dataclasses import dataclass


@dataclass(frozen=True)
class Config:
    """Persistence path resolution: arg > env var > default.

    Attributes:
        persist_path: Resolved path for calendar storage.
    """

    persist_path: str

    @classmethod
    def resolve(cls, persist_path: str | None = None) -> Config:
        """Resolve persist_path: argument overrides $FORETIAS_HOME overrides default.

        Resolution order:
            1. ``persist_path`` argument (highest priority)
            2. ``$FORETIAS_HOME`` environment variable
            3. Default: ``~/.foretias/``
        """
        if persist_path is not None:
            return cls(persist_path=persist_path)

        env = os.environ.get("FORETIAS_HOME")
        if env is not None:
            return cls(persist_path=env)

        return cls(persist_path=str(_default_path()))


def _default_path() -> str:
    """Return the default persistence path ~/.foretias/."""
    import pathlib

    return str(pathlib.Path.home() / ".foretias")
