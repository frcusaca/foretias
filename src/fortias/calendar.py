"""
Fortias Protocol v0.0.1 — Verification calendar.

The :class:`Calendar` is an in-memory, monotonically ordered store of
``(TimestampV0, verifier_bytes)`` ticks.  Each tick publishes an Ed25519
public key that is authoritative from ``calendar_timestamp`` onward (until
the next tick supersedes it).

Because :meth:`tick` enforces strictly increasing timestamps and
:class:`~fortias.timestamp.TimestampV0` compares as a single ``uint64``,
the underlying list is always fully sorted and lookups use binary search
(O(log n)) with no extra bookkeeping.

All data lives in process memory and is not persisted across restarts.
"""

from __future__ import annotations

import heapq
import uuid
from bisect import bisect_right
from datetime import datetime, timezone
from typing import Optional

from .crypto import PROTOCOL_VERSION
from .models import VerificationCalendar, VerificationCalendarResponse
from .timestamp import TimestampV0


class Calendar:
    """In-memory store of ``(TimestampV0, verifier)`` ticks in ascending order.

    Each entry pairs a :class:`~fortias.timestamp.TimestampV0` with the raw
    32-byte Ed25519 public key that was authoritative from that moment.
    """

    def __init__(self) -> None:
        self._heap: list[tuple[TimestampV0, bytes]] = []

    # ------------------------------------------------------------------
    # Tick interface
    # ------------------------------------------------------------------

    def tick(self, timestamp: TimestampV0, verifier: bytes) -> None:
        """Append ``(timestamp, verifier)`` iff *timestamp* is the new maximum.

        Enforces monotonicity: the calendar is an ever-advancing sequence.
        A timestamp equal to or earlier than the current head is rejected.

        Args:
            timestamp: Must be strictly greater than every previously
                       inserted timestamp.
            verifier:  Raw 32-byte Ed25519 public key authoritative from
                       *timestamp* onward.

        Raises:
            :class:`ValueError`: if *timestamp* is not strictly greater than
                                 the current maximum.
        """
        if self._heap and timestamp <= self._heap[-1][0]:
            raise ValueError(
                f"tick() requires a strictly increasing timestamp.  "
                f"Received {timestamp!r}, current maximum is {self._heap[-1][0]!r}."
            )
        heapq.heappush(self._heap, (timestamp, verifier))

    def get_tick(
        self, timestamp: TimestampV0
    ) -> Optional[tuple[TimestampV0, bytes]]:
        """Return the greatest tick with ``tick_ts ≤ timestamp``, or ``None``.

        O(log n) binary search on the sorted underlying list.

        Args:
            timestamp: Query timestamp.

        Returns:
            The ``(tick_timestamp, verifier)`` pair for the greatest tick
            whose timestamp is ``≤`` *timestamp*, or ``None`` if no such
            tick exists.
        """
        if not self._heap:
            return None
        idx = bisect_right(self._heap, timestamp, key=lambda e: e[0])
        if idx == 0:
            return None
        return self._heap[idx - 1]

    # ------------------------------------------------------------------
    # Verification calendar
    # ------------------------------------------------------------------

    def retrieve_for_verification(
        self, query_timestamp: TimestampV0
    ) -> VerificationCalendarResponse:
        """Look up the verification calendar entry for *query_timestamp*.

        Finds the greatest tick at-or-before *query_timestamp* and packages
        it into a :class:`~fortias.models.VerificationCalendarResponse`.

        Args:
            query_timestamp: The timestamp to look up (typically a stamp's
                             ``fortias_timestamp`` parsed into a
                             :class:`~fortias.timestamp.TimestampV0`).

        Returns:
            A :class:`~fortias.models.VerificationCalendarResponse` whose
            ``verification_calendar`` field carries the located
            :class:`~fortias.models.VerificationCalendar`, or ``None`` if
            no tick at-or-before *query_timestamp* exists.
        """
        hit = self.get_tick(query_timestamp)
        vcal = None
        if hit is not None:
            tick_ts, verifier = hit
            vcal = VerificationCalendar(
                calendar_timestamp=tick_ts,
                verifier=verifier,
            )
        return VerificationCalendarResponse(
            TBID=str(uuid.uuid4()),
            fortias_version=PROTOCOL_VERSION,
            fortias_timestamp=datetime.now(tz=timezone.utc).isoformat(
                timespec="microseconds"
            ),
            verification_calendar=vcal,
        )
