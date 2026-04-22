"""Fortias — Free, Open-source and Resilient Time Integrity Attestation Service."""

from __future__ import annotations

try:
    from ._version import __version__
except ImportError:
    __version__ = "0.0.0+unknown"

from .config import Config
from .models import Fortis, TickRecord
from .timebeing import Timebeing
from .chronomatter import (
    CalendarInterface,
    ChronomatterInterface,
    InquirerInterface,
    Inquirer,
    ChronomatterV1,
    ChronomatterV1Serial,
)
from .time_family import TimeFamily

__all__ = [
    "TimeFamily",
    "Timebeing",
    "CalendarInterface",
    "ChronomatterInterface",
    "InquirerInterface",
    "Inquirer",
    "ChronomatterV1",
    "ChronomatterV1Serial",
    "Fortis",
    "TickRecord",
    "Config",
    "__version__",
]
