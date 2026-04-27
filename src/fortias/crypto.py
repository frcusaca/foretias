"""Fortias v1 — Cryptographic primitives.

Ed25519 signatures over raw bytes (no pre-hash inside the signature input).
SHA-256 for content hashing.

.. deprecated:: Use ``fortias_p2p.CryptoServer`` (Rust) instead.
"""

from __future__ import annotations

import warnings

warnings.warn(
    "fortias.crypto is deprecated. Use fortias (Rust-backed) instead.",
    DeprecationWarning,
    stacklevel=2,
)

import hashlib

from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives.asymmetric.ed25519 import (
    Ed25519PrivateKey,
    Ed25519PublicKey,
)


def sha256(data: bytes | str) -> bytes:
    """SHA-256 digest of *data*.

    Args:
        data: Raw bytes or string to hash.

    Returns:
        32-byte digest.
    """
    if isinstance(data, str):
        data = data.encode("utf-8")
    return hashlib.sha256(data).digest()


def sha256_hex(data: bytes | str) -> str:
    """Hex-encoded SHA-256 digest of *data*."""
    if isinstance(data, str):
        data = data.encode("utf-8")
    return hashlib.sha256(data).hexdigest()


def generate_keypair() -> tuple[bytes, bytes]:
    """Generate a fresh Ed25519 keypair as 32-byte raw buffers.

    Returns:
        ``(private_key_bytes, public_key_bytes)`` -- each 32 bytes.
    """
    sk = Ed25519PrivateKey.generate()
    pk = sk.public_key()
    return sk.private_bytes_raw(), pk.public_bytes_raw()


def sign(payload: bytes, secret_key: bytes) -> bytes:
    """Sign *payload* with an Ed25519 private key.

    Args:
        payload:    Raw bytes to sign.
        secret_key: 32-byte Ed25519 private key in raw form.

    Returns:
        64-byte Ed25519 signature.

    Raises:
        ValueError: if *secret_key* is not a valid Ed25519 private key.
    """
    try:
        sk = Ed25519PrivateKey.from_private_bytes(secret_key)
    except (ValueError, TypeError) as exc:
        raise ValueError(f"Invalid Ed25519 private key: {exc}") from exc
    return sk.sign(payload)


def verify(payload: bytes, signature: bytes, public_key: bytes) -> bool:
    """Verify an Ed25519 signature over *payload*.

    Args:
        payload:     Bytes that were signed.
        signature:   64-byte Ed25519 signature.
        public_key:  32-byte Ed25519 public key.

    Returns:
        ``True`` if valid, ``False`` otherwise. Malformed input yields ``False``.
    """
    try:
        pk = Ed25519PublicKey.from_public_bytes(public_key)
        pk.verify(signature, payload)
    except (InvalidSignature, ValueError, TypeError):
        return False
    return True
