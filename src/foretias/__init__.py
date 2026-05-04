"""Foretias — Free, Open-source and Resilient Time Integrity Attestation Service."""
from __future__ import annotations

try:
    from ._version import __version__
except ImportError:
    __version__ = "0.0.0+unknown"

# Import from Rust bindings
from foretias_p2p import (
    PyTimeFamilyServer as TimeFamilyServer,
    PyTimeFamily as TimeFamily,
    PyCryptoServer as CryptoServer,
    PyForetis as Foretis,
    PyTickRecord as TickRecord,
    PyCalendar as Calendar,
)

__all__ = [
    "TimeFamilyServer",
    "TimeFamily",
    "CryptoServer",
    "Foretis",
    "TickRecord",
    "Calendar",
    "__version__",
]
