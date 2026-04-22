"""Fortias v1 — Chronomatter, Inquirer, and interfaces.

Manages Ed25519 key lifecycle, stamping, ticking, and publishes ticks
to all attached Calendars.  Pure functional operations are delegated
to :class:`~fortias._timebeing._timebeing`.
"""

from __future__ import annotations

import threading
import uuid
from typing import Protocol

from ._timebeing import _timebeing, _genesis_ma
from .calendar import Calendar
from .config import Config
from .crypto import generate_keypair, sign, sha256, verify
from .models import Fortis, TickRecord
from .timebeing import Timebeing


def _monotonic_ns() -> float:
    """Return the current value of the monotonic clock in nanoseconds."""
    import time

    return time.monotonic_ns()


# ---------------------------------------------------------------------------
# ABC Interfaces
# ---------------------------------------------------------------------------


class CalendarInterface(Protocol):
    """Interface for a time being that stores tick records."""

    @property
    def tbid(self) -> bytes: ...

    @property
    def tbn(self) -> str: ...

    @property
    def ticks(self) -> list[TickRecord]: ...

    def append(self, tick_record: TickRecord) -> None: ...

    def get(self, tick_number: int, count: int) -> list[TickRecord]: ...

    def latest(self) -> int | None: ...

    def integrity_check(self) -> bool: ...


class ChronomatterInterface(Protocol):
    """Interface for a time being that stamps and ticks."""

    @property
    def tbid(self) -> bytes: ...

    @property
    def tbn(self) -> str: ...

    @property
    def active(self) -> bool: ...

    @property
    def chronon_ns(self) -> float: ...

    @property
    def current_tick(self) -> int: ...

    def stamp(self, content: bytes | str) -> Fortis: ...

    def tick(self) -> None: ...

    def get(self, tick_number: int, count: int) -> list[TickRecord]: ...

    def attach_calendar(self, calendar: CalendarInterface) -> None: ...

    def shutdown(self) -> None: ...


class InquirerInterface(Protocol):
    """Interface for verifying Fortis artifacts."""

    def verify(self, content: bytes | str, fortis: Fortis) -> bool: ...


# ---------------------------------------------------------------------------
# Inquirer — standalone Timebeing that verifies Fortis artifacts
# ---------------------------------------------------------------------------


class Inquirer(Timebeing):
    """Verifies Fortis artifacts by looking up keys from attached calendars.

    Receives a list of calendars from TimeFamily and uses them to find
    the public key for a given tick number.

    Args:
        calendars: List of Calendar instances to query for key lookup.
    """

    def __init__(
        self,
        calendars: list[Calendar],
        tbid: bytes | None = None,
        name: str = "inquirer",
    ) -> None:
        # Initialize Timebeing attributes directly
        _tbid = tbid or uuid.uuid4().bytes
        self.tbid = _tbid
        self.tbn = f"Time Being {_tbid.hex()}"
        self._family = None
        self._calendars = calendars

    def verify(self, content: bytes | str, fortis: Fortis) -> bool:
        """Verify a Fortis against content.

        Looks up the tick at fortis.tick_number (Chronomatter counter index)
        in the calendar, optionally verifies the chain integrity with the
        previous tick, then verifies the signature.

        Args:
            content: The original content to verify.
            fortis: The Fortis artifact to verify.

        Returns:
            True if signature and content hash are valid.
        """
        # Find the calendar that owns this tbid
        target_calendar = None
        for cal in self._calendars:
            if cal.tbid == fortis.tbid:
                target_calendar = cal
                break
        if target_calendar is None:
            return False

        # Look up the tick at the counter index and its predecessor
        prev_tick, curr_tick = self._lookup_surrounding_ticks(fortis.tick_number, target_calendar)

        # Verify content hash
        content_bytes = content if isinstance(content, bytes) else content.encode("utf-8")
        content_hash = sha256(content_bytes)
        if content_hash != fortis.my_content_hash:
            return False

        # Determine which verification path to take
        if prev_tick is not None and curr_tick is not None:
            # Both ticks exist — verify chain integrity, then signature
            if not _timebeing._verify_pair(curr_tick, prev_tick, fortis.tbid):
                return False
            pk = curr_tick.public_key
        elif curr_tick is not None:
            pk = curr_tick.public_key
        else:
            return False

        signature_input = _concat(fortis.tbid, fortis.tick_number, content_bytes)
        return verify(signature_input, fortis.signature, pk)

    def _lookup_surrounding_ticks(
        self, tick_number: int, calendar: Calendar
    ) -> tuple[TickRecord | None, TickRecord | None]:
        """Find the two ticks surrounding tick_number.

        Fortis tick_number is the Chronomatter's internal counter (0, 1, 2...).
        Calendar ticks are stored in order of creation, so tick at counter N
        is at calendar.ticks[N].

        Returns (prev_tick, curr_tick) where prev_tick is the tick at counter
        N-1 (or None if N==0) and curr_tick is the tick at counter N (or None).
        """
        ticks = calendar.ticks
        if tick_number < 0 or tick_number >= len(ticks):
            return None, None
        curr_tick = ticks[tick_number]
        prev_tick = ticks[tick_number - 1] if tick_number > 0 else None
        return prev_tick, curr_tick


