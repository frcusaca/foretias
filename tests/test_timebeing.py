"""MVP functionality tests — fast, deterministic, per-class coverage.

Component-level tests for Timebeing, Calendar, ChronomatterV1,
ChronomatterV1Serial, and Inquirer in isolation.
"""

from __future__ import annotations

import tempfile
from unittest.mock import MagicMock

import pytest

from foretias import Foretis
from foretias.chronomatter import Inquirer, ChronomatterV1, ChronomatterV1Serial
from foretias.calendar import Calendar
from foretias.timebeing import Timebeing
from foretias.crypto import generate_keypair
from foretias.models import TickRecord


class TestTimebeing:
    def test_auto_generates_tbid(self):
        tb = Timebeing()
        assert tb.tbid is not None
        assert len(tb.tbid) == 16

    def test_accepts_explicit_tbid(self):
        tbid = b"\xab" * 32
        tb = Timebeing(tbid=tbid)
        assert tb.tbid == tbid

    def test_tbn_from_tbid(self):
        tbid = b"\xab" * 32
        tb = Timebeing(tbid=tbid)
        assert tb.tbn == f"Time Being {tbid.hex()}"

    def test_family_default_none(self):
        tb = Timebeing()
        assert tb.family is None

    def test_family_settable(self):
        tb = Timebeing()
        family = object()
        tb.family = family
        assert tb.family is family


class TestCalendarMVP:
    def test_append_to_empty(self):
        cal = Calendar(b"\x00" * 32, "test")
        _, pk = generate_keypair()
        cal.append(TickRecord(0, pk, b"sig", b"sig"))
        assert len(cal.ticks) == 1

    def test_rejects_equal_tick_number(self):
        cal = Calendar(b"\x00" * 32, "test")
        _, pk = generate_keypair()
        cal.append(TickRecord(100, pk, b"sig", b"sig"))
        with pytest.raises(ValueError, match="strictly increasing"):
            cal.append(TickRecord(100, pk, b"sig", b"sig"))

    def test_rejects_earlier_tick_number(self):
        cal = Calendar(b"\x00" * 32, "test")
        _, pk = generate_keypair()
        cal.append(TickRecord(200, pk, b"sig", b"sig"))
        with pytest.raises(ValueError, match="strictly increasing"):
            cal.append(TickRecord(100, pk, b"sig", b"sig"))

    def test_latest_empty_returns_none(self):
        cal = Calendar(b"\x00" * 32, "test")
        assert cal.latest() is None

    def test_latest_returns_highest(self):
        cal = Calendar(b"\x00" * 32, "test")
        _, pk = generate_keypair()
        cal.append(TickRecord(100, pk, b"sig", b"sig"))
        cal.append(TickRecord(200, pk, b"sig", b"sig"))
        assert cal.latest() == 200

    def test_get_range(self):
        cal = Calendar(b"\x00" * 32, "test")
        _, pk = generate_keypair()
        cal.append(TickRecord(100, pk, b"sig", b"sig"))
        cal.append(TickRecord(200, pk, b"sig", b"sig"))
        cal.append(TickRecord(300, pk, b"sig", b"sig"))
        result = cal.get(200, 2)
        assert len(result) == 2
        assert result[0].tick_number == 200

    def test_save_load_roundtrip(self):
        cal = Calendar(b"\x00" * 32, "test")
        _, pk = generate_keypair()
        cal.append(TickRecord(0, pk, b"sig", b"sig"))
        with tempfile.NamedTemporaryFile(suffix=".json") as f:
            cal.save(f.name)
            loaded = Calendar.load(f.name)
        assert loaded.tbid == cal.tbid
        assert loaded.tbn == cal.tbn
        assert len(loaded.ticks) == 1

    def test_integrity_check_empty(self):
        cal = Calendar(b"\x00" * 32, "test")
        assert cal.integrity_check() is True

    def test_integrity_check_single(self):
        cal = Calendar(b"\x00" * 32, "test")
        _, pk = generate_keypair()
        cal.append(TickRecord(0, pk, b"sig", b"sig"))
        assert cal.integrity_check() is True

    def test_extends_timebeing(self):
        cal = Calendar(b"\x00" * 32, "test")
        assert isinstance(cal, Timebeing)


class TestChronomatterV1:
    def _make_cm(self):
        cm = ChronomatterV1(chronon_ns=3_600_000_000_000.0)
        cal = Calendar(cm.tbid, cm.tbn)
        cm._family = MagicMock()
        cm._family._get_calendar.return_value = cal
        return cm, cal

    def test_create_active(self):
        cm, _ = self._make_cm()
        assert cm.active is True
        assert cm.tbid is not None
        assert cm.current_tick == 0
        cm.shutdown()

    def test_stamp_returns_foretis(self):
        cm, _ = self._make_cm()
        foretis = cm.stamp(b"hello")
        assert isinstance(foretis, Foretis)
        assert foretis.tick_number == 0
        assert len(foretis.my_content_hash) == 32
        assert len(foretis.signature) == 64
        cm.shutdown()

    def test_stamp_string(self):
        cm, _ = self._make_cm()
        foretis = cm.stamp("hello world")
        assert isinstance(foretis, Foretis)
        cm.shutdown()

    def test_stamp_does_not_trigger_tick(self):
        cm, _ = self._make_cm()
        tick_before = cm.current_tick
        cm.stamp(b"msg")
        assert cm.current_tick == tick_before
        cm.shutdown()

    def test_tick_advances(self):
        cm, _ = self._make_cm()
        cm.tick()
        assert cm.current_tick == 1
        cm.shutdown()

    def test_extends_calendar(self):
        cm, _ = self._make_cm()
        assert isinstance(cm, Calendar)
        assert isinstance(cm, Timebeing)
        cm.shutdown()


class TestChronomatterV1Serial:
    def _make_cm(self):
        cm = ChronomatterV1Serial()
        cal = Calendar(cm.tbid, cm.tbn)
        cm._family = MagicMock()
        cm._family._get_calendar.return_value = cal
        return cm, cal

    def test_create_active(self):
        cm, _ = self._make_cm()
        assert cm.active is True
        assert cm.current_tick == 0
        cm.shutdown()

    def test_stamp_triggers_tick(self):
        cm, _ = self._make_cm()
        assert cm.current_tick == 0
        cm.stamp(b"msg")
        assert cm.current_tick == 1
        cm.shutdown()

    def test_rapid_stamps_same_chronon(self):
        cm, _ = self._make_cm()
        cm.stamp(b"msg1")
        tick1 = cm.current_tick
        cm.stamp(b"msg2")
        assert cm.current_tick == tick1
        cm.shutdown()

    def test_extends_calendar(self):
        cm, _ = self._make_cm()
        assert isinstance(cm, Calendar)
        assert isinstance(cm, Timebeing)
        cm.shutdown()


class TestInquirer:
    def test_verify_valid_through_family(self):
        from foretias import TimeFamily
        tbf = TimeFamily(serialized=True)
        foretis = tbf.stamp(b"hello")
        assert tbf.inquirer().verify(b"hello", foretis) is True

    def test_verify_invalid_content(self):
        from foretias import TimeFamily
        tbf = TimeFamily(serialized=True)
        foretis = tbf.stamp(b"hello")
        assert tbf.inquirer().verify(b"wrong", foretis) is False

    def test_verify_without_family_fails(self):
        iq = Inquirer()
        assert iq.verify(b"hello", Foretis(0, b"\x00" * 32, b"\x00" * 64, b"\x00" * 32, "", "")) is False

    def test_extends_timebeing(self):
        iq = Inquirer()
        assert isinstance(iq, Timebeing)
