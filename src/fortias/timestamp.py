"""
Fortias Protocol v0.0.1 — Timestamps.

:class:`TimestampV0` stores a single ``ctypes.c_uint64`` value representing
nanoseconds since the Unix epoch (1970-01-01T00:00:00Z).  Because the value
is a single monotonically comparable integer, ordering is a plain integer
comparison — no field-by-field logic required.

Range: 0 – 18 446 744 073 709 551 615 ns
       ≈ 1970-01-01T00:00:00Z  to  2554-07-21T23:34:33Z

:class:`TimestampFactoryV0` is the canonical factory for this type and
provides construction from ``datetime.datetime``, from a nanosecond-precision
ISO 8601 string, and from explicit calendar components.
"""

from __future__ import annotations

import re
import struct
from ctypes import c_uint64
from dataclasses import dataclass
from datetime import datetime, timedelta, timezone
from functools import total_ordering


# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

_UINT64_MAX    = (1 << 64) - 1
_NS_PER_SEC    = 1_000_000_000
_NS_PER_US     = 1_000
_UNIX_EPOCH    = datetime(1970, 1, 1, tzinfo=timezone.utc)
_SPECIFICATION = "Fortias Timestamp V0: nanosecond-precision ISO 8601, AD 0-9999"

# Matches: 2026-04-19T14:30:00[.123456789][Z|+00:00]
_ISO_RE = re.compile(
    r"^(\d{4})-(\d{2})-(\d{2})[T ](\d{2}):(\d{2}):(\d{2})"
    r"(?:\.(\d{1,9}))?"
    r"(Z|[+-]\d{2}:\d{2})?$"
)


# ---------------------------------------------------------------------------
# Internal helpers
# ---------------------------------------------------------------------------

def _dt_to_ns(dt: datetime, extra_ns: int = 0) -> int:
    """Convert a :class:`~datetime.datetime` to nanoseconds since Unix epoch.

    ``datetime`` carries microsecond precision; *extra_ns* (0–999) adds the
    sub-microsecond nanosecond residual.

    Args:
        dt:       An aware :class:`~datetime.datetime`.  Naive datetimes are
                  assumed UTC.
        extra_ns: Sub-microsecond nanoseconds to add (0–999).
    """
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    delta = dt.astimezone(timezone.utc) - _UNIX_EPOCH
    ns = (
        delta.days         * 86_400 * _NS_PER_SEC
        + delta.seconds    * _NS_PER_SEC
        + delta.microseconds * _NS_PER_US
        + extra_ns
    )
    if not (0 <= ns <= _UINT64_MAX):
        raise ValueError(f"Datetime {dt!r} is outside the uint64 nanosecond range.")
    return ns


# ---------------------------------------------------------------------------
# TimestampV0
# ---------------------------------------------------------------------------


