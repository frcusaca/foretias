"""Fortias v1 — TimeFamily (Chrona nuntia): the Orchestrator.

Coordinates between :class:`~fortias.chronomatter.ChronomatterV1` (the Time Authority)
and :class:`~fortias.calendar.Calendar` (passive tick storage), providing
a user-facing API that delegates to the Chronomatter for stamping/ticking and
to Calendar(s) for storage.
"""

from __future__ import annotations

import pathlib
import uuid
import threading
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from .calendar import Calendar
    from .chronomatter import (
        CalendarInterface,
        ChronomatterInterface,
        InquirerInterface,
    )

from .calendar import Calendar
from .chronomatter import (
    CalendarInterface,
    ChronomatterInterface,
    Inquirer,
    InquirerInterface,
    ChronomatterV1,
    ChronomatterV1Serial,
)
from .config import Config
from .models import Fortis, TickRecord


class TimeFamily:
    """Chrona nuntia — Time Integrity Orchestrator.

    Coordinates between Chronomatter (the time authority) and Calendar(s)
    (the tick storage).  Provides a user-facing API that delegates
    to the Chronomatter for stamping/ticking and to Calendar(s) for storage.

    Args:
        name: TBN prefix — final name becomes ``"Time Being {tbid.hex()}"``.
        chronon_ns: Fixed tick interval in nanoseconds.
        tbid: Internal identity (UUID v4 bytes). Auto-generated if None.
        serialized: If True, tick advances only on stamp (throttled).
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
        self._config = Config.resolve(persist_path=persist_path)

        # Resolve tbid once, shared between Calendar and Chronomatter
        resolved_tbid = tbid or uuid.uuid4().bytes
        resolved_tbn = f"Time Being {resolved_tbid.hex()}"

        # Create Calendar
        self._calendar = Calendar(tbid=resolved_tbid, tbn=resolved_tbn)

        # Create Chronomatter based on serialized flag
        if serialized:
            self._stamp = ChronomatterV1Serial(
                name=name,
                chronon_ns=chronon_ns,
                tbid=resolved_tbid,
                persist_path=persist_path,
            )
        else:
            self._stamp = ChronomatterV1(
                name=name,
                chronon_ns=chronon_ns,
                tbid=resolved_tbid,
                persist_path=persist_path,
            )

        # Attach calendar to chronomatter
        self._stamp.attach_calendar(self._calendar)
        self._calendar._stamp_tbid = self._stamp.tbid

        # Create Inquirer
        self._inquirer = Inquirer(calendars=[self._calendar])

        # Set family references
        self._calendar.family = self
        self._stamp.family = self
        self._inquirer.family = self

        # Internal lock
        self._rlock = threading.RLock()

    # ---------------------------------------------------------------
    # Interface accessors
    # ---------------------------------------------------------------

    def calendar(self) -> CalendarInterface:
        """Return the Calendar interface."""
        return self._calendar

    def chronomatter(self) -> ChronomatterInterface:
        """Return the Chronomatter interface."""
        return self._stamp

    def inquirer(self) -> InquirerInterface:
        """Return the Inquirer interface."""
        return self._inquirer

    # ---------------------------------------------------------------
    # Properties (backward compatibility)
    # ---------------------------------------------------------------

    @property
    def tbid(self) -> bytes:
        return self._stamp.tbid

    @property
    def tbn(self) -> str:
        return self._stamp.tbn

    @property
    def active(self) -> bool:
        return self._stamp.active

    @property
    def serialized(self) -> bool:
        return isinstance(self._stamp, ChronomatterV1Serial)

    @property
    def chronon_ns(self) -> float:
        return self._stamp.chronon_ns

    # ---------------------------------------------------------------
    # Core operations
    # ---------------------------------------------------------------

    def stamp(self, content: bytes | str) -> Fortis:
        """Sign *content* under the current tick's private key.

        For serialized timebeings, the first stamp ticks immediately
        (genesis → tick 1) and schedules a tick for the end of the
        current chronon window.  Subsequent stamps within the same
        window stamp at the current tick.

        Raises RuntimeError if not active (dormant).

        Args:
            content: The data to timestamp.

        Returns:
            A :class:`Fortis` artifact.
        """
        return self._stamp.stamp(content)

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
        with self._stamp._rlock:
            result = self._inquirer.verify(content, fortis)
        if next_tick_number is None:
            return result
        # Window check
        calendar_ticks = list(self._calendar.ticks)
        next_exists = any(t.tick_number == next_tick_number for t in calendar_ticks)
        return (result, next_exists)

    def current_tick(self) -> int:
        """Return the current tick number."""
        with self._stamp._rlock:
            return self._stamp.current_tick

    def tick(self) -> None:
        """Advance the calendar to the next tick.

        Concurrent calls are handled gracefully.

        Raises RuntimeError if not active.
        """
        if not self._stamp.active:
            raise RuntimeError("Cannot tick: time being is dormant.")
        with self._rlock:
            self._stamp.tick()

    def get(self, tick_number: int, count: int = 1) -> list[TickRecord]:
        """Return earliest *count* ticks at or after *tick_number*."""
        with self._stamp._rlock:
            return self._stamp.get(tick_number, count)

    # ---------------------------------------------------------------
    # Persistence
    # ---------------------------------------------------------------

    def save(self) -> None:
        """Persist the Calendar to disk."""
        with self._rlock:
            self._stamp.save()
            path = (
                f"{self._config.persist_path}/"
                f"{self._calendar.tbid.hex()}/calendar.json"
            )
            self._calendar.save(path)

    # ---------------------------------------------------------------
    # Shutdown
    # ---------------------------------------------------------------

    def shutdown(self) -> None:
        """Signal the background daemon thread to stop, persist the calendar,
        and wait for it.

        Safe to call on serialized timebeings (no-op).
        """
        if self._stamp._daemon is None:
            return
        with self._rlock:
            try:
                # Persist calendar before stopping daemon
                path = (
                    f"{self._config.persist_path}/"
                    f"{self._calendar.tbid.hex()}/calendar.json"
                )
                self._calendar.save(path)
                if hasattr(self._stamp, "_next_tick_time_ns"):
                    self._stamp._next_tick_time_ns = None
            except Exception:
                pass  # Non-fatal; we still shut down
        self._stamp._shutdown_event.set()
        self._stamp._daemon.join()

    # ---------------------------------------------------------------
    # Dormant loading
    # ---------------------------------------------------------------

    @classmethod
    def load(cls, persist_path: str | None = None) -> TimeFamily:
        """Load a time from its calendar file on disk.

        Returns a **dormant** time — it can verify but cannot stamp,
        because the private key was never persisted.

        Args:
            persist_path: Path to search for calendars. Resolved via :class:`Config`
                          if None.

        Returns:
            A dormant :class:`TimeFamily`.
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

        # Create dormant ChronomatterV1Serial (never stamps = no-op behavior)
        stamp = ChronomatterV1Serial.__new__(ChronomatterV1Serial)
        stamp.tbid = tbid
        stamp.tbn = tbn
        stamp._chronon_ns = 60_000_000_000.0
        stamp._current_sk = None
        stamp._current_pk = None
        stamp._active = False
        stamp._config = config
        stamp._calendars = []
        stamp._rlock = threading.RLock()
        stamp._next_tick_time_ns = None
        stamp._last_tick_wall_ns = 0.0
        stamp._shutdown_event = threading.Event()
        stamp._daemon = None
        stamp._ticks = []
        stamp._tick_pks = []
        stamp._tick_counter = 0
        stamp._stamp_tbid = tbid

        # Extract public keys from calendar ticks for dormant verification
        for t in calendar.ticks:
            stamp._tick_pks.append(t.public_key)
        if calendar.ticks:
            stamp._ticks = [calendar.ticks[0]]
            stamp._current_pk = calendar.ticks[0].public_key

        # Create instance
        instance = cls.__new__(cls)
        instance._stamp = stamp
        instance._calendar = calendar
        instance._config = config
        instance._inquirer = Inquirer(calendars=[calendar])
        instance._rlock = threading.RLock()

        return instance
