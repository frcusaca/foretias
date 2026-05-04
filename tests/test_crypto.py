"""Unit tests for foretias.crypto."""

from __future__ import annotations

import hashlib

import pytest

from foretias.crypto import (
    generate_keypair,
    sha256,
    sha256_hex,
    sign,
    verify,
)


class TestSha256:
    def test_bytes_input(self):
        result = sha256(b"hello")
        assert result == hashlib.sha256(b"hello").digest()
        assert isinstance(result, bytes)
        assert len(result) == 32

    def test_str_input(self):
        result = sha256("hello")
        assert result == hashlib.sha256(b"hello").digest()

    def test_empty_input(self):
        result = sha256(b"")
        assert result == hashlib.sha256(b"").digest()

    def test_hex_output(self):
        result = sha256_hex(b"hello")
        assert result == hashlib.sha256(b"hello").hexdigest()
        assert isinstance(result, str)


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
        assert isinstance(sig, bytes)
        assert len(sig) == 64  # Ed25519 signature size
        assert verify(payload, sig, pk) is True

    def test_wrong_key_fails(self):
        sk, _ = generate_keypair()
        _, other_pk = generate_keypair()
        sig = sign(b"msg", sk)
        assert verify(b"msg", sig, other_pk) is False

    def test_tampered_payload_fails(self):
        sk, pk = generate_keypair()
        sig = sign(b"original", sk)
        assert verify(b"tampered", sig, pk) is False

    def test_malformed_verifier_fails_silently(self):
        sk, _ = generate_keypair()
        sig = sign(b"msg", sk)
        assert verify(b"msg", sig, b"\x00" * 16) is False  # wrong length

    def test_bad_secret_key_raises_value_error(self):
        with pytest.raises(ValueError):
            sign(b"msg", b"\x00" * 16)  # wrong length
