"""Fortias v1 — Chronomatter (Chronos authenticus): the Time Authority.

Manages Ed25519 key lifecycle, stamping, ticking, and publishes ticks
to all attached Calendars.  Pure functional operations are delegated
to :class:`~fortias._timebeing._timebeing`.
"""

from __future__ import annotations

import threading
import uuid

from ._timebeing import _timebeing, _genesis_ma, _now_ns
from .calendar import Calendar
from .config import Config
from .crypto import generate_keypair, sign, sha256, verify
from .models import Fortis, TickRecord

def _monotonic_ns() -> float:
    """Return the current value of the monotonic clock in nanoseconds."""
    import time

    return time.monotonic_ns()


class Chronomatter:
    """Chronos authenticus — The Time Authority.

    Manages Ed25519 key lifecycle (genesis to tick rotation), performs
    stamping and ticking, and publishes ticks to all attached Calendars.

    Args:
        name: TBN prefix — final name becomes ``"Time Being {tbid.hex()}"``.
        chronon_ns: Fixed tick interval in nanoseconds.
        tbid: Internal identity (UUID v4 bytes). Auto-generated if None.
        serialized: If True, tick advances only on chronomatter (throttled).
        persist_path: Disk path for Chronomatter state persistence.

    Attributes:
        tbid: The chronomatter's opaque internal identity.
        tbn: Human-readable external name.
        serialized: Whether the chronomatter computes sparse ticks.
        chronon_ns: Tick interval in nanoseconds.
        active: True if this instance holds the current private key.
    """

    def __init__(
        self,
        name: str = "timebeing",
        chronon_ns: float = 60_000_000_000.0,
        tbid: bytes | None = None,
        serialized: bool = False,
        persist_path: str | None = None,
    ) -> None:
        self._chronon_ns = chronon_ns
        self._serialized = serialized
        self._tbid = tbid or uuid.uuid4().bytes
        self._tbn = f"Time Being {self._tbid.hex()}"
        self._config = Config.resolve(persist_path=persist_path)
        self._calendars: list[Calendar] = []
        self._rlock = threading.RLock()
        # Absolute monotonic time of the next scheduled tick.
        self._next_tick_time_ns: float | None = None
        self._last_tick_wall_ns: float = _monotonic_ns()

        # Create Chronomatter's own genesis (uses Chronomatter.tbid).
        sk, pk = generate_keypair()
        genesis_ma = _genesis_ma(self._tbid, pk)
        genesis_forward = sign(genesis_ma, sk)
        genesis_backward = sign(genesis_ma, sk)
        self._genesis = TickRecord(
            tick_number=0,
            public_key=pk,
            forward_fortis=genesis_forward,
            backward_fortis=genesis_backward,
        )
        self._current_sk = sk
        self._current_pk = pk
        self._active = True
        self._ticks: list[TickRecord] = [self._genesis]
        # Chronomatter uses its own sequential counter for stamping/verification,
        # independent of the TickRecord's tick_number (Unix nanoseconds).
        self._tick_counter: int = 0  # Current stamping tick (0, 1, 2, ...)
        self._tick_pks: list[bytes] = [pk]  # Public keys indexed by counter

        # Publish adapted genesis to all initially attached calendars.
        # (Calendars attached via attach_calendar do this themselves.)

        # Start daemon thread.
        self._shutdown_event: threading.Event = threading.Event()
        self._daemon = threading.Thread(target=self._daemon_loop, daemon=True)
        self._daemon.start()

    # ---------------------------------------------------------------
    # Properties
    # ---------------------------------------------------------------

    @property
    def tbid(self) -> bytes:
        return self._tbid

    @property
    def tbn(self) -> str:
        return self._tbn

    @property
    def serialized(self) -> bool:
        return self._serialized

    @property
    def chronon_ns(self) -> float:
        return self._chronon_ns

    @property
    def active(self) -> bool:
        return self._active

    @property
    def current_tick(self) -> int:
        return self._tick_counter

    @property
    def current_pk(self) -> bytes:
        return self._current_pk

    # ---------------------------------------------------------------
    # Core operations
    # ---------------------------------------------------------------

    def stamp(self, content: bytes | str) -> Fortis:
        """Sign *content* under the current tick's private key.

        For serialized stamps, the first stamp ticks immediately
        (genesis -> tick 1) and schedules a tick for the end of the
        current chronon window.  Subsequent stamps within the same
        window stamp at the current tick.

        Returns:
            A :class:`Fortis` artifact (returned to caller, NOT stored).

        Raises:
            RuntimeError: if not active (dormant).
        """
        if not self._active:
            raise RuntimeError("Cannot stamp: chronomatter is dormant.")

        with self._rlock:
            if self._serialized:
                self._maybe_tick_serialized()
            tick_number = self.current_tick
            private_key = self._current_sk
            # Publish the tick to calendars so verify can look up the key.
            self._publish_tick_to_calendars()

        content_bytes = content if isinstance(content, bytes) else content.encode("utf-8")
        content_hash = sha256(content_bytes)
        signature_input = self._concat(self._tbid, tick_number, content_bytes)
        signature = sign(signature_input, private_key)

        return Fortis(
            tick_number=tick_number,
            my_content_hash=content_hash,
            signature=signature,
            tbid=self._tbid,
            echo=str(content) if isinstance(content, bytes) else content,
            tbn=self._tbn,
        )

    def verify(
        self,
        content: bytes | str,
        fortis: Fortis,
        calendar: Calendar,
    ) -> bool:
        """Verify a Fortis against content and calendar ticks.

        Uses the Calendar's tick records to look up the public key
        for the claimed tick number.

        Args:
            content: The original content to verify.
            fortis: The Fortis artifact to verify.
            calendar: The calendar whose ticks provide key lookup.

        Returns:
            True if signature and content hash are valid.
        """
        pk = self._lookup_key_for_tick(fortis.tick_number, calendar)
        if pk is None:
            return False

        content_bytes = content if isinstance(content, bytes) else content.encode("utf-8")
        content_hash = sha256(content_bytes)
        if content_hash != fortis.my_content_hash:
            return False

        signature_input = self._concat(fortis.tbid, fortis.tick_number, content_bytes)
        return verify(signature_input, fortis.signature, pk)

    def tick(self) -> None:
        """Advance the calendar to the next tick.

        Publishes the new TickRecord to all attached Calendars.

        Raises:
            RuntimeError: if not active (dormant).
        """
        if not self._active:
            raise RuntimeError("Cannot tick: chronomatter is dormant.")
        with self._rlock:
            self._advance_tick()
            self._publish_tick_to_calendars()
            self._last_tick_wall_ns = _monotonic_ns()

    def get(self, tick_number: int, count: int = 1) -> list[TickRecord]:
        """Return earliest *count* ticks at or after Chronomatter's *tick_number*."""
        with self._rlock:
            # Search by Chronomatter's internal counter, not TickRecord's tick_number.
            start = None
            for i in range(len(self._ticks)):
                if i >= tick_number:
                    start = i
                    break
            if start is None:
                return []
            end = min(start + count, len(self._ticks))
            return list(self._ticks[start:end])

    # ---------------------------------------------------------------
    # Calendar attachment
    # ---------------------------------------------------------------

    def attach_calendar(self, calendar: Calendar) -> None:
        """Attach a calendar, publishing adapted genesis if empty.

        The Chronomatter creates an adapted genesis record for the Calendar:
        Calendar.tbid + Chronomatter's current public key + self-signatures.
        Sets calendar._stamp_tbid to the Chronomatter's tbid for integrity checks.
        """
        with self._rlock:
            self._calendars.append(calendar)
            calendar._stamp_tbid = self._tbid
            if len(calendar.ticks) == 0:
                adapted = self._create_adapted_genesis(calendar)
                calendar.append(adapted)

    # ---------------------------------------------------------------
    # Persistence
    # ---------------------------------------------------------------

    def save(self) -> None:
        """No-op. Only the Calendar is persisted to disk."""
        pass

    def shutdown(self) -> None:
        """Signal the background daemon thread to stop and wait."""
        self._next_tick_time_ns = None
        self._shutdown_event.set()
        if self._daemon is not None:
            self._daemon.join()

    # ---------------------------------------------------------------
    # Internal helpers
    # ---------------------------------------------------------------

    def _advance_tick(self) -> None:
        """Advance to the next tick.

        Must be called with *self._rlock* held.
        """
        current = self._ticks[-1]
        new_record, new_sk = _timebeing._tick(
            tbid=self._tbid,
            current_tick_record=current,
            current_private_key=self._current_sk,
        )
        self._ticks.append(new_record)
        self._tick_pks.append(new_record.public_key)
        self._tick_counter += 1
        self._current_sk = new_sk
        self._current_pk = new_record.public_key

    def _maybe_tick_serialized(self) -> None:
        """Handle serialized ticking.

        If at genesis, tick immediately. Schedule a tick for chronon later.
        Must be called with *self._rlock* held.
        """
        if self._tick_counter == 0:
            self._advance_tick()
            self._last_tick_wall_ns = _monotonic_ns()
        if self._next_tick_time_ns is None:
            self._next_tick_time_ns = self._last_tick_wall_ns + self._chronon_ns

    def _publish_tick_to_calendars(self) -> None:
        """Publish the latest tick to all attached Calendars.

        Only publishes if the calendar doesn't already have this tick
        (avoids duplicates from stamp+tick interactions).
        Must be called with *self._rlock* held.
        """
        latest = self._ticks[-1]
        for calendar in self._calendars:
            cal_latest = calendar.latest()
            if cal_latest is None or cal_latest < latest.tick_number:
                calendar.append(latest)

    def _create_adapted_genesis(self, calendar: Calendar) -> TickRecord:
        """Create a genesis TickRecord for a Calendar.

        Uses Chronomatter's current keypair but the Calendar's tbid/tbn.
        Both forward and backward fortis are self-signatures (Chronomatter's key).
        """
        genesis_ma = _genesis_ma(calendar.tbid, self._current_pk)
        forward = sign(genesis_ma, self._current_sk)
        backward = sign(genesis_ma, self._current_sk)
        return TickRecord(
            tick_number=0,
            public_key=self._current_pk,
            forward_fortis=forward,
            backward_fortis=backward,
        )

    def _lookup_key_for_tick(
        self, tick_number: int, calendar: Calendar
    ) -> bytes | None:
        """Look up the public key for a given Chronomatter tick counter.

        Uses the Chronomatter's own key index (independent of Calendar's
        TickRecord.tick_number which is a Unix timestamp).
        """
        if 0 <= tick_number < len(self._tick_pks):
            return self._tick_pks[tick_number]
        return None

    @staticmethod
    def _concat(tbid: bytes, tick_number: int, content: bytes | str) -> bytes:
        """Concatenate tbid, tick_number as uint64 BE, and content."""
        if isinstance(content, bytes):
            content_bytes = content
        else:
            content_bytes = content.encode("utf-8")
        return tbid + tick_number.to_bytes(8, "big") + content_bytes

    # ---------------------------------------------------------------
    # Daemon thread
    # ---------------------------------------------------------------

    def _daemon_loop(self) -> None:
        """Background loop for scheduled ticks.

        Serialized mode: wakes every 100ms to check for scheduled ticks.
        Non-serialized mode: ticks every chronon.
        """
        if self._serialized:
            check_interval_s = 0.1
            while not self._shutdown_event.is_set():
                self._shutdown_event.wait(check_interval_s)
                if self._shutdown_event.is_set():
                    break
                with self._rlock:
                    if self._next_tick_time_ns is not None:
                        if _monotonic_ns() >= self._next_tick_time_ns:
                            self._next_tick_time_ns = None
                        else:
                            continue
                self.tick()
        else:
            wait_s = self._chronon_ns / 1_000_000_000
            while not self._shutdown_event.is_set():
                self._shutdown_event.wait(wait_s)
                if self._shutdown_event.is_set():
                    break
                try:
                    self.tick()
                except RuntimeError:
                    break
                except Exception:
                    pass
