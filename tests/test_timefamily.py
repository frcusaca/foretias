"""MVP functionality tests — TimeFamily orchestrator.

End-to-end stamp, verify, tick, persistence, and family wiring.
"""

from __future__ import annotations

import tempfile

import pytest

from fortias import TimeFamily, Fortis


class TestTimeFamily:
    def test_create_active(self):
        tbf = TimeFamily(serialized=True)
        assert tbf.active is True
        assert tbf.tbid is not None
        assert tbf.tbn.startswith("Time Being ")

    def test_stamp_returns_fortis(self):
        tbf = TimeFamily(serialized=True)
        fortis = tbf.stamp(b"hello")
        assert isinstance(fortis, Fortis)
        assert fortis.tick_number >= 0
        assert len(fortis.my_content_hash) == 32
        assert len(fortis.signature) == 64

    def test_stamp_string(self):
        tbf = TimeFamily(serialized=True)
        fortis = tbf.stamp("hello world")
        assert isinstance(fortis, Fortis)

    def test_verify_valid_fortis(self):
        tbf = TimeFamily(serialized=True)
        fortis = tbf.stamp(b"hello")
        assert tbf.verify(b"hello", fortis) is True

    def test_verify_wrong_content_fails(self):
        tbf = TimeFamily(serialized=True)
        fortis = tbf.stamp(b"hello")
        assert tbf.verify(b"world", fortis) is False

    def test_verify_with_window_check(self):
        tbf = TimeFamily(serialized=True)
        fortis = tbf.stamp(b"hello")
        current = tbf.current_tick()
        result = tbf.verify(b"hello", fortis, next_tick_number=current + 1)
        sig_valid, window_closed = result
        assert sig_valid is True
        assert window_closed is False

    def test_current_tick(self):
        tbf = TimeFamily(serialized=True)
        assert tbf.current_tick() >= 0

    def test_tick_advances(self):
        tbf = TimeFamily(serialized=True)
        tick0 = tbf.current_tick()
        tbf.tick()
        assert tbf.current_tick() > tick0

    def test_serialized_flag(self):
        assert TimeFamily(serialized=True).serialized is True
        assert TimeFamily(serialized=False).serialized is False

    def test_interface_accessors(self):
        tbf = TimeFamily(serialized=True)
        assert tbf.calendar() is not None
        assert tbf.chronomatter() is not None
        assert tbf.inquirer() is not None

    def test_internal_interface_accessors(self):
        tbf = TimeFamily(serialized=True)
        assert tbf._get_calendar() is not None
        assert tbf._get_chronomatter() is not None
        assert tbf._get_inquirer() is not None

    def test_family_references(self):
        tbf = TimeFamily(serialized=True)
        assert tbf.calendar().family is tbf
        assert tbf.chronomatter().family is tbf
        assert tbf.inquirer().family is tbf

    def test_non_serialized_stamp_no_tick(self):
        tbf = TimeFamily(serialized=False, chronon_ns=3_600_000_000_000.0)
        tick_before = tbf.current_tick()
        tbf.stamp(b"msg")
        assert tbf.current_tick() == tick_before

    def test_get_returns_records(self):
        tbf = TimeFamily(serialized=True)
        tbf.stamp(b"msg1")
        tbf.tick()
        tbf.stamp(b"msg2")
        records = tbf.get(0, 2)
        assert len(records) == 2

    def test_tampered_content_fails(self):
        tbf = TimeFamily(serialized=True)
        fortis = tbf.stamp(b"original")
        assert tbf.verify(b"original", fortis) is True
        assert tbf.verify(b"tampered", fortis) is False
