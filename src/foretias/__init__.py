"""Foretias — thin shim over foretias-p2p (Rust PyO3 bindings).

All protocol logic lives in the Rust ``foretias_p2p`` crate.
This package re-exports the public types with Python-friendly names.
"""
from __future__ import annotations

from foretias_p2p import (
    PyTimeFamilyServer as TimeFamilyServer,
    PyTimeFamily as TimeFamily,
    PyCryptoServer as CryptoServer,
    PyForetis as Foretis,
    PyTickRecord as TickRecord,
    PyCalendar as Calendar,
    PyExternalAttestation as ExternalAttestation,
    PyPeerScore as PeerScore,
    PyEpochSnapshot as EpochSnapshot,
    PySealedBlob as SealedBlob,
    PyCollisionConfig as CollisionConfig,
    PyNodeConfig as NodeConfig,
    PyHeartbeat as Heartbeat,
    PyProbityReport as ProbityReport,
    PyJsonRpcError as JsonRpcError,
    PyCalendarBlock as CalendarBlock,
)

__version__ = "0.2.0"

__all__ = [
    "TimeFamilyServer",
    "TimeFamily",
    "CryptoServer",
    "Foretis",
    "TickRecord",
    "Calendar",
    "ExternalAttestation",
    "PeerScore",
    "EpochSnapshot",
    "SealedBlob",
    "CollisionConfig",
    "NodeConfig",
    "Heartbeat",
    "ProbityReport",
    "JsonRpcError",
    "CalendarBlock",
    "__version__",
]
