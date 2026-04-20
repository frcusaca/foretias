"""
Fortias Protocol v0.0.1 — Verification calendar.

The :class:`Calendar` is an in-memory, monotonically ordered store of
``(TimestampV0, verifier_bytes)`` ticks.  Each tick publishes an Ed25519
public key that is authoritative from ``calendar_timestamp`` onward (until
the next tick supersedes it).

Monotonic insertion policy
--------------------------
:meth:`tick` enforces strictly increasing timestamps.  A timestamp equal to
or earlier than the current head is rejected.  Because the list is always
fully sorted in ascending order, lookups use binary search (O(log n)) with
no extra bookkeeping.

All data lives in process memory and is not persisted across restarts.

Lookup / cutoff policy
----------------------
:meth:`get_tick` returns the *greatest* tick whose timestamp is **less than
or equal to** the query timestamp (``≤``).  This is the "latest-key-at-time"
policy.

Example::

    # Tick published at exactly 00:05:00
    cal.tick(TS(2026, 1, 1, 0, 5, 0), new_key)

    # A stamp issued at 00:05:30 — still within the 5-minute window
    cal.get_tick(TS(2026, 1, 1, 0, 5, 30))
    # → returns the 00:05:00 tick (key is valid)

    # A stamp issued at 00:06:00 — clock has turned the next minute
    cal.get_tick(TS(2026, 1, 1, 0, 6, 0))
    # → returns the NEXT tick (00:06:00), not the 00:05:00 one.
    #   The 00:05:00 key is no longer authoritative.

This policy means that key rotations are always discoverable: a verifier
asking "what key was valid at T?" gets the key that was active *at that
exact moment*, and as soon as a new tick supersedes it, old stamps signed
under the old key will correctly fail verification.
"""

from __future__ import annotations

import uuid
from bisect import bisect_right
from datetime import datetime, timezone

from .crypto import PROTOCOL_VERSION
from .models import VerificationCalendar, VerificationCalendarResponse
from .timestamp import TimestampFactoryV0, TimestampV0


class Calendar:
    """In-memory store of ``(TimestampV0, verifier)`` ticks in ascending order.

    Each entry pairs a :class:`~fortias.timestamp.TimestampV0` with the raw
    32-byte Ed25519 public key that was authoritative from that moment.

    The list is maintained in strictly ascending timestamp order by enforcing
    monotonicity on :meth:`tick`.  This allows O(log n) binary search for
    lookups.
    """

    def __init__(self) -> None:
        self._ticks: list[tuple[TimestampV0, bytes]] = []

    # ------------------------------------------------------------------
    # Tick interface
    # ------------------------------------------------------------------

    def tick(self, timestamp: TimestampV0, verifier: bytes) -> None:
        """Append ``(timestamp, verifier)`` iff *timestamp* is the new maximum.

        Enforces monotonicity: the calendar is an ever-advancing sequence.
        A timestamp equal to or earlier than the current head is rejected.

        Because the list is always sorted in ascending order, the last
        element is always the maximum timestamp.

        Args:
            timestamp: Must be strictly greater than every previously
                       inserted timestamp.
            verifier:  Raw 32-byte Ed25519 public key authoritative from
                       *timestamp* onward.

        Raises:
            :class:`ValueError`: if *timestamp* is not strictly greater than
                                 the current maximum.
        """
        if self._ticks and timestamp <= self._ticks[-1][0]:
            raise ValueError(
                f"tick() requires a strictly increasing timestamp.  "
                f"Received {timestamp!r}, current maximum is {self._ticks[-1][0]!r}."
            )
        self._ticks.append((timestamp, verifier))

    def get_tick(
        self, timestamp: TimestampV0
    ) -> tuple[TimestampV0, bytes] | None:
        """Return the greatest tick with ``tick_ts ≤ timestamp``, or ``None``.

        O(log n) binary search on the sorted underlying list.

        This implements the **latest-key-at-time** policy: given a query
        timestamp, return the most recent tick whose timestamp is less than
        or equal to the query.  This is the key that was authoritative at
        the query moment.

        Args:
            timestamp: Query timestamp.

        Returns:
            The ``(tick_timestamp, verifier)`` pair for the greatest tick
            whose timestamp is ``≤`` *timestamp*, or ``None`` if no such
            tick exists.
        """
        if not self._ticks:
            return None
        idx = bisect_right(self._ticks, timestamp, key=lambda e: e[0])
        if idx == 0:
            return None
        return self._ticks[idx - 1]

    # ------------------------------------------------------------------
    # Verification calendar
    # ------------------------------------------------------------------

    def retrieve_for_verification(
        self,
        query_timestamp: TimestampV0,
        tbid: str,
    ) -> VerificationCalendarResponse:
        """Look up the verification calendar entry for *query_timestamp*.

        Finds the greatest tick at-or-before *query_timestamp* and packages
        it into a :class:`~fortias.models.VerificationCalendarResponse`.

        The TBID is provided by the caller (the TimeBeing service object)
        so that each verification lookup has a unique correlation id.

        Args:
            query_timestamp: The timestamp to look up (typically a stamp's
                             ``fortias_timestamp`` parsed into a
                             :class:`~fortias.timestamp.TimestampV0`).
            tbid:            UUID4 correlation id from the service layer.

        Returns:
            A :class:`~fortias.models.VerificationCalendarResponse` whose
            ``verification_calendar`` field carries the located
            :class:`~fortias.models.VerificationCalendar`, or ``None`` if
            no tick at-or-before *query_timestamp* exists.
        """
        hit = self.get_tick(query_timestamp)
        vcal: VerificationCalendar | None = None
        if hit is not None:
            tick_ts, verifier = hit
            vcal = VerificationCalendar(
                calendar_timestamp=tick_ts,
                verifier=verifier,
            )
        return VerificationCalendarResponse(
            TBID=tbid,
            fortias_version=PROTOCOL_VERSION,
            fortias_timestamp=TimestampFactoryV0.from_datetime(
                datetime.now(tz=timezone.utc)
            ).to_iso_string(),
            verification_calendar=vcal,
        )