def _uint64_be(value: int) -> bytes:
    """Encode an integer as 8-byte big-endian."""
    return value.to_bytes(8, "big")


def _concat(tbid: bytes, tick_number: int, content: bytes | str) -> bytes:
    """Concatenate tbid, tick_number as uint64 BE, and content for signature input."""
    if isinstance(content, bytes):
        content_bytes = content
    else:
        content_bytes = content.encode("utf-8")
    return tbid + _uint64_be(tick_number) + content_bytes


# ---------------------------------------------------------------------------
# ChronomatterV1 — non-serialized daemon
# ---------------------------------------------------------------------------


class ChronomatterV1(Calendar):
    """Non-serialized Chronomatter — ticks every chronon regardless of activity."""

    def __init__(
        self,
        name: str = "timebeing",
        chronon_ns: float = 60_000_000_000.0,
        tbid: bytes | None = None,
        persist_path: str | None = None,
    ) -> None:
        # Calendar.__init__ takes (tbid, tbn, ...)
        _tbid = tbid or uuid.uuid4().bytes
        super().__init__(tbid=_tbid, tbn=f"Time Being {_tbid.hex()}")
        self._chronon_ns = chronon_ns
        self._config = Config.resolve(persist_path=persist_path)
        self._rlock = threading.RLock()
        self._last_tick_wall_ns: float = _monotonic_ns()
        self._calendars: list[Calendar] = []

        sk, pk = generate_keypair()
        genesis_ma = _genesis_ma(self.tbid, pk)
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
        self._ticks.append(self._genesis)
        self._tick_counter: int = 0
        self._tick_pks: list[bytes] = [pk]

        self._shutdown_event: threading.Event = threading.Event()
        self._daemon = threading.Thread(target=self._daemon_loop, daemon=True)
        self._daemon.start()

    @property
    def active(self) -> bool:
        return self._active

    @property
    def chronon_ns(self) -> float:
        return self._chronon_ns

    @property
    def current_tick(self) -> int:
        return self._tick_counter

    @property
    def current_pk(self) -> bytes:
        return self._current_pk

    def stamp(self, content: bytes | str) -> Fortis:
        """Sign *content* under the current tick's private key."""
        if not self._active:
            raise RuntimeError("Cannot stamp: chronomatter is dormant.")

        with self._rlock:
            tick_number = self.current_tick
            private_key = self._current_sk
            self._publish_tick_to_calendars()

        content_bytes = content if isinstance(content, bytes) else content.encode("utf-8")
        content_hash = sha256(content_bytes)
        signature_input = _concat(self.tbid, tick_number, content_bytes)
        signature = sign(signature_input, private_key)

        return Fortis(
            tick_number=tick_number,
            my_content_hash=content_hash,
            signature=signature,
            tbid=self.tbid,
            echo=str(content) if isinstance(content, bytes) else content,
            tbn=self.tbn,
        )

    def tick(self) -> None:
        """Advance the calendar to the next tick."""
        if not self._active:
            raise RuntimeError("Cannot tick: chronomatter is dormant.")
        with self._rlock:
            self._advance_tick()
            self._publish_tick_to_calendars()
            self._last_tick_wall_ns = _monotonic_ns()

    def get(self, tick_number: int, count: int = 1) -> list[TickRecord]:
        """Return earliest *count* ticks at or after Chronomatter's *tick_number*."""
        with self._rlock:
            start = None
            for i in range(len(self._ticks)):
                if i >= tick_number:
                    start = i
                    break
            if start is None:
                return []
            end = min(start + count, len(self._ticks))
            return list(self._ticks[start:end])

    def attach_calendar(self, calendar: Calendar) -> None:
        """Attach a calendar, publishing adapted genesis if empty."""
        with self._rlock:
            self._calendars.append(calendar)
            calendar._stamp_tbid = self.tbid
            if len(calendar.ticks) == 0:
                adapted = self._create_adapted_genesis(calendar)
                calendar.append(adapted)

    def save(self) -> None:
        """No-op. Only the Calendar is persisted to disk."""
        pass

    def shutdown(self) -> None:
        """Signal the background daemon thread to stop and wait."""
        self._next_tick_time_ns = None  # type: ignore[attr-defined]
        self._shutdown_event.set()
        if self._daemon is not None:
            self._daemon.join()

    def _advance_tick(self) -> None:
        """Advance to the next tick. Must be called with lock held."""
        current = self._ticks[-1]
        new_record, new_sk = _timebeing._tick(
            tbid=self.tbid,
            current_tick_record=current,
            current_private_key=self._current_sk,
        )
        self._ticks.append(new_record)
        self._tick_pks.append(new_record.public_key)
        self._tick_counter += 1
        self._current_sk = new_sk
        self._current_pk = new_record.public_key

    def _publish_tick_to_calendars(self) -> None:
        """Publish the latest tick to all attached Calendars. Must be called with lock held."""
        latest = self._ticks[-1]
        for calendar in self._calendars:
            cal_latest = calendar.latest()
            if cal_latest is None or cal_latest < latest.tick_number:
                calendar.append(latest)

    def _create_adapted_genesis(self, calendar: Calendar) -> TickRecord:
        """Create a genesis TickRecord for a Calendar."""
        genesis_ma = _genesis_ma(calendar.tbid, self._current_pk)
        forward = sign(genesis_ma, self._current_sk)
        backward = sign(genesis_ma, self._current_sk)
        return TickRecord(
            tick_number=0,
            public_key=self._current_pk,
            forward_fortis=forward,
            backward_fortis=backward,
        )

    def _daemon_loop(self) -> None:
        """Background loop: tick every chronon seconds."""
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


