"""Fortias v1 — Configuration."""

from __future__ import annotations

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
        """Resolve persist_path: argument overrides $FORTIAS_HOME overrides default.

        Resolution order:
            1. ``persist_path`` argument (highest priority)
            2. ``$FORTIAS_HOME`` environment variable
            3. Default: ``~/.fortias/``
        """
        if persist_path is not None:
            return cls(persist_path=persist_path)

        env = os.environ.get("FORTIAS_HOME")
        if env is not None:
            return cls(persist_path=env)

        return cls(persist_path=str(_default_path()))


def _default_path() -> str:
    """Return the default persistence path ~/.fortias/."""
    import pathlib

    return str(pathlib.Path.home() / ".fortias")
