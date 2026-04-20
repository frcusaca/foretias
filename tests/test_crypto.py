"""Unit tests for fortias.crypto."""

from __future__ import annotations

import hashlib

import pytest

from fortias.crypto import (
    PROTOCOL_VERSION,
    generate_keypair,
    hash_payload_for_version,
    sign,
    verify_signature,
)
from fortias.exceptions import InvalidKeyError


class TestHashPayloadForVersion:
    def test_sha256_for_v001(self):
        payload = b"hello, fortias"
        assert hash_payload_for_version(payload, "0.0.1") == \
            hashlib.sha256(payload).hexdigest()

    def test_empty_payload_ok(self):
        assert hash_payload_for_version(b"", "0.0.1") == \
            hashlib.sha256(b"").hexdigest()

    def test_rejects_unknown_version(self):
        with pytest.raises(ValueError):
            hash_payload_for_version(b"x", "9.9.9")

    def test_protocol_version_constant(self):
        assert PROTOCOL_VERSION == "0.0.1"


class TestGenerateKeypair:
    def test_shape(self):
        sk, pk = generate_keypair()
        assert isinstance(sk, bytes) and len(sk) == 32
        assert isinstance(pk, bytes) and len(pk) == 32

    def test_keys_are_random(self):
        sk1, pk1 = generate_keypair()
        sk2, pk2 = generate_keypair()
        assert sk1 != sk2
        assert pk1 != pk2


class TestSignAndVerify:
    def test_roundtrip_valid(self):
        sk, pk = generate_keypair()
        payload = b"some bytes"
        sig = sign(payload, sk)
        assert len(bytes.fromhex(sig)) == 64  # Ed25519 signature size
        assert verify_signature(payload, sig, pk) is True

    def test_wrong_key_fails(self):
        sk, _ = generate_keypair()
        _, other_pk = generate_keypair()
        sig = sign(b"msg", sk)
        assert verify_signature(b"msg", sig, other_pk) is False

    def test_tampered_payload_fails(self):
        sk, pk = generate_keypair()
        sig = sign(b"original", sk)
        assert verify_signature(b"tampered", sig, pk) is False

    def test_malformed_signature_hex_fails_silently(self):
        _, pk = generate_keypair()
        assert verify_signature(b"x", "not-hex!!", pk) is False

    def test_malformed_verifier_bytes_fails_silently(self):
        sk, _ = generate_keypair()
        sig = sign(b"msg", sk)
        assert verify_signature(b"msg", sig, b"\x00" * 16) is False  # wrong length

    def test_bad_secret_key_raises_invalid_key_error(self):
        with pytest.raises(InvalidKeyError):
            sign(b"msg", b"\x00" * 16)  # wrong length
