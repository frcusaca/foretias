"""Fortias v1 — TimeFamily (Chrona nuntia): the Orchestrator.

Coordinates between Chronomatter (the Time Authority) and Calendar (passive tick storage),
providing a user-facing API that delegates to the Chronomatter for stamping/ticking and
to Calendar for storage.

.. deprecated:: Use ``fortias_p2p.PyTimeFamilyServer`` (Rust) instead.
"""

from __future__ import annotations

import warnings

warnings.warn(
    "fortias.time_family is deprecated. Use fortias (Rust-backed) instead.",
    DeprecationWarning,
    stacklevel=2,
)

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

        resolved_tbid = tbid or uuid.uuid4().bytes
        resolved_tbn = f"Time Being {resolved_tbid.hex()}"

        self._calendar = Calendar(tbid=resolved_tbid, tbn=resolved_tbn)

        if serialized:
            self._chronomatter = ChronomatterV1Serial(
                name=name,
                chronon_ns=chronon_ns,
                tbid=resolved_tbid,
                persist_path=persist_path,
            )
        else:
            self._chronomatter = ChronomatterV1(
                name=name,
                chronon_ns=chronon_ns,
                tbid=resolved_tbid,
                persist_path=persist_path,
            )

        self._inquirer = Inquirer()

        self._calendar.family = self
        self._chronomatter.family = self
        self._inquirer.family = self
        self._calendar.stamp_tbid = self._chronomatter.tbid

        if len(self._calendar.ticks) == 0:
            adapted = self._chronomatter.create_adapted_genesis(self._calendar)
            self._calendar.append(adapted)

        self._rlock = threading.RLock()

    # -- Internal interface accessors --

    def _get_calendar(self) -> CalendarInterface:
        return self._calendar

    def _get_chronomatter(self) -> ChronomatterInterface:
        return self._chronomatter

    def _get_inquirer(self) -> InquirerInterface:
        return self._inquirer

    calendar = _get_calendar
    chronomatter = _get_chronomatter
    inquirer = _get_inquirer

    # -- Properties --

    @property
    def tbid(self) -> bytes:
        return self._chronomatter.tbid

    @property
    def tbn(self) -> str:
        return self._chronomatter.tbn

    @property
    def active(self) -> bool:
        return self._chronomatter.active

    @property
    def serialized(self) -> bool:
        return isinstance(self._chronomatter, ChronomatterV1Serial)

    @property
    def chronon_ns(self) -> float:
        return self._chronomatter.chronon_ns

    # -- Core operations --

    def stamp(self, content: bytes | str) -> Fortis:
        """Sign *content* under the current tick's private key.

        For serialized timebeings, the first stamp ticks immediately
        (genesis → tick 1) and schedules a tick for the end of the
        current chronon window.  Subsequent stamps within the same
        window stamp at the current tick.

        Raises RuntimeError if not active (dormant).
        """
        return self._chronomatter.stamp(content)

    def verify(
        self,
        content: bytes | str,
        fortis: Fortis,
        next_tick_number: int | None = None,
    ) -> bool | tuple[bool, bool | None]:
        """Verify a Fortis against content and calendar.

        Returns bool if *next_tick_number* is None.
        Returns (sig_valid, window_closed) tuple otherwise.
        """
        with self._chronomatter.hold_read_lock():
            result = self._inquirer.verify(content, fortis)
        if next_tick_number is None:
            return result
        calendar_ticks = list(self._calendar.ticks)
        next_exists = any(t.tick_number == next_tick_number for t in calendar_ticks)
        return (result, next_exists)

    def current_tick(self) -> int:
        """Return the current tick number."""
        with self._chronomatter.hold_read_lock():
            return self._chronomatter.current_tick

    def tick(self) -> None:
        """Advance the calendar to the next tick.

        Raises RuntimeError if not active.
        """
        if not self._chronomatter.active:
            raise RuntimeError("Cannot tick: time being is dormant.")
        with self._rlock:
            self._chronomatter.tick()

    def get(self, tick_number: int, count: int = 1) -> list[TickRecord]:
        """Return earliest *count* ticks at or after *tick_number*."""
        with self._chronomatter.hold_read_lock():
            return self._chronomatter.get(tick_number, count)

    # -- Persistence --

    def save(self) -> None:
        """Persist the Calendar to disk."""
        with self._rlock:
            self._chronomatter.save()
            path = (
                f"{self._config.persist_path}/"
                f"{self._calendar.tbid.hex()}/calendar.json"
            )
            self._calendar.save(path)

    # -- Shutdown --

    def shutdown(self) -> None:
        """Signal the background daemon thread to stop, persist the calendar,
        and wait for it.

        Safe to call when no daemon is running.
        """
        if not self._chronomatter.has_daemon:
            return
        with self._rlock:
            try:
                path = (
                    f"{self._config.persist_path}/"
                    f"{self._calendar.tbid.hex()}/calendar.json"
                )
                self._calendar.save(path)
            except Exception:
                pass
        self._chronomatter.shutdown()

    # -- Dormant loading --

    def _init_from_components(
        self,
        config: Config,
        calendar: Calendar,
        chronomatter: ChronomatterV1Serial,
    ) -> None:
        """Wire pre-constructed components into this instance.

        Used by ``load()`` to assemble a dormant TimeFamily without
        re-creating any of the child timebeings.
        """
        self._config = config
        self._calendar = calendar
        self._chronomatter = chronomatter
        self._inquirer = Inquirer()
        self._rlock = threading.RLock()
        self._calendar.family = self
        self._chronomatter.family = self
        self._inquirer.family = self

    @classmethod
    def load(cls, persist_path: str | None = None) -> TimeFamily:
        """Load a time from its calendar file on disk.

        Returns a **dormant** time — it can verify but cannot stamp,
        because the private key was never persisted.
        """
        config = Config.resolve(persist_path=persist_path)
        calendar_dir = config.persist_path

        cal_path = None
        for f in pathlib.Path(calendar_dir).glob("*/calendar.json"):
            cal_path = f
            break
        if cal_path is None:
            raise FileNotFoundError(f"No calendar.json found under {calendar_dir}")

        calendar = Calendar.load(cal_path)
        chronomatter = ChronomatterV1Serial.from_calendar(calendar, config)

        instance = cls.__new__(cls)
        instance._init_from_components(config, calendar, chronomatter)
        return instance
