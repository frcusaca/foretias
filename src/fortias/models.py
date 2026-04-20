"""
Fortias Protocol v0.0.1 — Data models.

All public wire types are frozen dataclasses — immutable, hashable, and safe
to share across the protocol boundary.

Surface
-------
/stamp
    :class:`StampRequest`   → :class:`StampResponse`

/verify
    :class:`VerifyRequest`  → :class:`VerifyResponse`   (with :class:`VerifyResult` per check)

Verification calendar (internal, consumed by :class:`~fortias.protocol.Fortias`)
    :class:`VerificationCalendar`
    :class:`VerificationCalendarResponse`

Request-hash pre-check
----------------------
Both request types carry a client-supplied ``stamp_request_hash`` (hex SHA-256
of *payload*).  The server re-computes SHA-256(payload) and rejects the
request with :class:`~fortias.exceptions.PayloadHashMismatchError` before any
further processing.  The hash is then echoed back in the response.

Echo field
----------
Both requests accept an optional ``echo`` attribute (default ``None``) whose
value is copied verbatim into the response.  ``echo is None`` means the
client did not supply one; serializers are expected to omit the field from
the wire representation in that case.

Status field
------------
Every response carries a ``status`` string.  It is ``"normal"`` when the
request executed as designed, or ``"abnormal: <message>"`` when some
condition prevented normal execution (e.g. a hash-mismatch pre-check).
A verification that runs to completion and reports ``valid=False`` is still
``"normal"`` — the check ran; it simply failed.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from .timestamp import TimestampV0


# ---------------------------------------------------------------------------
# /stamp
# ---------------------------------------------------------------------------


@dataclass(frozen=True, kw_only=True)
class StampRequest:
    """Input to /stamp.

    Attributes:
        payload:            Raw byte-array to be stamped.
        stamp_request_hash: Hex-encoded SHA-256 digest of *payload*, supplied
                            by the client.  The server re-computes and
                            rejects mismatches before signing.
        echo:               Optional opaque value round-tripped into
                            :class:`StampResponse`.  ``None`` means
                            "not provided".
    """

    payload: bytes
    stamp_request_hash: str
    echo: Any | None = None


@dataclass(frozen=True, kw_only=True)
class StampResponse:
    """Output of /stamp — the stamp itself.

    The signature is Ed25519 over *payload* directly (not over the hash).
    ``stamp_request_hash`` is the SHA-256 digest of the payload, confirmed
    by the server and carried here for the client's convenience.

    Attributes:
        TBID:               UUID4 correlation id.
        fortias_version:    Protocol version of the signer (``"0.0.1"``).
        fortias_timestamp:  UTC timestamp at the moment of signing
                            (ISO-8601, microsecond precision).
        stamp_request_hash: Hex-encoded SHA-256 digest of the signed payload.
        signature:          Hex-encoded Ed25519 signature over the payload.
        echo:               Echoed from the request; ``None`` if the request
                            did not supply one.
    """

    TBID: str
    fortias_version: str
    fortias_timestamp: str
    stamp_request_hash: str
    signature: str
    echo: Any | None = None
    status: str = "normal"


# ---------------------------------------------------------------------------
# /verify
# ---------------------------------------------------------------------------


@dataclass(frozen=True, kw_only=True)
class VerifyRequest:
    """Input to /verify.

    Attributes:
        payload:            The *original* bytes that were stamped.
        stamp:              The :class:`StampResponse` returned by /stamp,
                            echoed back.
        stamp_request_hash: Hex-encoded SHA-256 digest of *payload*, supplied
                            by the client.  The server re-computes and
                            rejects mismatches before verifying.
        echo:               Optional opaque value round-tripped into
                            :class:`VerifyResponse`.
    """

    payload: bytes
    stamp: StampResponse
    stamp_request_hash: str
    echo: Any | None = None


@dataclass(frozen=True)
class VerifyResult:
    """Outcome of a single named check within /verify."""

    name: str
    passed: bool
    detail: str = ""


@dataclass(frozen=True, kw_only=True)
class VerifyResponse:
    """Output of /verify.

    Attributes:
        TBID:               UUID4 correlation id for this verification.
        fortias_version:    Protocol version of the verifying server.
        fortias_timestamp:  UTC timestamp at which verification completed.
        stamp_request_hash: The request hash echoed back (already validated).
        valid:              ``True`` iff every check in *results* passed.
        results:            Ordered per-check outcomes.
        echo:               Echoed from the request; ``None`` if the request
                            did not supply one.
    """

    TBID: str
    fortias_version: str
    fortias_timestamp: str
    stamp_request_hash: str
    valid: bool
    results: tuple[VerifyResult, ...]
    echo: Any | None = None
    status: str = "normal"

    @classmethod
    def from_results(
        cls,
        results: list[VerifyResult],
        *,
        TBID: str,
        fortias_version: str,
        fortias_timestamp: str,
        stamp_request_hash: str,
        echo: Any | None = None,
        status: str = "normal",
    ) -> "VerifyResponse":
        return cls(
            TBID=TBID,
            fortias_version=fortias_version,
            fortias_timestamp=fortias_timestamp,
            stamp_request_hash=stamp_request_hash,
            valid=all(r.passed for r in results),
            results=tuple(results),
            echo=echo,
            status=status,
        )


# ---------------------------------------------------------------------------
# Verification calendar
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class VerificationCalendar:
    """A single entry in the verification calendar — the tick at-or-before
    the query timestamp.

    Attributes:
        calendar_timestamp: The tick's :class:`~fortias.timestamp.TimestampV0`
                            (the greatest calendar tick ``≤`` the query).
        verifier:           32-byte raw Ed25519 public key valid at
                            *calendar_timestamp*.
    """

    calendar_timestamp: TimestampV0
    verifier: bytes


@dataclass(frozen=True, kw_only=True)
class VerificationCalendarResponse:
    """Envelope returned by
    :meth:`~fortias.calendar.Calendar.retrieve_for_verification`.

    Attributes:
        TBID:                  UUID4 correlation id for this lookup.
        fortias_version:       Protocol version that produced this response.
        fortias_timestamp:     UTC timestamp at which the lookup was served.
        verification_calendar: The located :class:`VerificationCalendar`, or
                               ``None`` if no tick ``≤`` the query timestamp
                               exists.
    """

    TBID: str
    fortias_version: str
    fortias_timestamp: str
    verification_calendar: VerificationCalendar | None
