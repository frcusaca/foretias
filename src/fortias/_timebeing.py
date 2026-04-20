"""Fortias v1 — Pure functional time being operations.

All methods are static, pure, and side-effect-free. They take state as
arguments and return new state or booleans. The mutable
:class:`~fortias.timebeing_family.TimebeingFamily` class calls these
functions behind a mutex.
"""

from __future__ import annotations

import struct

from .crypto import generate_keypair, sha256, sign, verify
from .models import Fortis, TickRecord


def _concat(tbid: bytes, tick_number: int, content: bytes | str) -> bytes:
    """Concatenate tbid, tick_number as uint64 BE, and content for signature input."""
    if isinstance(content, bytes):
        content_bytes = content
    else:
        content_bytes = content.encode("utf-8")
    return tbid + struct.pack(">Q", tick_number) + content_bytes


class _timebeing:
    """Pure functional operations for the Fortias protocol.

    Every method is a pure function with no side effects.
    """

    @staticmethod
    def _stamp(
        content: bytes | str,
        tbid: bytes,
        tick_number: int,
        private_key: bytes,
        echo: str = "",
        tbn: str = "",
    ) -> Fortis:
        """Stamp *content* with Ed25519 under the given tick key.

        Process:
            1. Hash the content: content_hash = SHA-256(content).
            2. Concatenate the signature input: concat(tbid, tick_number, content).
            3. Sign: signature = Ed25519_sign(signature_input, private_key).
            4. Return a Fortis containing (tick_number, content_hash, signature, tbid, echo, tbn).

        Ed25519 has internal hashing, so the raw concatenation is signed
        directly — no pre-hash of the signature input.
        """
        content_bytes = content if isinstance(content, bytes) else content.encode("utf-8")
        content_hash = sha256(content_bytes)
        signature_input = _concat(tbid, tick_number, content_bytes)
        signature = sign(signature_input, private_key)
        return Fortis(
            tick_number=tick_number,
            my_content_hash=content_hash,
            signature=signature,
            tbid=tbid,
            echo=echo,
            tbn=tbn,
        )

    @staticmethod
    def _tick(
        tbid: bytes,
        current_tick_record: TickRecord,
        current_private_key: bytes,
    ) -> tuple[TickRecord, bytes]:
        """Advance from the current tick to a new tick.

        Tick transition algorithm:
            1. Generate keypair (new_sk, new_pk). tick_number = now (nanoseconds).
            2. new_fortis = _stamp(content=new_pk, tbid=TBID, tick_number=new_tick, private_key=new_sk)
            3. old_fortis = _stamp(content=new_pk, tbid=TBID, tick_number=new_tick, private_key=cur_sk)
            4. Append new TickRecord.
            5. Destroy cur_sk.
            6. New keypair becomes active.

        Args:
            tbid: Time being identity.
            current_tick_record: The current (genesis or last) TickRecord.
            current_private_key: The current private key (destroyed after).

        Returns:
            (new_tick_record, new_private_key).
        """
        new_tick_number = _now_ns()
        new_sk, new_pk = generate_keypair()

        cur_pk = current_tick_record.public_key
        cur_tick = current_tick_record.tick_number

        # new_fortis: new key signs new public key (self-attestation)
        new_fortis = _timebeing._stamp(
            content=new_pk,
            tbid=tbid,
            tick_number=new_tick_number,
            private_key=new_sk,
        )

        # old_fortis: current key signs new public key (cross-attestation to next tick)
        old_fortis = _timebeing._stamp(
            content=new_pk,
            tbid=tbid,
            tick_number=new_tick_number,
            private_key=current_private_key,
        )

        new_record = TickRecord(
            tick_number=new_tick_number,
            public_key=new_pk,
            new_fortis=new_fortis.signature,
            old_fortis=old_fortis.signature,
        )

        return new_record, new_sk

    @staticmethod
    def _verify_pair(B: TickRecord, A: TickRecord, tbid: bytes) -> bool:
        """Verify two consecutive tick records are mutually attesting.

        B is the later tick, A is the earlier tick.

        Checks both cross-stamp signatures:
            1. A.new_fortis verifies against A.public_key.
            2. B.old_fortis verifies against A.public_key (A's key attested to B).
        """
        # A.new_fortis: A's private key signed the content for this tick
        new_fortis_input = _concat(tbid, A.tick_number, A.public_key)
        if not verify(new_fortis_input, A.new_fortis, A.public_key):
            return False

        # B.old_fortis: A's private key signed concat(tbid, B.tick_number, B.public_key)
        if B.old_fortis is None:
            return False
        old_fortis_input = _concat(tbid, B.tick_number, B.public_key)
        if not verify(old_fortis_input, B.old_fortis, A.public_key):
            return False

        return True

    @staticmethod
    def _verify_chain(
        ticks: list[TickRecord],
        tbid: bytes,
        return_failures: bool = False,
    ) -> bool | tuple[bool, list[int] | None]:
        """Verify all consecutive pairs in the chain.

        Args:
            ticks: List of TickRecord entries in order.
            tbid: Time being identity (used to reconstruct signature inputs).
            return_failures: If True, returns (bool, list[int]) with indexes
                           of failed pairs.

        Returns:
            bool if *return_failures* is False.
            (bool, list[int]) if True.
        """
        if len(ticks) < 2:
            return (True, []) if return_failures else True

        failures: list[int] = []
        for i in range(len(ticks) - 1):
            if not _timebeing._verify_pair(ticks[i + 1], ticks[i], tbid):
                failures.append(i)

        ok = len(failures) == 0
        if return_failures:
            return (ok, failures)
        return ok

    @staticmethod
    def _verify(
        content: bytes | str,
        fortis: Fortis,
        calendar_ticks: list[TickRecord],
        next_tick_number: int | None = None,
    ) -> bool | tuple[bool, bool | None]:
        """Verify a Fortis against content and calendar.

        Args:
            content: The original content to verify.
            fortis: The Fortis artifact to verify.
            calendar_ticks: The calendar tick records (for key lookup).
            next_tick_number: If provided, also check window closed.

        Returns:
            If next_tick_number is None:
                bool: True if signature and content hash are valid.
            If next_tick_number is not None:
                (sig_valid, window_closed):
                    sig_valid: True if signature and content hash are valid.
                    window_closed: True if next tick exists (key destroyed),
                                   False if not (still active).
                                   None if sig_valid is False.
        """
        # 1. Look up the public key for the claimed tick
        pk = None
        for t in calendar_ticks:
            if t.tick_number == fortis.tick_number:
                pk = t.public_key
                break
        if pk is None:
            if next_tick_number is not None:
                return (False, None)
            return False

        # 2. Verify content hash
        content_bytes = content if isinstance(content, bytes) else content.encode("utf-8")
        content_hash = sha256(content_bytes)
        if content_hash != fortis.my_content_hash:
            if next_tick_number is not None:
                return (False, None)
            return False

        # 3. Verify signature
        signature_input = _concat(fortis.tbid, fortis.tick_number, content_bytes)
        sig_valid = verify(signature_input, fortis.signature, pk)

        if next_tick_number is None:
            return sig_valid

        if not sig_valid:
            return (False, None)

        # 4. Window check: does next_tick_number exist in calendar?
        next_exists = any(t.tick_number == next_tick_number for t in calendar_ticks)
        return (True, next_exists)


def _now_ns() -> int:
    """Return current Unix epoch time in nanoseconds."""
    import time
    return int(time.time() * 1_000_000_000)
