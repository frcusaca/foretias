"""
Fortias Protocol v0.0.1 — Verification calendar.

The :class:`Calendar` is an in-memory store that maps time periods to public
keys using an interval tree, allowing efficient lookup of whichever key was
valid at any given point in time.  All data lives in process memory and is
not persisted across restarts.
"""

from __future__ import annotations

from datetime import datetime, timezone
from typing import TYPE_CHECKING, Optional

from intervaltree import Interval, IntervalTree

if TYPE_CHECKING:
    from .models import VerificationCalendar, VerifyRequest


def _to_epoch(timestamp: str) -> float:
    """Parse an ISO-8601 string to a UTC POSIX timestamp (float seconds)."""
    dt = datetime.fromisoformat(timestamp)
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt.timestamp()


class Calendar:
    """In-memory store of public keys indexed by validity period.

    Uses an interval tree so that :meth:`retrieve` is O(log n + k) rather
    than a linear scan.  Each entry maps a half-open interval
    ``[period_start, period_end)`` of UTC time to a public key (raw bytes).
    When multiple intervals overlap at the same point, the most-recently
    opened validity window (latest ``period_start``) is returned.
    """

    def __init__(self) -> None:
        self._tree: IntervalTree = IntervalTree()

    # ------------------------------------------------------------------
    # Storage interface
    # ------------------------------------------------------------------

    def store(self, period_start: str, period_end: str, public_key: bytes) -> None:
        """Store a *public_key* valid over ``[period_start, period_end)``.

        Args:
            period_start: ISO-8601 UTC string marking the start of the
                          validity window (inclusive).
            period_end:   ISO-8601 UTC string marking the end of the
                          validity window (exclusive).
            public_key:   Raw public-key bytes to associate with this period.

        Raises:
            :class:`ValueError`: if *period_start* >= *period_end*, or if
                                 either string is not a valid ISO-8601 timestamp.
        """
        start = _to_epoch(period_start)
        end = _to_epoch(period_end)
        if start >= end:
            raise ValueError(
                f"period_start must be strictly before period_end "
                f"({period_start!r} >= {period_end!r})"
            )
        self._tree.addi(start, end, public_key)

    def retrieve(self, timestamp: str) -> Optional[bytes]:
        """Return the public key valid at *timestamp*, or ``None``.

        If multiple stored intervals overlap *timestamp*, the key from the
        interval with the latest start time is returned (i.e. the most
        recently opened validity window wins).

        Args:
            timestamp: ISO-8601 UTC string representing the point in time to
                       query.

        Returns:
            The matching public key bytes, or ``None`` if no interval covers
            *timestamp*.
        """
        point = _to_epoch(timestamp)
        matches: set[Interval] = self._tree.at(point)
        if not matches:
            return None
        best: Interval = max(matches, key=lambda iv: iv.begin)
        return best.data

    # ------------------------------------------------------------------
    # Verification calendar
    # ------------------------------------------------------------------

    def retrieve_calendar_for_verification(
        self, request: VerifyRequest
    ) -> VerificationCalendar:
        """Internal method for retrieving relevant calendar information for
        verifying the request specified.

        Queries this in-memory calendar for the public key valid at the time
        recorded in *request.stamp.issued_at*, and packages it alongside the
        corresponding validity window into a
        :class:`~fortias.models.VerificationCalendarV0`.

        This is an internal call consumed by
        :meth:`~fortias.protocol.Fortias.verify`; external callers should
        use that method rather than calling this directly.

        Args:
            request: The :class:`~fortias.models.VerifyRequest` whose calendar
                     context should be retrieved.

        Returns:
            A :class:`~fortias.models.VerificationCalendar` (concretely a
            :class:`~fortias.models.VerificationCalendarV0` for protocol
            version ``"0.0.1"``) containing the validity window and the
            public key active at the stamp's issue time.
        """
        ...
