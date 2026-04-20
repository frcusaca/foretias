"""Integration tests for fortias.protocol.Fortias end-to-end flows."""

from __future__ import annotations

import hashlib
from dataclasses import replace

import pytest

from fortias.calendar import Calendar
from fortias.crypto import generate_keypair
from fortias.exceptions import InvalidKeyError
from fortias.models import StampRequest, VerifyRequest
from fortias.protocol import Fortias
from fortias.timestamp import TimestampFactoryV0


def _sha256(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def _stamp_req(payload: bytes, *, echo=None, stamp_request_hash=None) -> StampRequest:
    return StampRequest(
        payload=payload,
        stamp_request_hash=stamp_request_hash or _sha256(payload),
        echo=echo,
    )


def _verify_req(payload, stamp, *, echo=None, stamp_request_hash=None) -> VerifyRequest:
    return VerifyRequest(
        payload=payload,
        stamp=stamp,
        stamp_request_hash=stamp_request_hash or _sha256(payload),
        echo=echo,
    )


def _server_with_published_key(tick_year: int = 2000):
    """Spin up a Fortias whose public key is already on the calendar."""
    sk, pk = generate_keypair()
    calendar = Calendar()
    calendar.tick(TimestampFactoryV0.make(year=tick_year, month=1, day=1), pk)
    return Fortias(secret_key=sk, calendar=calendar), calendar, pk


class TestInit:
    def test_rejects_empty_secret_key(self):
        with pytest.raises(InvalidKeyError):
            Fortias(secret_key=b"", calendar=Calendar())

    def test_rejects_short_secret_key(self):
        with pytest.raises(InvalidKeyError):
            Fortias(secret_key=b"\x00" * 16, calendar=Calendar())

    def test_accepts_32_byte_secret(self):
        sk, _ = generate_keypair()
        Fortias(secret_key=sk, calendar=Calendar())


class TestStampThenVerifyHappyPath:
    def test_valid_stamp_verifies(self):
        server, _cal, _pk = _server_with_published_key()
        payload = b"proof of this message at 2026-04-19"

        stamp_resp = server.stamp(_stamp_req(payload))
        verify_resp = server.verify(_verify_req(payload, stamp_resp))

        assert stamp_resp.status == "normal"
        assert verify_resp.status == "normal"
        assert verify_resp.valid is True
        names = [r.name for r in verify_resp.results]
        assert names == [
            "version_check", "payload_integrity",
            "calendar_lookup", "signature",
        ]
        assert all(r.passed for r in verify_resp.results)

    def test_stamp_response_fields_populated(self):
        server, _cal, _pk = _server_with_published_key()
        resp = server.stamp(_stamp_req(b"hi"))
        assert resp.fortias_version == "0.0.1"
        assert resp.TBID
        assert resp.fortias_timestamp
        assert resp.stamp_request_hash == _sha256(b"hi")
        assert len(bytes.fromhex(resp.signature)) == 64
        assert resp.status == "normal"
        assert resp.echo is None

    def test_verify_response_carries_stamp_request_hash(self):
        server, _cal, _pk = _server_with_published_key()
        payload = b"x"
        stamp_resp = server.stamp(_stamp_req(payload))
        verify_resp = server.verify(_verify_req(payload, stamp_resp))
        assert verify_resp.stamp_request_hash == _sha256(payload)


class TestEchoRoundtrip:
    def test_stamp_echoes_value(self):
        server, _cal, _pk = _server_with_published_key()
        resp = server.stamp(_stamp_req(b"x", echo={"client": "req-42"}))
        assert resp.echo == {"client": "req-42"}

    def test_verify_echoes_value(self):
        server, _cal, _pk = _server_with_published_key()
        stamp_resp = server.stamp(_stamp_req(b"x"))
        verify_resp = server.verify(
            _verify_req(b"x", stamp_resp, echo="correlation-token")
        )
        assert verify_resp.echo == "correlation-token"

    def test_stamp_and_verify_echoes_are_independent(self):
        """Verify's echo is the verify request's echo, not the stamp's."""
        server, _cal, _pk = _server_with_published_key()
        stamp_resp = server.stamp(_stamp_req(b"x", echo="from-stamp"))
        verify_resp = server.verify(
            _verify_req(b"x", stamp_resp, echo="from-verify")
        )
        assert stamp_resp.echo == "from-stamp"
        assert verify_resp.echo == "from-verify"


class TestRequestHashPreCheck:
    def test_stamp_abnormal_on_hash_mismatch(self):
        server, _cal, _pk = _server_with_published_key()
        bad_req = _stamp_req(b"x", stamp_request_hash="00" * 32)  # wrong
        resp = server.stamp(bad_req)

        assert resp.status.startswith("abnormal:")
        assert "stamp_request_hash does not match payload" in resp.status
        assert resp.signature == ""
        # The claimed hash is echoed back so the caller can correlate.
        assert resp.stamp_request_hash == "00" * 32

    def test_stamp_abnormal_preserves_echo(self):
        server, _cal, _pk = _server_with_published_key()
        bad = _stamp_req(b"x", stamp_request_hash="00" * 32, echo="trace-1")
        resp = server.stamp(bad)
        assert resp.status.startswith("abnormal:")
        assert resp.echo == "trace-1"

    def test_verify_abnormal_on_hash_mismatch(self):
        server, _cal, _pk = _server_with_published_key()
        good_stamp = server.stamp(_stamp_req(b"x"))
        bad = _verify_req(b"x", good_stamp, stamp_request_hash="00" * 32)
        resp = server.verify(bad)

        assert resp.status.startswith("abnormal:")
        assert resp.valid is False
        assert resp.results == ()
        assert resp.stamp_request_hash == "00" * 32


class TestVerifyFailureModesStillNormal:
    """Check failures should yield valid=False with status='normal'."""

    def test_tampered_payload(self):
        server, _cal, _pk = _server_with_published_key()
        good = server.stamp(_stamp_req(b"original"))
        # NOTE: stamp_request_hash must match the *new* payload for the
        # pre-check to pass; otherwise we'd trip the abnormal path.
        verify_resp = server.verify(_verify_req(b"tampered", good))

        assert verify_resp.status == "normal"
        assert verify_resp.valid is False
        by_name = {r.name: r for r in verify_resp.results}
        assert by_name["version_check"].passed is True
        assert by_name["payload_integrity"].passed is False
        assert by_name["calendar_lookup"].passed is True
        assert by_name["signature"].passed is False

    def test_version_mismatch(self):
        server, _cal, _pk = _server_with_published_key()
        good = server.stamp(_stamp_req(b"x"))
        bad_stamp = replace(good, fortias_version="9.9.9")
        verify_resp = server.verify(_verify_req(b"x", bad_stamp))

        assert verify_resp.status == "normal"
        by_name = {r.name: r for r in verify_resp.results}
        assert by_name["version_check"].passed is False
        assert verify_resp.valid is False

    def test_wrong_calendar_pubkey(self):
        sk, _ = generate_keypair()
        _, wrong_pk = generate_keypair()
        calendar = Calendar()
        calendar.tick(
            TimestampFactoryV0.make(year=2000, month=1, day=1), wrong_pk
        )
        server = Fortias(secret_key=sk, calendar=calendar)

        good = server.stamp(_stamp_req(b"x"))
        verify_resp = server.verify(_verify_req(b"x", good))

        assert verify_resp.status == "normal"
        by_name = {r.name: r for r in verify_resp.results}
        assert by_name["calendar_lookup"].passed is True
        assert by_name["signature"].passed is False
        assert verify_resp.valid is False

    def test_empty_calendar(self):
        sk, _ = generate_keypair()
        server = Fortias(secret_key=sk, calendar=Calendar())

        good = server.stamp(_stamp_req(b"x"))
        verify_resp = server.verify(_verify_req(b"x", good))

        assert verify_resp.status == "normal"
        by_name = {r.name: r for r in verify_resp.results}
        assert by_name["calendar_lookup"].passed is False
        assert by_name["signature"].passed is False
        assert verify_resp.valid is False

    def test_calendar_tick_after_stamp(self):
        """Pubkey published AFTER the stamp was issued — lookup should miss."""
        sk, pk = generate_keypair()
        server = Fortias(secret_key=sk, calendar=Calendar())

        good = server.stamp(_stamp_req(b"x"))
        # 2500 is within the TimestampV0 uint64 ns range (which ends ~2554)
        # but reliably later than any "now" at test time.
        server._calendar.tick(  # noqa: SLF001 — deliberate for this test
            TimestampFactoryV0.make(year=2500, month=1, day=1), pk
        )
        verify_resp = server.verify(_verify_req(b"x", good))

        assert verify_resp.status == "normal"
        by_name = {r.name: r for r in verify_resp.results}
        assert by_name["calendar_lookup"].passed is False
        assert verify_resp.valid is False


class TestVerifyAbnormalBeyondHashMismatch:
    def test_unparseable_stamp_timestamp_is_abnormal(self):
        server, _cal, _pk = _server_with_published_key()
        good = server.stamp(_stamp_req(b"x"))
        bad_stamp = replace(good, fortias_timestamp="not-a-timestamp")

        resp = server.verify(_verify_req(b"x", bad_stamp))
        assert resp.status.startswith("abnormal:")
        assert resp.valid is False
        assert resp.results == ()


class TestCrossSignerVerification:
    """A Fortias verifies a stamp produced by a *different* private key so long
    as the other signer's pubkey is in the calendar at-or-before issued_at.
    """

    def test_verifier_with_different_key_can_verify(self):
        sk_a, pk_a = generate_keypair()
        calendar = Calendar()
        calendar.tick(TimestampFactoryV0.make(year=2000, month=1, day=1), pk_a)

        signer = Fortias(secret_key=sk_a, calendar=calendar)
        stamp_resp = signer.stamp(_stamp_req(b"shared payload"))

        sk_b, _ = generate_keypair()
        verifier_server = Fortias(secret_key=sk_b, calendar=calendar)
        verify_resp = verifier_server.verify(_verify_req(b"shared payload", stamp_resp))

        assert verify_resp.status == "normal"
        assert verify_resp.valid is True
