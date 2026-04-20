"""
Fortias Protocol v0.0.1 — Protocol implementation.

The public surface is the :class:`Fortias` class, which owns an Ed25519
signing key and a :class:`~fortias.calendar.Calendar` and exposes the two
protocol endpoints as instance methods::

    fortias = Fortias(secret_key=sk_bytes, calendar=Calendar())
    fortias.stamp(StampRequest(payload=..., stamp_request_hash=...))
        -> StampResponse
    fortias.verify(VerifyRequest(payload=..., stamp=..., stamp_request_hash=...))
        -> VerifyResponse

Both endpoints always return a response — including on pre-check failure.
The returned response's ``status`` field is ``"normal"`` on the happy path
or ``"abnormal: <message>"`` when some condition (e.g. a hash-mismatch
pre-check, an unparseable stamp timestamp) prevented normal execution.

Signing uses Ed25519 over the raw payload (see :mod:`fortias.crypto`).
Verification resolves the authoritative public key from the calendar by
locating the tick whose timestamp is ``≤`` the stamp's ``fortias_timestamp``
— the calendar acts as a time-indexed public-key registry.
"""

from __future__ import annotations

import uuid
from datetime import datetime, timezone

from .calendar import Calendar
from .crypto import (
    PROTOCOL_VERSION,
    hash_payload_for_version,
    sign,
    verify_signature,
)
from .exceptions import FortiasError, InvalidKeyError, PayloadHashMismatchError
from .models import (
    StampRequest,
    StampResponse,
    VerifyRequest,
    VerifyResponse,
    VerifyResult,
)
from .timestamp import TimestampFactoryV0


_ED25519_SK_LEN = 32


def _require_key(secret_key: bytes) -> None:
    if not isinstance(secret_key, (bytes, bytearray)) or len(secret_key) != _ED25519_SK_LEN:
        raise InvalidKeyError(
            f"secret_key must be {_ED25519_SK_LEN} raw bytes; "
            f"got {type(secret_key).__name__} of length "
            f"{len(secret_key) if hasattr(secret_key, '__len__') else 'N/A'}."
        )


def _utc_now_iso() -> str:
    """Return current UTC time as a nanosecond-precision ISO 8601 string.

    Uses :class:`~fortias.timestamp.TimestampV0` to ensure the output
    format matches the nanosecond-precision uint64 representation used
    throughout the protocol.
    """
    return TimestampFactoryV0.from_datetime(
        datetime.now(tz=timezone.utc)
    ).to_iso_string()


def _make_tbid() -> str:
    return str(uuid.uuid4())


def _require_matching_hash(payload: bytes, claimed_hash: str) -> str:
    """Raise if ``SHA-256(payload) != claimed_hash``; otherwise return the hash.

    TODO: Consider whether this should hash the payload (SHA-256) and
    compare with the hash that came in with it, rather than relying on
    the client-supplied hash. This would provide an additional layer of
    integrity checking.
    """
    actual = hash_payload_for_version(payload, PROTOCOL_VERSION)
    if actual != claimed_hash:
        raise PayloadHashMismatchError(
            f"stamp_request_hash does not match payload: "
            f"claimed={claimed_hash!r}, actual={actual!r}."
        )
    return actual


