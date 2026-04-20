"""Unit tests for fortias.models."""

from __future__ import annotations

import hashlib
from dataclasses import FrozenInstanceError

import pytest

from fortias.models import (
    StampRequest,
    StampResponse,
    VerificationCalendar,
    VerificationCalendarResponse,
    VerifyRequest,
    VerifyResponse,
    VerifyResult,
)
from fortias.timestamp import TimestampFactoryV0


def _hex(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def _stamp_response(**overrides) -> StampResponse:
    base = dict(
        TBID="tbid-1",
        fortias_version="0.0.1",
        fortias_timestamp="2026-04-19T14:30:00.000000Z",
        stamp_request_hash="deadbeef",
        signature="ff" * 64,
    )
    base.update(overrides)
    return StampResponse(**base)


class TestFrozenness:
    def test_stamp_request_is_frozen(self):
        req = StampRequest(payload=b"x", stamp_request_hash=_hex(b"x"))
        with pytest.raises(FrozenInstanceError):
            req.payload = b"y"  # type: ignore[misc]

    def test_stamp_response_is_frozen(self):
        resp = _stamp_response()
        with pytest.raises(FrozenInstanceError):
            resp.signature = "00"  # type: ignore[misc]

    def test_verify_response_is_frozen(self):
        resp = VerifyResponse.from_results(
            [],
            TBID="t",
            fortias_version="0.0.1",
            fortias_timestamp="2026-04-19T14:30:00.000000Z",
            stamp_request_hash="deadbeef",
        )
        with pytest.raises(FrozenInstanceError):
            resp.valid = False  # type: ignore[misc]


class TestDefaults:
    def test_stamp_request_echo_default_none(self):
        req = StampRequest(payload=b"x", stamp_request_hash=_hex(b"x"))
        assert req.echo is None

    def test_stamp_response_status_default_normal(self):
        resp = _stamp_response()
        assert resp.status == "normal"
        assert resp.echo is None

    def test_verify_response_status_default_normal(self):
        resp = VerifyResponse.from_results(
            [], TBID="t", fortias_version="0.0.1",
            fortias_timestamp="2026-04-19T14:30:00.000000Z",
            stamp_request_hash="deadbeef",
        )
        assert resp.status == "normal"
        assert resp.echo is None


class TestVerifyResponseFromResults:
    def test_all_pass_valid_true(self):
        r = VerifyResponse.from_results(
            [VerifyResult("a", True), VerifyResult("b", True)],
            TBID="t", fortias_version="0.0.1",
            fortias_timestamp="2026-04-19T14:30:00.000000Z",
            stamp_request_hash="deadbeef",
        )
        assert r.valid is True
        assert r.results == (VerifyResult("a", True), VerifyResult("b", True))

    def test_any_fail_valid_false(self):
        r = VerifyResponse.from_results(
            [VerifyResult("a", True), VerifyResult("b", False)],
            TBID="t", fortias_version="0.0.1",
            fortias_timestamp="2026-04-19T14:30:00.000000Z",
            stamp_request_hash="deadbeef",
        )
        assert r.valid is False

    def test_empty_results_valid_true(self):
        r = VerifyResponse.from_results(
            [],
            TBID="t", fortias_version="0.0.1",
            fortias_timestamp="2026-04-19T14:30:00.000000Z",
            stamp_request_hash="deadbeef",
        )
        assert r.valid is True
        assert r.results == ()

    def test_echo_and_status_passthrough(self):
        r = VerifyResponse.from_results(
            [],
            TBID="t", fortias_version="0.0.1",
            fortias_timestamp="2026-04-19T14:30:00.000000Z",
            stamp_request_hash="deadbeef",
            echo={"client": "req-7"},
            status="abnormal: test",
        )
        assert r.echo == {"client": "req-7"}
        assert r.status == "abnormal: test"


class TestVerifyRequest:
    def test_carries_payload_and_stamp(self):
        stamp = _stamp_response()
        req = VerifyRequest(
            payload=b"data",
            stamp=stamp,
            stamp_request_hash=_hex(b"data"),
        )
        assert req.payload == b"data"
        assert req.stamp is stamp
        assert req.echo is None


class TestVerificationCalendar:
    def test_fields(self):
        ts = TimestampFactoryV0.make(year=2026, month=1, day=1)
        vcal = VerificationCalendar(calendar_timestamp=ts, verifier=b"\x01" * 32)
        assert vcal.calendar_timestamp == ts
        assert vcal.verifier == b"\x01" * 32

    def test_response_wraps_optional_calendar(self):
        ok = VerificationCalendarResponse(
            TBID="t",
            fortias_version="0.0.1",
            fortias_timestamp="2026-04-19T14:30:00.000000Z",
            verification_calendar=VerificationCalendar(
                calendar_timestamp=TimestampFactoryV0.make(year=2026, month=1, day=1),
                verifier=b"\x02" * 32,
            ),
        )
        assert ok.verification_calendar is not None

        miss = VerificationCalendarResponse(
            TBID="t",
            fortias_version="0.0.1",
            fortias_timestamp="2026-04-19T14:30:00.000000Z",
            verification_calendar=None,
        )
        assert miss.verification_calendar is None
