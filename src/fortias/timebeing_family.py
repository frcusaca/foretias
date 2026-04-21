"""Fortias v1 — Mutable TimebeingFamily class with threading and persistence.

This class wraps the pure functional operations in :class:`~fortias._timebeing._timebeing`
and manages mutable state: the calendar, current keys, threading, and disk persistence.
"""

from __future__ import annotations

import pathlib
import threading
import uuid
from datetime import timedelta

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
        chronon: Fixed tick interval. Only used for non-serialized daemon thread.
        tbid: Internal identity (UUID v4). Auto-generated if None.
        serialized: If True, tick advances only on stamp. If False,
                    a background daemon thread ticks every *chronon*.
        persist_path: Disk path for calendar storage. Resolved via :class:`Config`
                      if None.

    Attributes:
        tbid: The time being's opaque internal identity.
        tbn: Human-readable external name.
        serialized: Whether the time being computes sparse ticks.
        chronon_seconds: Tick interval in seconds.
        active: True if this instance holds the current private key.
    """

    def __init__(
        self,
        name: str = "timebeing",
        chronon: timedelta | None = None,
        tbid: bytes | None = None,
        serialized: bool = False,
        persist_path: str | None = None,
    ) -> None:
        self.chronon_seconds = int(
            (chronon or timedelta(minutes=1)).total_seconds()
        )
        self.serialized = serialized
        self._tbid = tbid or uuid.uuid4().bytes
        self._tbn = f"Time Being {self._tbid.hex()}"
        self._config = Config.resolve(persist_path=persist_path)
        self._lock = threading.Lock()

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
            serialized=serialized, chronon_seconds=self.chronon_seconds,
            ticks=[genesis],
        )
        self._current_sk = sk
        self._current_pk = pk
        self._active = True

        # Start daemon thread for non-serialized mode
        if not serialized:
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

        For serialized timebeings, advances the tick before signing.
        Raises RuntimeError if not active (dormant).

        Args:
            content: The data to timestamp.

        Returns:
            A :class:`Fortis` artifact.
        """
        if not self._active:
            raise RuntimeError("Cannot stamp: time being is dormant (loaded from disk).")

        with self._lock:
            if self.serialized:
                self._advance_tick()
            return _timebeing._stamp(
                content=content,
                tbid=self._tbid,
                tick_number=self._calendar.latest() or 0,
                private_key=self._current_sk,
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
        with self._lock:
            return _timebeing._verify(
                content=content,
                fortis=fortis,
                calendar_ticks=self._calendar.ticks,
                next_tick_number=next_tick_number,
            )

    def current_tick(self) -> int:
        """Return the current tick number."""
        return self._calendar.latest() or 0

    def tick(self) -> None:
        """Advance the calendar to the next tick.

        Raises RuntimeError if not active.
        """
        if not self._active:
            raise RuntimeError("Cannot tick: time being is dormant.")
        with self._lock:
            self._advance_tick()
            self._persist()

    def _advance_tick(self) -> None:
        """Internal tick advancement (must be called with lock held)."""
        current = self._calendar.ticks[-1]
        new_record, new_sk = _timebeing._tick(
            tbid=self._tbid,
            current_tick_record=current,
            current_private_key=self._current_sk,
        )
        self._calendar.append(new_record)
        self._current_sk = new_sk
        self._current_pk = new_record.public_key

    def get(self, tick_number: int, count: int = 1) -> list[TickRecord]:
        """Return earliest *count* ticks at or after *tick_number*."""
        with self._lock:
            return self._calendar.get(tick_number, count)

    # ---------------------------------------------------------------
    # Persistence
    # ---------------------------------------------------------------

    def save(self) -> None:
        """Persist the calendar to disk."""
        with self._lock:
            self._persist()

    def _persist(self) -> None:
        """Internal save (must be called with lock held)."""
        path = f"{self._config.persist_path}/{self._tbid.hex()}/calendar.json"
        self._calendar.save(path)

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
        instance.chronon_seconds = calendar.chronon_seconds
        instance._config = config
        instance._lock = threading.Lock()
        instance._calendar = calendar
        instance._current_sk = None
        instance._current_pk = None
        instance._active = False
        instance._daemon = None
        return instance

    # ---------------------------------------------------------------
    # Daemon thread (non-serialized mode)
    # ---------------------------------------------------------------

    def _daemon_loop(self) -> None:
        """Background loop: tick every chronon seconds."""
        import time
        while True:
            time.sleep(self.chronon_seconds)
            try:
                self.tick()
            except RuntimeError:
                # Dormant — stop daemon
                break
            except Exception:
                pass  # Log in production
