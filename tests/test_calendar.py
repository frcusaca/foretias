"""Unit tests for fortias.calendar — especially the ≤ predecessor contract."""

from __future__ import annotations

import pytest

from fortias.calendar import Calendar
from fortias.timestamp import TimestampFactoryV0


def _ts(year: int, *, month: int = 1, day: int = 1, nanoseconds: int = 0):
    return TimestampFactoryV0.make(
        year=year, month=month, day=day, nanoseconds=nanoseconds
    )


class TestTickMonotonic:
    def test_empty_calendar_returns_none(self):
        cal = Calendar()
        assert cal.get_tick(_ts(2026)) is None

    def test_accepts_strictly_increasing(self):
        cal = Calendar()
        cal.tick(_ts(2024), b"k1" + b"\x00" * 30)
        cal.tick(_ts(2025), b"k2" + b"\x00" * 30)
        cal.tick(_ts(2026), b"k3" + b"\x00" * 30)
        # no exception = ok

    def test_rejects_equal_timestamp(self):
        cal = Calendar()
        cal.tick(_ts(2024), b"k1" + b"\x00" * 30)
        with pytest.raises(ValueError):
            cal.tick(_ts(2024), b"k2" + b"\x00" * 30)

    def test_rejects_earlier_timestamp(self):
        cal = Calendar()
        cal.tick(_ts(2025), b"k1" + b"\x00" * 30)
        with pytest.raises(ValueError):
            cal.tick(_ts(2024), b"k2" + b"\x00" * 30)


class TestGetTickLessOrEqual:
    """The predecessor search must be ≤ (not strictly <)."""

    def test_returns_none_when_query_before_all_ticks(self):
        cal = Calendar()
        cal.tick(_ts(2025), b"k1" + b"\x00" * 30)
        assert cal.get_tick(_ts(2024)) is None

    def test_returns_exact_match_when_query_equals_tick(self):
        """Stamp issued at exactly the rotation moment -> use the NEW key."""
        cal = Calendar()
        cal.tick(_ts(2025), b"oldkey" + b"\x00" * 26)
        cal.tick(_ts(2026), b"newkey" + b"\x00" * 26)
        hit = cal.get_tick(_ts(2026))
        assert hit is not None
        assert hit[0] == _ts(2026)
        assert hit[1] == b"newkey" + b"\x00" * 26

    def test_returns_predecessor_when_query_between_ticks(self):
        cal = Calendar()
        cal.tick(_ts(2024), b"k24" + b"\x00" * 29)
        cal.tick(_ts(2026), b"k26" + b"\x00" * 29)
        hit = cal.get_tick(_ts(2025))
        assert hit is not None
        assert hit[0] == _ts(2024)

    def test_returns_latest_tick_when_query_after_all(self):
        cal = Calendar()
        cal.tick(_ts(2024), b"k24" + b"\x00" * 29)
        cal.tick(_ts(2025), b"k25" + b"\x00" * 29)
        hit = cal.get_tick(_ts(2030))
        assert hit is not None
        assert hit[0] == _ts(2025)


class TestRetrieveForVerification:
    def test_wraps_hit_in_response(self):
        cal = Calendar()
        verifier = b"pubkey" + b"\x00" * 26
        cal.tick(_ts(2025), verifier)

        resp = cal.retrieve_for_verification(_ts(2026), tbid="test-tbid")
        assert resp.verification_calendar is not None
        assert resp.verification_calendar.calendar_timestamp == _ts(2025)
        assert resp.verification_calendar.verifier == verifier
        assert resp.fortias_version == "0.0.1"
        assert resp.TBID == "test-tbid"

    def test_returns_none_verification_calendar_when_empty(self):
        cal = Calendar()
        resp = cal.retrieve_for_verification(_ts(2026), tbid="test-tbid")
        assert resp.verification_calendar is None
        # Envelope fields still populated.
        assert resp.TBID == "test-tbid"
        assert resp.fortias_timestamp
