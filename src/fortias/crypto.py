"""
Fortias Protocol v0.0.1 — Cryptographic primitives.

Signatures are Ed25519 over the raw payload bytes (no pre-hash inside the
signature input).  Public keys stored in the verification
:class:`~fortias.calendar.Calendar` are the 32-byte raw form returned by
:meth:`cryptography.hazmat.primitives.asymmetric.ed25519.Ed25519PublicKey.public_bytes_raw`.

The integrity hash kept in :class:`~fortias.models.StampResponse`
(``stamp_request_hash``) is a separate SHA-256 digest of the payload — it lets
a requester re-derive the same value client-side and compare, without pulling
the whole payload through the signature check.
"""

from __future__ import annotations

import hashlib

from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives.asymmetric.ed25519 import (
    Ed25519PrivateKey,
    Ed25519PublicKey,
)

from .exceptions import InvalidKeyError


PROTOCOL_VERSION = "0.0.1"


def hash_payload_for_version(payload: bytes, version: str) -> str:
    """Hex-encoded payload digest selected by protocol version.

    Args:
        payload: Raw bytes to digest.
        version: Protocol version string.  Only ``"0.0.1"`` is supported and
                 selects SHA-256.

    Returns:
        Hex-encoded digest string.

    Raises:
        :class:`ValueError`: if *version* is not a recognised protocol version.
    """
    if version == PROTOCOL_VERSION:
        return hashlib.sha256(payload).hexdigest()
    raise ValueError(f"Unsupported protocol version: {version!r}")


def generate_keypair() -> tuple[bytes, bytes]:
    """Generate a fresh Ed25519 keypair as 32-byte raw buffers.

    Returns:
        ``(private_key_bytes, public_key_bytes)`` — each 32 bytes.
    """
    sk = Ed25519PrivateKey.generate()
    pk = sk.public_key()
    return sk.private_bytes_raw(), pk.public_bytes_raw()


def sign(payload: bytes, secret_key: bytes) -> str:
    """Sign *payload* with an Ed25519 private key.

    Args:
        payload:    Raw bytes to sign.
        secret_key: 32-byte Ed25519 private key in raw form.

    Returns:
        Hex-encoded 64-byte signature.

    Raises:
        :class:`~fortias.exceptions.InvalidKeyError`: if *secret_key* is not a
            valid Ed25519 private key.
    """
    try:
        sk = Ed25519PrivateKey.from_private_bytes(secret_key)
    except (ValueError, TypeError) as exc:
        raise InvalidKeyError(f"Invalid Ed25519 private key: {exc}") from exc
    return sk.sign(payload).hex()


def verify_signature(payload: bytes, signature_hex: str, verifier: bytes) -> bool:
    """Verify an Ed25519 signature over *payload*.

    Args:
        payload:       Bytes that were signed.
        signature_hex: Hex-encoded signature produced by :func:`sign`.
        verifier:      32-byte Ed25519 public key in raw form.

    Returns:
        ``True`` if the signature is valid, ``False`` otherwise.  A malformed
        key, malformed hex signature, or cryptographic mismatch all yield
        ``False`` rather than raising.
    """
    try:
        pk = Ed25519PublicKey.from_public_bytes(verifier)
        pk.verify(bytes.fromhex(signature_hex), payload)
    except (InvalidSignature, ValueError, TypeError):
        return False
    return True
