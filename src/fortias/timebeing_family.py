"""Fortias v1 — Mutable TimebeingFamily class with threading and persistence.

This class wraps the pure functional operations in :class:`~fortias._timebeing._timebeing`
and manages mutable state: the calendar, current keys, threading, and disk persistence.
"""

from __future__ import annotations

import pathlib
import threading
import uuid

from ._timebeing import _timebeing, _genesis_ma
from .crypto import sign
from .calendar import Calendar
from .config import Config
from .crypto import generate_keypair
from .models import Fortis, TickRecord


class TimebeingFamily:
    """Mutable time being that stamps and verifies Fortis artifacts.

    Creates a self-sovereign temporal integrity entity that stamps content
    with Ed25519 signatures under rotating tick keys. When active, it can
    both stamp and verify. After loading from disk (dormant), it can only
    verify.

    Args:
        name: TBN prefix — final name becomes ``"Time Being {tbid.hex()}"``.
        chronon_ns: Fixed tick interval in nanoseconds.  Stamp()-driven
                    ticks are throttled to at most one per chronon window.
                    Nanoseconds are a practical convenience for v1 (fits
                    in a float with sub-nanosecond jitter); not a
                    fundamental limit of the protocol.
        tbid: Internal identity (UUID v4). Auto-generated if None.
        serialized: If True, tick advances only on stamp (throttled to
                    once per chronon window). If False, a background
                    daemon thread ticks every *chronon*.
        persist_path: Disk path for calendar storage. Resolved via :class:`Config`
                      if None.

    Attributes:
        tbid: The time being's opaque internal identity.
        tbn: Human-readable external name.
        serialized: Whether the time being computes sparse ticks.
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
        self.chronon_ns = chronon_ns
        self.serialized = serialized
        self._tbid = tbid or uuid.uuid4().bytes
        self._tbn = f"Time Being {self._tbid.hex()}"
        self._config = Config.resolve(persist_path=persist_path)
        self._rlock = threading.RLock()
        # Absolute monotonic time of the next scheduled tick (serialized mode).
        self._next_tick_time_ns: float | None = None
        self._last_tick_wall_ns: float = _monotonic_ns()

        # Initialize calendar with genesis.
        # Genesis uses a self-transition: MA(genesis, genesis).
        # Both fortis are computed; neither is None.
        sk, pk = generate_keypair()
        genesis_ma = _genesis_ma(self._tbid, pk)
        genesis_forward = sign(genesis_ma, sk)
        genesis_backward = sign(genesis_ma, sk)
        genesis = TickRecord(
            tick_number=0, public_key=pk,
            forward_fortis=genesis_forward,
            backward_fortis=genesis_backward,
        )
        self._calendar = Calendar(
            tbid=self._tbid, tbn=self._tbn,
            serialized=serialized, chronon_ns=self.chronon_ns,
            ticks=[genesis],
        )
        self._current_sk = sk
        self._current_pk = pk
        self._active = True

        # Start daemon thread for both modes.
        # Serialized: fires scheduled ticks (from stamp).
        # Non-serialized: ticks every chronon.
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
    def active(self) -> bool:
        return self._active

    @property
    def calendar(self) -> Calendar:
        return self._calendar

    # ---------------------------------------------------------------
    # Core operations
    # ---------------------------------------------------------------

    def stamp(self, content: bytes | str) -> Fortis:
        """Sign *content* under the current tick's private key.

        For serialized timebeings, the first stamp ticks immediately
        (genesis → tick 1) and schedules a tick for the end of the
        current chronon window.  Subsequent stamps within the same
        window stamp at the current tick.  A tick is also scheduled so
        it fires automatically after the chronon elapses, regardless
        of further stamping activity.

        Raises RuntimeError if not active (dormant).

        Args:
            content: The data to timestamp.

        Returns:
            A :class:`Fortis` artifact.
        """
        if not self._active:
            raise RuntimeError("Cannot stamp: time being is dormant (loaded from disk).")

        with self._rlock:
            if self.serialized:
                self._maybe_tick_serialized()
            tick_number = self._calendar.latest() or 0
            private_key = self._current_sk
        return _timebeing._stamp(
            content=content,
            tbid=self._tbid,
            tick_number=tick_number,
            private_key=private_key,
            echo=str(content) if isinstance(content, bytes) else content,
            tbn=self._tbn,
        )

    def verify(
        self,
        content: bytes | str,
        fortis: Fortis,
        next_tick_number: int | None = None,
    ) -> bool | tuple[bool, bool | None]:
        """Verify a Fortis against content and calendar.

        Args:
            content: The original content to verify.
            fortis: The Fortis artifact to verify.
            next_tick_number: If provided, also check window closed.

        Returns:
            bool if *next_tick_number* is None.
            (sig_valid, window_closed) tuple otherwise.
        """
        with self._rlock:
            ticks = list(self._calendar.ticks)
        return _timebeing._verify(
            content=content,
            fortis=fortis,
            calendar_ticks=ticks,
            next_tick_number=next_tick_number,
        )

    def current_tick(self) -> int:
        """Return the current tick number."""
        with self._rlock:
            return self._calendar.latest() or 0

    def tick(self) -> None:
        """Advance the calendar to the next tick.

        Concurrent calls are handled gracefully — if another caller has
        already advanced the tick since this call started, this call
        returns without creating a duplicate tick.

        Raises RuntimeError if not active.
        """
        if not self._active:
            raise RuntimeError("Cannot tick: time being is dormant.")
        with self._rlock:
            self._advance_tick()
            self._persist()
            self._last_tick_wall_ns = _monotonic_ns()

    def _advance_tick(self) -> None:
        """Advance the calendar to the next tick.

        Must be called with *self._rlock* held.
        """
        current = self._calendar.ticks[-1]
        new_record, new_sk = _timebeing._tick(
            tbid=self._tbid,
            current_tick_record=current,
            current_private_key=self._current_sk,
        )
        self._calendar.append(new_record)
        self._current_sk = new_sk
        self._current_pk = new_record.public_key

    def _maybe_tick_serialized(self) -> None:
        """Handle serialized ticking.

        If the calendar contains only genesis, advances to tick 1
        immediately.  In either case, schedules a tick for the end of
        the current chronon window (no-op if one is already scheduled).
        The scheduled tick fires asynchronously via the daemon thread.

        """
        with self._rlock:
            if len(self._calendar.ticks) == 1:
                # First stamp: tick immediately so stamp isn't at genesis.
                self._advance_tick()
                # Anchor next tick to when this tick just happened.
                self._last_tick_wall_ns = _monotonic_ns()
            # Schedule a tick for chronon later (guarded: no-op if already set).
            if self._next_tick_time_ns is None:
                self._next_tick_time_ns = self._last_tick_wall_ns + self.chronon_ns

    def get(self, tick_number: int, count: int = 1) -> list[TickRecord]:
        """Return earliest *count* ticks at or after *tick_number*."""
        with self._rlock:
            return self._calendar.get(tick_number, count)

    # ---------------------------------------------------------------
    # Persistence
    # ---------------------------------------------------------------

    def save(self) -> None:
        """Persist the calendar to disk."""
        with self._rlock:
            self._persist()

    def _persist(self) -> None:
        """Persist the calendar to disk.

        Must be called with *self._rlock* held.
        """
        path = f"{self._config.persist_path}/{self._tbid.hex()}/calendar.json"
        self._calendar.save(path)

    def shutdown(self) -> None:
        """Signal the background daemon thread to stop, persist the calendar,
        and wait for it.

        Safe to call on serialized timebeings (no-op).
        """
        if self._daemon is None:
            return
        with self._rlock:
            try:
                self._persist()
            except Exception:
                pass  # Non-fatal; we still shut down
            self._next_tick_time_ns = None
        self._shutdown_event.set()
        self._daemon.join()

    @classmethod
    def load(cls, persist_path: str | None = None) -> TimebeingFamily:
        """Load a time being from its calendar file on disk.

        Returns a **dormant** time being — it can verify but cannot stamp,
        because the private key was never persisted.

        Args:
            persist_path: Path to search for calendars. Resolved via :class:`Config`
                          if None.

        Returns:
            A dormant :class:`TimebeingFamily`.
        """
        config = Config.resolve(persist_path=persist_path)
        calendar_dir = config.persist_path
        # Find calendar files in the directory
        cal_path = None
        for f in pathlib.Path(calendar_dir).glob("*/calendar.json"):
            cal_path = f
            break
        if cal_path is None:
            raise FileNotFoundError(f"No calendar.json found under {calendar_dir}")

        calendar = Calendar.load(cal_path)
        tbid = calendar.tbid
        tbn = calendar.tbn

        # Create a dormant instance
        instance = cls.__new__(cls)
        instance._tbid = tbid
        instance._tbn = tbn
        instance.serialized = calendar.serialized
        instance.chronon_ns = calendar.chronon_ns
        instance._config = config
        instance._rlock = threading.RLock()
        instance._calendar = calendar
        instance._current_sk = None
        instance._current_pk = None
        instance._active = False
        instance._daemon = None
        instance._shutdown_event = threading.Event()
        instance._next_tick_time_ns = None
        instance._last_tick_wall_ns = _monotonic_ns()
        return instance

    # ---------------------------------------------------------------
    # Daemon thread (non-serialized mode)
    # ---------------------------------------------------------------

    def _daemon_loop(self) -> None:
        """Background loop: tick every chronon nanoseconds.

        For serialized mode the daemon only fires when a tick has been
        scheduled by a stamp (every 100ms to detect scheduled ticks
        promptly).  For non-serialized mode it ticks every *chronon*
        seconds as before.
        """
        if self.serialized:
            # Serialized: fire scheduled ticks every 100ms.
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
            # Non-serialized: tick every chronon.
            wait_s = self.chronon_ns / 1_000_000_000
            while not self._shutdown_event.is_set():
                self._shutdown_event.wait(wait_s)
                if self._shutdown_event.is_set():
                    break
                try:
                    self.tick()
                except RuntimeError:
                    break
                except Exception:
                    pass  # Log in production


def _monotonic_ns() -> float:
    """Return the current value of the monotonic clock in nanoseconds."""
    import time
    return time.monotonic_ns()
