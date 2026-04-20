"""Fortias v1 — Data models."""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class TickRecord:
    """A single tick entry in a time being's calendar.

    Attributes:
        tick_number: Nanoseconds since Unix epoch; the tick's time boundary.
        public_key: Ed25519 public key for this tick (32 bytes).
        new_fortis: Signature of previous tick's public_key, signed by
                    previous tick's private key (32 bytes).
        old_fortis: Signature of this tick's public_key, signed by this
                    tick's private key. ``None`` for the genesis record.
    """

    tick_number: int
    public_key: bytes
    new_fortis: bytes
    old_fortis: bytes | None


@dataclass(frozen=True)
class Fortis:
    """A Fortias TimeStamp — the stamped artifact.

    Attributes:
        tick_number: The tick during which the content was signed.
        my_content_hash: SHA-256 digest of the signed content.
        signature: Ed25519 signature (32 bytes).
        tbid: The time being's internal UUID v4 identity (32 bytes).
        echo: Pass-through of the original content string; not part of
              the cryptographic signature input.
        tbn: Human-readable external name of the time being.
    """

    tick_number: int
    my_content_hash: bytes
    signature: bytes
    tbid: bytes
    echo: str
    tbn: str
