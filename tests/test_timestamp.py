"""Unit tests for fortias.timestamp."""

from __future__ import annotations

from ctypes import c_uint64
from datetime import datetime, timezone

import pytest

from fortias.timestamp import TimestampFactoryV0, TimestampV0


class TestTimestampV0:
    def test_constructs_from_cuint64(self):
        ts = TimestampV0(c_uint64(12345))
        assert ts.value == 12345

    def test_accepts_zero_and_max(self):
        assert TimestampV0(c_uint64(0)).value == 0
        assert TimestampV0(c_uint64((1 << 64) - 1)).value == (1 << 64) - 1

    def test_equality_and_hash(self):
        a = TimestampV0(c_uint64(42))
        b = TimestampV0(c_uint64(42))
        c = TimestampV0(c_uint64(43))
        assert a == b
        assert a != c
        assert hash(a) == hash(b)
        assert hash(a) != hash(c)

    def test_ordering(self):
        a = TimestampV0(c_uint64(1))
        b = TimestampV0(c_uint64(2))
        assert a < b
        assert b > a
        assert a <= b
        assert b >= a
        assert a != b

    def test_bytes_roundtrip(self):
        ts = TimestampV0(c_uint64(1_700_000_000_000_000_000))
        data = ts.to_bytes()
        assert len(data) == 8
        assert TimestampV0.from_bytes(data) == ts

    def test_from_bytes_rejects_wrong_length(self):
        with pytest.raises(ValueError):
            TimestampV0.from_bytes(b"\x00" * 4)

    def test_to_iso_string_format(self):
        # 2026-04-19T14:30:00.000000001Z
        ns = int(
            datetime(2026, 4, 19, 14, 30, 0, tzinfo=timezone.utc).timestamp()
            * 1_000_000_000
        ) + 1
        ts = TimestampV0(c_uint64(ns))
        assert ts.to_iso_string() == "2026-04-19T14:30:00.000000001Z"

    def test_repr(self):
        ts = TimestampV0(c_uint64(0))
        assert "TimestampV0" in repr(ts)
        assert "1970-01-01T00:00:00.000000000Z" in repr(ts)


class TestTimestampFactoryV0:
    def test_specification(self):
        spec = TimestampFactoryV0.get_specification()
        assert "nanosecond-precision ISO 8601" in spec

    def test_make_basic(self):
        ts = TimestampFactoryV0.make(year=2026, month=4, day=19)
        assert ts.to_iso_string() == "2026-04-19T00:00:00.000000000Z"

    def test_make_with_nanoseconds(self):
        ts = TimestampFactoryV0.make(
            year=2026, month=4, day=19,
            hour=14, minute=30, second=0,
            microseconds=500_000, nanoseconds=123,
        )
        assert ts.to_iso_string() == "2026-04-19T14:30:00.500000123Z"

    def test_make_rejects_bad_nanoseconds(self):
        with pytest.raises(ValueError):
            TimestampFactoryV0.make(year=2026, month=4, day=19, nanoseconds=1000)

    def test_from_datetime_naive_treated_as_utc(self):
        dt = datetime(2026, 4, 19, 14, 30, 0)
        assert TimestampFactoryV0.from_datetime(dt).to_iso_string() == \
            "2026-04-19T14:30:00.000000000Z"

    def test_from_datetime_aware(self):
        dt = datetime(2026, 4, 19, 14, 30, 0, tzinfo=timezone.utc)
        assert TimestampFactoryV0.from_datetime(dt, nanoseconds=7).to_iso_string() == \
            "2026-04-19T14:30:00.000000007Z"

    @pytest.mark.parametrize("s,expected", [
        ("2026-04-19T14:30:00Z", "2026-04-19T14:30:00.000000000Z"),
        ("2026-04-19T14:30:00.1Z", "2026-04-19T14:30:00.100000000Z"),
        ("2026-04-19T14:30:00.123456789Z", "2026-04-19T14:30:00.123456789Z"),
        ("2026-04-19 14:30:00.000000001Z", "2026-04-19T14:30:00.000000001Z"),
    ])
    def test_from_iso_string_roundtrip(self, s, expected):
        assert TimestampFactoryV0.from_iso_string(s).to_iso_string() == expected

    def test_from_iso_string_with_offset(self):
        # 14:30 +05:30 == 09:00 UTC
        ts = TimestampFactoryV0.from_iso_string("2026-04-19T14:30:00+05:30")
        assert ts.to_iso_string() == "2026-04-19T09:00:00.000000000Z"

    def test_from_iso_string_rejects_garbage(self):
        with pytest.raises(ValueError):
            TimestampFactoryV0.from_iso_string("not a timestamp")
