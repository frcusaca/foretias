"""Thin Python client wrapping the Rust PyO3 bindings.

Provides a clean, simple API for stamping and verifying content
without requiring the full TimeFamilyServer infrastructure.

Usage:
    from foretias.thin_client import ForetiasClient

    client = ForetiasClient("my-app")
    stamp = client.stamp("hello world")
    assert client.verify("hello world", stamp)
"""

from __future__ import annotations

import json
from typing import Any

from foretias_p2p import PyForetis, PyTimeFamily


class ForetiasClient:
    """Lightweight client for content attestation via Rust PyO3 bindings.

    Wraps a single :class:`PyTimeFamily` instance for stamping and
    verifying arbitrary content. All cryptographic operations are
    delegated to the compiled Rust layer — no pure-Python crypto.

    Example::

        client = ForetiasClient("my-service")
        record = client.stamp("important data")
        assert client.verify("important data", record)
        assert client.public_key()
    """

    def __init__(self, name: str = "thin-client") -> None:
        """Create a new client with a fresh identity.

        Args:
            name: The time-branch name (tbn) identifying this client.
        """
        self._rust: PyTimeFamily = PyTimeFamily(tbn=name)

    # ---- stamping --------------------------------------------------------

    def stamp(self, content: str | bytes, echo: str = "") -> dict[str, Any]:
        """Stamp content, producing a signed attestation.

        The content is hashed (SHA-256) and signed with the client's
        Ed25519 key. The resulting attestation is appended to the
        internal calendar and the tick counter advances.

        Args:
            content: The payload to attest. Strings are UTF-8 encoded.
            echo: Optional free-form string embedded in the attestation.

        Returns:
            A JSON-serializable dict containing the attestation fields
            (tick_number, content_hash, signature, tbid, echo, tbn,
            time_being_reference_time).
        """
        raw = content.encode("utf-8") if isinstance(content, str) else content
        foretis = self._rust.stamp(raw, echo)
        return json.loads(foretis.to_json())

    # ---- verification ----------------------------------------------------

    def verify(self, content: str | bytes, foretis_dict: dict[str, Any]) -> bool:
        """Verify content against a previously-produced attestation.

        Reconstructs a :class:`PyForetis` from the dict, then delegates
        to the Rust ``verify()`` binding.

        Args:
            content: The payload to verify. Strings are UTF-8 encoded.
            foretis_dict: The attestation dict (as returned by :meth:`stamp`).

        Returns:
            ``True`` if the signature and content hash match.
        """
        raw = content.encode("utf-8") if isinstance(content, str) else content
        foretis = PyForetis.from_json(json.dumps(foretis_dict))
        return self._rust.verify(raw, foretis)

    # ---- tick control ----------------------------------------------------

    def tick(self) -> int:
        """Advance the tick counter by stamping an empty payload.

        Useful for creating a calendar entry without meaningful content
        (e.g. heartbeat or synchronization tick).

        Returns:
            The new tick number after advancing.
        """
        foretis = self._rust.stamp(b"", "")
        return foretis.tick_number

    def current_tick(self) -> int:
        """Return the current tick counter value.

        Returns:
            The tick number (0 if no stamps have been made).
        """
        return self._rust.get_current_tick()

    # ---- calendar --------------------------------------------------------

    def calendar(self) -> list[dict[str, Any]]:
        """Return all calendar tick records as a list of dicts.

        Each dict contains the serialized tick record fields
        (tick_number, public_key, forward_foretis, backward_foretis).

        Returns:
            A list of JSON-serializable dicts, one per tick.
        """
        cal = self._rust.get_calendar()
        data = json.loads(cal.to_json())
        return data.get("ticks", [])

    # ---- identity --------------------------------------------------------

    def public_key(self) -> str:
        """Return the Ed25519 public key as a hex string.

        Returns:
            64-character hex string (32 bytes).
        """
        return self._rust.get_public_key()

    def tbid(self) -> str:
        """Return the time-being identifier as a hex string.

        Returns:
            32-character hex string (16 bytes).
        """
        return self._rust.get_tbid()