# ---------------------------------------------------------------------------
# ChronomatterV1Serial — throttled daemon
# ---------------------------------------------------------------------------


class ChronomatterV1Serial(Calendar):
    """Serialized Chronomatter — ticks only on first stamp or when chronon elapses."""

    def __init__(
        self,
        name: str = "timebeing",
        chronon_ns: float = 60_000_000_000.0,
        tbid: bytes | None = None,
        persist_path: str | None = None,
    ) -> None:
        # Calendar.__init__ takes (tbid, tbn, ...)
        _tbid = tbid or uuid.uuid4().bytes
        super().__init__(tbid=_tbid, tbn=f"Time Being {_tbid.hex()}")
        self._chronon_ns = chronon_ns
        self._config = Config.resolve(persist_path=persist_path)
        self._rlock = threading.RLock()
        self._last_tick_wall_ns: float = _monotonic_ns()
        self._calendars: list[Calendar] = []
        self._next_tick_time_ns: float | None = None

        sk, pk = generate_keypair()
        genesis_ma = _genesis_ma(self.tbid, pk)
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
        self._ticks.append(self._genesis)
        self._tick_counter: int = 0
        self._tick_pks: list[bytes] = [pk]

        self._shutdown_event: threading.Event = threading.Event()
        self._daemon = threading.Thread(target=self._daemon_loop, daemon=True)
        self._daemon.start()

    @property
    def active(self) -> bool:
        return self._active

    @property
    def chronon_ns(self) -> float:
        return self._chronon_ns

    @property
    def current_tick(self) -> int:
        return self._tick_counter

    @property
    def current_pk(self) -> bytes:
        return self._current_pk

    def stamp(self, content: bytes | str) -> Fortis:
        """Sign *content* under the current tick's private key."""
        if not self._active:
            raise RuntimeError("Cannot stamp: chronomatter is dormant.")

        with self._rlock:
            self._maybe_tick_serialized()
            tick_number = self.current_tick
            private_key = self._current_sk
            self._publish_tick_to_calendars()

        content_bytes = content if isinstance(content, bytes) else content.encode("utf-8")
        content_hash = sha256(content_bytes)
        signature_input = _concat(self.tbid, tick_number, content_bytes)
        signature = sign(signature_input, private_key)

        return Fortis(
            tick_number=tick_number,
            my_content_hash=content_hash,
            signature=signature,
            tbid=self.tbid,
            echo=str(content) if isinstance(content, bytes) else content,
            tbn=self.tbn,
        )

    def tick(self) -> None:
        """Advance the calendar to the next tick."""
        if not self._active:
            raise RuntimeError("Cannot tick: chronomatter is dormant.")
        with self._rlock:
            self._advance_tick()
            self._publish_tick_to_calendars()
            self._last_tick_wall_ns = _monotonic_ns()

    def get(self, tick_number: int, count: int = 1) -> list[TickRecord]:
        """Return earliest *count* ticks at or after Chronomatter's *tick_number*."""
        with self._rlock:
            start = None
            for i in range(len(self._ticks)):
                if i >= tick_number:
                    start = i
                    break
            if start is None:
                return []
            end = min(start + count, len(self._ticks))
            return list(self._ticks[start:end])

    def attach_calendar(self, calendar: Calendar) -> None:
        """Attach a calendar, publishing adapted genesis if empty."""
        with self._rlock:
            self._calendars.append(calendar)
            calendar._stamp_tbid = self.tbid
            if len(calendar.ticks) == 0:
                adapted = self._create_adapted_genesis(calendar)
                calendar.append(adapted)

    def save(self) -> None:
        """No-op. Only the Calendar is persisted to disk."""
        pass

    def shutdown(self) -> None:
        """Signal the background daemon thread to stop and wait."""
        self._next_tick_time_ns = None
        self._shutdown_event.set()
        if self._daemon is not None:
            self._daemon.join()

    def _advance_tick(self) -> None:
        """Advance to the next tick. Must be called with lock held."""
        current = self._ticks[-1]
        new_record, new_sk = _timebeing._tick(
            tbid=self.tbid,
            current_tick_record=current,
            current_private_key=self._current_sk,
        )
        self._ticks.append(new_record)
        self._tick_pks.append(new_record.public_key)
        self._tick_counter += 1
        self._current_sk = new_sk
        self._current_pk = new_record.public_key

    def _maybe_tick_serialized(self) -> None:
        """Handle serialized ticking. Must be called with lock held."""
        if self._tick_counter == 0:
            self._advance_tick()
            self._last_tick_wall_ns = _monotonic_ns()
        if self._next_tick_time_ns is None:
            self._next_tick_time_ns = self._last_tick_wall_ns + self._chronon_ns

    def _publish_tick_to_calendars(self) -> None:
        """Publish the latest tick to all attached Calendars. Must be called with lock held."""
        latest = self._ticks[-1]
        for calendar in self._calendars:
            cal_latest = calendar.latest()
            if cal_latest is None or cal_latest < latest.tick_number:
                calendar.append(latest)

    def _create_adapted_genesis(self, calendar: Calendar) -> TickRecord:
        """Create a genesis TickRecord for a Calendar."""
        genesis_ma = _genesis_ma(calendar.tbid, self._current_pk)
        forward = sign(genesis_ma, self._current_sk)
        backward = sign(genesis_ma, self._current_sk)
        return TickRecord(
            tick_number=0,
            public_key=self._current_pk,
            forward_fortis=forward,
            backward_fortis=backward,
        )

    def _daemon_loop(self) -> None:
        """Background loop: wake every 100ms to check for scheduled ticks."""
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
