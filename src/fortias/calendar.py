"""Fortias v1 — Calendar: append-only log of tick records."""

from __future__ import annotations

import json
import pathlib

from ._timebeing import _timebeing
from .models import TickRecord

_HEX_ERROR = "hex-encoded fields must be valid hex strings"


def _bytes_to_hex(value: bytes) -> str:
    return value.hex()


def _hex_to_bytes(value: str) -> bytes:
    try:
        return bytes.fromhex(value)
    except ValueError:
        raise ValueError(_HEX_ERROR) from None


class Calendar:
    """Chrona grapha — Append-only log of a time being's tick chain.

    Attributes:
        tbid: The time being's identity.
        tbn: The time being's human-readable name.
        ticks: The list of :class:`TickRecord` entries.
        stamp_tbid: The identity of the Chronomatter that published these ticks
                    (may differ from tbid for adapted genesis).
    """

    def __init__(
        self,
        tbid: bytes,
        tbn: str,
        ticks: list[TickRecord] | None = None,
        stamp_tbid: bytes | None = None,
    ) -> None:
        self.tbid = tbid
        self.tbn = tbn
        self._stamp_tbid = stamp_tbid or tbid
        self._ticks: list[TickRecord] = list(ticks or [])

    # ------------------------------------------------------------------
    # Mutation
    # ------------------------------------------------------------------

    def append(self, tick_record: TickRecord) -> None:
        """Add a new tick record.

        Raises:
            ValueError: if *tick_record* is not strictly greater than
                        the current head.
        """
        if self._ticks and tick_record.tick_number <= self._ticks[-1].tick_number:
            raise ValueError(
                f"append() requires a strictly increasing tick_number. "
                f"Received {tick_record.tick_number!r}, current maximum is {self._ticks[-1].tick_number!r}."
            )
        self._ticks.append(tick_record)

    # ------------------------------------------------------------------
    # Lookup
    # ------------------------------------------------------------------

    def get(self, tick_number: int, count: int) -> list[TickRecord]:
        """Return earliest *count* ticks at or after *tick_number*.

        Uses binary search to find the first tick >= *tick_number*, then
        returns up to *count* records from that point.
        """
        # Binary search for first tick >= tick_number
        lo, hi = 0, len(self._ticks)
        while lo < hi:
            mid = (lo + hi) // 2
            if self._ticks[mid].tick_number < tick_number:
                lo = mid + 1
            else:
                hi = mid
        start = lo
        end = min(start + count, len(self._ticks))
        return list(self._ticks[start:end])

    def latest(self) -> int | None:
        """Return the highest tick number, or None if empty."""
        return self._ticks[-1].tick_number if self._ticks else None

    # ------------------------------------------------------------------
    # Persistence
    # ------------------------------------------------------------------

    def save(self, path: str | pathlib.Path) -> None:
        """Save to JSON file. Hex-encodes all byte arrays."""
        path = pathlib.Path(path)
        path.parent.mkdir(parents=True, exist_ok=True)
        data = {
            "tbid": _bytes_to_hex(self.tbid),
            "tbn": self.tbn,
            "stamp_tbid": _bytes_to_hex(self._stamp_tbid),
            "ticks": [
                {
                    "tick_number": t.tick_number,
                    "public_key": _bytes_to_hex(t.public_key),
                    "forward_fortis": _bytes_to_hex(t.forward_fortis) if t.forward_fortis is not None else None,
                    "backward_fortis": _bytes_to_hex(t.backward_fortis) if t.backward_fortis is not None else None,
                }
                for t in self._ticks
            ],
        }
        path.write_text(json.dumps(data, indent=2), encoding="utf-8")

    @classmethod
    def load(cls, path: str | pathlib.Path) -> Calendar:
        """Load from JSON file. Validates ascending tick_numbers, hex parse, chain integrity.

        Raises:
            ValueError: if validation fails.
        """
        path = pathlib.Path(path)
        data = json.loads(path.read_text(encoding="utf-8"))

        # Parse top-level fields
        tbid = _hex_to_bytes(data["tbid"])
        tbn = data["tbn"]
        stamp_tbid = _hex_to_bytes(data.get("stamp_tbid", tbid.hex()))

        # Parse tick records
        ticks: list[TickRecord] = []
        prev_tick_number = -1
        for i, td in enumerate(data["ticks"]):
            tick_number = td["tick_number"]
            # Validate non-negative and strictly ascending
            if not isinstance(tick_number, int) or tick_number < 0:
                raise ValueError(f"tick[{i}].tick_number must be a non-negative integer")
            if tick_number <= prev_tick_number:
                raise ValueError(f"tick[{i}].tick_number must be strictly ascending")
            prev_tick_number = tick_number

            public_key = _hex_to_bytes(td["public_key"])
            forward_fortis = _hex_to_bytes(td["forward_fortis"]) if td["forward_fortis"] is not None else None
            backward_fortis = _hex_to_bytes(td["backward_fortis"]) if td["backward_fortis"] is not None else None

            ticks.append(TickRecord(tick_number, public_key, forward_fortis, backward_fortis))

        cal = cls(tbid, tbn, ticks, stamp_tbid)

        # Chain integrity check
        ok, failures = cal.integrity_check(return_failures=True)
        if not ok:
            raise ValueError(
                f"Calendar chain integrity check failed at indexes: {failures}"
            )

        return cal

    # ------------------------------------------------------------------
    # Verification
    # ------------------------------------------------------------------

    def integrity_check(self, return_failures: bool = False) -> bool | tuple[bool, list[int] | None]:
        """Check all consecutive pairs via _verify_pair.

        Returns:
            bool if *return_failures* is False (default).
            (bool, list[int]) if *return_failures* is True.
        """
        if len(self._ticks) < 2:
            return (True, []) if return_failures else True

        failures: list[int] = []
        for i in range(len(self._ticks) - 1):
            if not _timebeing._verify_pair(self._ticks[i + 1], self._ticks[i], self._stamp_tbid):
                failures.append(i)

        ok = len(failures) == 0
        if return_failures:
            return (ok, failures)
        return ok

    @property
    def ticks(self) -> list[TickRecord]:
        """Access the tick list (read-only)."""
        return list(self._ticks)