class Fortias:
    """Fortias protocol implementation (v0.0.1).

    Args:
        secret_key: 32-byte raw Ed25519 private key used to sign
                    :meth:`stamp` responses.
        calendar:   A :class:`~fortias.calendar.Calendar` used by
                    :meth:`verify` to resolve the public key that was
                    authoritative at a stamp's ``fortias_timestamp``.
    """

    def __init__(self, secret_key: bytes, calendar: Calendar) -> None:
        try:
            _require_key(secret_key)
        except InvalidKeyError as exc:
            print(f"Error: {exc}")
            raise
        self._secret_key = bytes(secret_key)
        self._calendar = calendar

    # ------------------------------------------------------------------
    # /stamp
    # ------------------------------------------------------------------

    def stamp(self, request: StampRequest) -> StampResponse:
        """Produce a signed stamp over *request.payload*.

        Returns a :class:`~fortias.models.StampResponse`.  On pre-check
        failure (hash mismatch, invalid signing key, etc.) the response's
        ``status`` field is ``"abnormal: <message>"``, ``signature`` is
        empty, and ``stamp_request_hash`` carries the value the client
        claimed — so the caller can correlate request and response.
        """
        try:
            payload_hash = _require_matching_hash(
                request.payload, request.stamp_request_hash
            )
            signature = sign(request.payload, self._secret_key)
        except FortiasError as exc:
            return StampResponse(
                TBID=_make_tbid(),
                fortias_version=PROTOCOL_VERSION,
                fortias_timestamp=_utc_now_iso(),
                stamp_request_hash=request.stamp_request_hash,
                signature="",
                echo=request.echo,
                status=f"abnormal: {exc}",
            )

        return StampResponse(
            TBID=_make_tbid(),
            fortias_version=PROTOCOL_VERSION,
            fortias_timestamp=_utc_now_iso(),
            stamp_request_hash=payload_hash,
            signature=signature,
            echo=request.echo,
            status="normal",
        )

    # ------------------------------------------------------------------
    # /verify
    # ------------------------------------------------------------------

    def verify(self, request: VerifyRequest) -> VerifyResponse:
        """Verify *request.stamp* against *request.payload*.

        Pre-check (converted to abnormal status on failure):
            SHA-256(request.payload) must equal ``request.stamp_request_hash``.

        Result checks (all must pass for ``valid=True``):
            1. ``version_check``     — stamp version equals ``PROTOCOL_VERSION``.
            2. ``payload_integrity`` — SHA-256(payload) equals the stamp's
               ``stamp_request_hash``.
            3. ``calendar_lookup``   — a calendar tick exists with
               ``tick_ts ≤ stamp.fortias_timestamp``.
            4. ``signature``         — Ed25519 verify under the pubkey from (3).

        A failed check yields ``valid=False`` with ``status="normal"`` —
        the verification ran to completion, the answer is simply "no".
        ``status="abnormal: ..."`` is reserved for conditions that stopped
        verification from running at all (hash-mismatch pre-check, an
        unparseable ``stamp.fortias_timestamp``, etc.).
        """
        try:
            payload_hash = _require_matching_hash(
                request.payload, request.stamp_request_hash
            )
            query_ts = TimestampFactoryV0.from_iso_string(
                request.stamp.fortias_timestamp
            )
        except (FortiasError, ValueError) as exc:
            return VerifyResponse(
                TBID=_make_tbid(),
                fortias_version=PROTOCOL_VERSION,
                fortias_timestamp=_utc_now_iso(),
                stamp_request_hash=request.stamp_request_hash,
                valid=False,
                results=(),
                echo=request.echo,
                status=f"abnormal: {exc}",
            )

        results: list[VerifyResult] = []
        stamp = request.stamp

        # --- 1. version check -----------------------------------------------
        version_ok = stamp.fortias_version == PROTOCOL_VERSION
        results.append(VerifyResult(
            name="version_check",
            passed=version_ok,
            detail=(
                f"version matches ({PROTOCOL_VERSION!r})"
                if version_ok
                else (
                    f"version mismatch: stamp={stamp.fortias_version!r}, "
                    f"expected={PROTOCOL_VERSION!r}"
                )
            ),
        ))

        # --- 2. payload integrity vs. the stamp ------------------------------
        hash_ok = payload_hash == stamp.stamp_request_hash
        results.append(VerifyResult(
            name="payload_integrity",
            passed=hash_ok,
            detail=(
                "payload hash matches stamp"
                if hash_ok
                else (
                    f"hash mismatch against stamp: payload={payload_hash!r}, "
                    f"stamp={stamp.stamp_request_hash!r}"
                )
            ),
        ))

        # --- 3. calendar lookup ---------------------------------------------
        vcal_response = self._calendar.retrieve_for_verification(query_ts, tbid=_make_tbid())
        vcal = vcal_response.verification_calendar
        calendar_ok = vcal is not None
        results.append(VerifyResult(
            name="calendar_lookup",
            passed=calendar_ok,
            detail=(
                f"verifier located at calendar tick "
                f"{vcal.calendar_timestamp.to_iso_string()}"
                if calendar_ok
                else (
                    f"no calendar tick at or before "
                    f"{stamp.fortias_timestamp!r}"
                )
            ),
        ))

        # --- 4. signature ---------------------------------------------------
        if calendar_ok:
            sig_ok = verify_signature(
                payload=request.payload,
                signature_hex=stamp.signature,
                verifier=vcal.verifier,
            )
            sig_detail = (
                "signature valid"
                if sig_ok
                else "signature invalid under calendar-published key"
            )
        else:
            sig_ok = False
            sig_detail = "cannot verify signature: no verifier available"
        results.append(VerifyResult(
            name="signature",
            passed=sig_ok,
            detail=sig_detail,
        ))

        return VerifyResponse.from_results(
            results,
            TBID=_make_tbid(),
            fortias_version=PROTOCOL_VERSION,
            fortias_timestamp=_utc_now_iso(),
            stamp_request_hash=payload_hash,
            echo=request.echo,
            status="normal",
        )
