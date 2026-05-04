"""Foretias v1 — Timebeing base class.

All time beings inherit from this class, which provides identity (tbid, tbn)
and a reference to the family that owns them.

.. deprecated:: Use ``foretias_p2p.PyTimeFamily`` (Rust) instead.
"""

from __future__ import annotations

import warnings

warnings.warn(
    "foretias.timebeing is deprecated. Use foretias (Rust-backed) instead.",
    DeprecationWarning,
    stacklevel=2,
)

import uuid
from typing import Any


class Timebeing:
    """Base class for all time beings.

    Args:
        tbid: Internal identity (UUID v4 bytes). Auto-generated if None.
        name: TBN prefix — final name becomes ``"Time Being {tbid.hex()}"``.
    """

    def __init__(
        self,
        tbid: bytes | None = None,
        name: str = "timebeing",
    ) -> None:
        self._family: Any = None
        self.tbid = tbid or uuid.uuid4().bytes
        self.tbn = f"Time Being {self.tbid.hex()}"

    @property
    def family(self) -> Any:
        return self._family

    @family.setter
    def family(self, value: Any) -> None:
        self._family = value