@total_ordering
@dataclass(frozen=True, eq=False)
class TimestampV0:
    """Nanoseconds since Unix epoch stored as a ``ctypes.c_uint64``.

    Storage is a single unsigned 64-bit integer.  Ordering is therefore
    plain integer comparison — no multi-field lexicographic logic required.

    ``ctypes.c_uint64`` does not define comparison operators itself; they
    are provided here via ``@total_ordering`` over ``.value``.

    Attributes:
        _storage: The underlying ``c_uint64`` holding nanoseconds since epoch.
    """

    _storage: c_uint64

    # ------------------------------------------------------------------
    # Construction validation
    # ------------------------------------------------------------------

    def __post_init__(self) -> None:
        # Validate *before* the c_uint64 wraps silently on overflow.
        raw = self._storage.value
        if not (0 <= raw <= _UINT64_MAX):
            raise ValueError(
                f"Timestamp value {raw} is outside uint64 range 0–{_UINT64_MAX}."
            )

    # ------------------------------------------------------------------
    # Value access
    # ------------------------------------------------------------------

    @property
    def value(self) -> int:
        """The raw nanosecond count as a plain Python ``int``."""
        return self._storage.value

    # ------------------------------------------------------------------
    # Comparison
    # ------------------------------------------------------------------

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, TimestampV0):
            return NotImplemented
        return self._storage.value == other._storage.value

    def __lt__(self, other: object) -> bool:
        if not isinstance(other, TimestampV0):
            return NotImplemented
        return self._storage.value < other._storage.value

    def __hash__(self) -> int:
        return hash(self._storage.value)

    # @total_ordering derives __le__, __gt__, __ge__, __ne__.

    # ------------------------------------------------------------------
    # Serialisation
    # ------------------------------------------------------------------

    def to_bytes(self) -> bytes:
        """Serialise to 8 big-endian bytes (network byte order)."""
        return struct.pack(">Q", self._storage.value)

    @classmethod
    def from_bytes(cls, data: bytes) -> "TimestampV0":
        """Deserialise from 8 bytes produced by :meth:`to_bytes`.

        Raises:
            :class:`ValueError`: if *data* is not exactly 8 bytes.
        """
        if len(data) != 8:
            raise ValueError(
                f"TimestampV0 requires exactly 8 bytes, got {len(data)}."
            )
        (ns,) = struct.unpack(">Q", data)
        return cls(c_uint64(ns))

    # ------------------------------------------------------------------
    # ISO 8601 formatting
    # ------------------------------------------------------------------

    def to_iso_string(self) -> str:
        """Format as a nanosecond-precision ISO 8601 UTC string.

        Returns a string of the form ``"YYYY-MM-DDTHH:MM:SS.nnnnnnnnnZ"``.

        Example::

            ts.to_iso_string()  # "2026-04-19T14:30:00.500000000Z"
        """
        seconds, ns = divmod(self._storage.value, _NS_PER_SEC)
        dt = datetime.fromtimestamp(seconds, tz=timezone.utc)
        return f"{dt.strftime('%Y-%m-%dT%H:%M:%S')}.{ns:09d}Z"

    # ------------------------------------------------------------------
    # Display
    # ------------------------------------------------------------------

    def __repr__(self) -> str:
        return f"TimestampV0({self.to_iso_string()!r})"


# ---------------------------------------------------------------------------
# TimestampFactoryV0
# ---------------------------------------------------------------------------


