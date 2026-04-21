"""Fortias — Free, Open-source and Resilient Time Integrity Attestation Service."""

from __future__ import annotations

try:
    from ._version import __version__
except ImportError:
    __version__ = "0.0.0+unknown"

from .config import Config
from .models import Fortis, TickRecord
from .stamper import Chronomatter
from .time_family import TimeFamily

__all__ = ["TimeFamily", "Chronomatter", "Fortis", "TickRecord", "Config", "__version__"]
