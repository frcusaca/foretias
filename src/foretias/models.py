"""Foretias v1 — Data models.

.. deprecated:: Use ``foretias_p2p.PyForetis`` / ``foretias_p2p.PyTickRecord`` (Rust) instead.
"""

from __future__ import annotations

import warnings

warnings.warn(
    "foretias.models is deprecated. Use foretias (Rust-backed) instead.",
    DeprecationWarning,
    stacklevel=2,
)

from dataclasses import dataclass


@dataclass(frozen=True)
class TickRecord:
    """A single tick entry in a time being's calendar.

    Attributes:
        tick_number: Nanoseconds since Unix epoch; the tick's starting time boundary. This tick represents chronon containing nothing before this moment.
        public_key: Ed25519 public key for this tick (32 bytes).
        forward_foretis: Auto-attestation signature by this tick's
                        *previous* private key.
        backward_foretis: Auto-attestation signature by this tick's
                          private key.
    """

    tick_number: int
    public_key: bytes
    forward_foretis: bytes
    backward_foretis: bytes

    def __post_init__(self) -> None:
        if self.forward_foretis is None:
            raise TypeError("forward_foretis must never be None")
        if self.backward_foretis is None:
            raise TypeError("backward_foretis must never be None")


@dataclass(frozen=True)
class Foretis:
    """A Foretias TimeStamp — the stamped artifact.

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