class TimestampFactoryV0:
    """Canonical Fortias factory for nanosecond-precision ISO 8601 timestamps.

    Specification
    -------------
    ``"Fortias Timestamp V0: nanosecond-precision ISO 8601, AD 0-9999"``

    All factory methods return a :class:`TimestampV0` whose ``c_uint64``
    stores nanoseconds elapsed since ``1970-01-01T00:00:00Z`` (Unix epoch).

    Construction methods
    --------------------
    :meth:`make`             — explicit calendar components (keyword-only)
    :meth:`from_datetime`    — from a ``datetime.datetime``
    :meth:`from_iso_string`  — from a nanosecond-precision ISO 8601 string

    Notes
    -----
    * ``datetime.datetime`` carries only microsecond precision.
      :meth:`from_datetime` and :meth:`make` accept an optional *nanoseconds*
      argument (0–999) for the sub-microsecond residual.
    * Naive datetimes are treated as UTC.
    * The representable range is approximately 1970–2554.
    """

    @staticmethod
    def get_specification() -> str:
        """Return the canonical specification string.

        Returns:
            ``"Fortias Timestamp V0: nanosecond-precision ISO 8601, AD 0-9999"``
        """
        return _SPECIFICATION

    # ------------------------------------------------------------------
    # Construction: explicit components
    # ------------------------------------------------------------------

    @staticmethod
    def make(
        *,
        year: int,
        month: int,
        day: int,
        hour: int = 0,
        minute: int = 0,
        second: int = 0,
        microseconds: int = 0,
        nanoseconds: int = 0,
    ) -> TimestampV0:
        """Construct from explicit ISO 8601 calendar components.

        All arguments are keyword-only to prevent silent positional mistakes.

        Args:
            year:         Full Gregorian year (e.g. 2026).
            month:        Month of year, 1–12.
            day:          Day of month, 1–31.
            hour:         Hour of day, 0–23. Defaults to 0.
            minute:       Minute of hour, 0–59. Defaults to 0.
            second:       Second of minute, 0–59. Defaults to 0.
            microseconds: Microsecond component, 0–999 999. Defaults to 0.
            nanoseconds:  Sub-microsecond nanosecond residual, 0–999.
                          Defaults to 0.

        Returns:
            A :class:`TimestampV0` encoding the given instant.

        Raises:
            :class:`ValueError`: if any component is out of range.
        """
        if not (0 <= nanoseconds <= 999):
            raise ValueError(f"nanoseconds residual {nanoseconds} is outside 0–999.")
        dt = datetime(year, month, day, hour, minute, second,
                      microseconds, tzinfo=timezone.utc)
        ns = _dt_to_ns(dt, extra_ns=nanoseconds)
        return TimestampV0(c_uint64(ns))

    # ------------------------------------------------------------------
    # Construction: from datetime.datetime
    # ------------------------------------------------------------------

    @staticmethod
    def from_datetime(dt: datetime, *, nanoseconds: int = 0) -> TimestampV0:
        """Construct from a ``datetime.datetime``.

        ``datetime`` has microsecond precision.  Pass *nanoseconds* (0–999)
        to specify the sub-microsecond residual if needed.

        Args:
            dt:          Source datetime.  Naive datetimes are treated as UTC.
            nanoseconds: Sub-microsecond nanosecond residual, 0–999.

        Returns:
            A :class:`TimestampV0` encoding *dt* at nanosecond precision.

        Raises:
            :class:`ValueError`: if *dt* is outside the uint64 range or
                                 *nanoseconds* is outside 0–999.
        """
        if not (0 <= nanoseconds <= 999):
            raise ValueError(f"nanoseconds residual {nanoseconds} is outside 0–999.")
        ns = _dt_to_ns(dt, extra_ns=nanoseconds)
        return TimestampV0(c_uint64(ns))

    # ------------------------------------------------------------------
    # Construction: from ISO 8601 string
    # ------------------------------------------------------------------

    @staticmethod
    def from_iso_string(s: str) -> TimestampV0:
        """Construct from a nanosecond-precision ISO 8601 string.

        Accepted formats::

            "2026-04-19T14:30:00Z"
            "2026-04-19T14:30:00.123456789Z"
            "2026-04-19T14:30:00.123456789+05:30"
            "2026-04-19 14:30:00.000000001Z"

        The fractional-seconds field may be 1–9 digits; shorter strings are
        zero-padded on the right to 9 digits (e.g. ``".1"`` → 100 000 000 ns).
        A missing timezone suffix is treated as UTC.

        Args:
            s: ISO 8601 timestamp string with optional nanosecond precision.

        Returns:
            A :class:`TimestampV0` encoding the given instant.

        Raises:
            :class:`ValueError`: if *s* cannot be parsed.
        """
        m = _ISO_RE.match(s.strip())
        if not m:
            raise ValueError(
                f"Cannot parse ISO 8601 string: {s!r}.  "
                f"Expected format: YYYY-MM-DDTHH:MM:SS[.nnnnnnnnn][Z|+HH:MM]"
            )

        year, month, day, hour, minute, second = (int(g) for g in m.groups()[:6])

        # Fractional seconds: pad/truncate to exactly 9 digits
        frac_str = m.group(7) or "0"
        total_ns_frac = int(frac_str.ljust(9, "0")[:9])

        # Timezone
        tz_str = m.group(8)
        if tz_str is None or tz_str == "Z":
            tz = timezone.utc
        else:
            sign  = 1 if tz_str[0] == "+" else -1
            hh, mm = int(tz_str[1:3]), int(tz_str[4:6])
            tz = timezone(timedelta(hours=sign * hh, minutes=sign * mm))

        # Build datetime at microsecond granularity, then add leftover ns
        microseconds, leftover_ns = divmod(total_ns_frac, _NS_PER_US)
        dt = datetime(year, month, day, hour, minute, second,
                      microseconds, tzinfo=tz)
        ns = _dt_to_ns(dt, extra_ns=leftover_ns)
        return TimestampV0(c_uint64(ns))
