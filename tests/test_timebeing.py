"""MVP functionality tests — fast, deterministic, per-class coverage.

Classes tested:
  Timebeing          — identity and family reference
  Calendar           — append, get, latest, save/load, integrity
  ChronomatterV1     — creation, stamp, tick, verify, shutdown
  ChronomatterV1Serial — creation, stamp-tick coupling
  Inquirer           — verify valid/invalid, missing calendar
  TimeFamily         — orchestration, stamp, verify, interface accessors
"""

from __future__ import annotations

import tempfile

import pytest

from fortias import TimeFamily, Fortis
from fortias.chronomatter import Inquirer, ChronomatterV1, ChronomatterV1Serial
from fortias.calendar import Calendar
from fortias.timebeing import Timebeing
from fortias.crypto import generate_keypair
from fortias.models import TickRecord


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
    def test_create_active(self):
        cm = ChronomatterV1(chronon_ns=3_600_000_000_000.0)
        assert cm.active is True
        assert cm.tbid is not None
        assert cm.current_tick == 0
        cm.shutdown()

    def test_stamp_returns_fortis(self):
        cm = ChronomatterV1(chronon_ns=3_600_000_000_000.0)
        cal = Calendar(cm.tbid, cm.tbn)
        cm.attach_calendar(cal)
        fortis = cm.stamp(b"hello")
        assert isinstance(fortis, Fortis)
        assert fortis.tick_number == 0
        assert len(fortis.my_content_hash) == 32
        assert len(fortis.signature) == 64
        cm.shutdown()

    def test_stamp_string(self):
        cm = ChronomatterV1(chronon_ns=3_600_000_000_000.0)
        cal = Calendar(cm.tbid, cm.tbn)
        cm.attach_calendar(cal)
        fortis = cm.stamp("hello world")
        assert isinstance(fortis, Fortis)
        cm.shutdown()

    def test_stamp_does_not_trigger_tick(self):
        cm = ChronomatterV1(chronon_ns=3_600_000_000_000.0)
        cal = Calendar(cm.tbid, cm.tbn)
        cm.attach_calendar(cal)
        tick_before = cm.current_tick
        cm.stamp(b"msg")
        assert cm.current_tick == tick_before
        cm.shutdown()

    def test_tick_advances(self):
        cm = ChronomatterV1(chronon_ns=3_600_000_000_000.0)
        cal = Calendar(cm.tbid, cm.tbn)
        cm.attach_calendar(cal)
        cm.tick()
        assert cm.current_tick == 1
        cm.shutdown()

    def test_extends_calendar(self):
        cm = ChronomatterV1(chronon_ns=3_600_000_000_000.0)
        assert isinstance(cm, Calendar)
        assert isinstance(cm, Timebeing)
        cm.shutdown()


class TestChronomatterV1Serial:
    def test_create_active(self):
        cm = ChronomatterV1Serial()
        assert cm.active is True
        assert cm.current_tick == 0
        cm.shutdown()

    def test_stamp_triggers_tick(self):
        cm = ChronomatterV1Serial()
        cal = Calendar(cm.tbid, cm.tbn)
        cm.attach_calendar(cal)
        assert cm.current_tick == 0
        cm.stamp(b"msg")
        assert cm.current_tick == 1
        cm.shutdown()

    def test_rapid_stamps_same_chronon(self):
        cm = ChronomatterV1Serial()
        cal = Calendar(cm.tbid, cm.tbn)
        cm.attach_calendar(cal)
        cm.stamp(b"msg1")
        tick1 = cm.current_tick
        cm.stamp(b"msg2")
        assert cm.current_tick == tick1
        cm.shutdown()

    def test_extends_calendar(self):
        cm = ChronomatterV1Serial()
        assert isinstance(cm, Calendar)
        assert isinstance(cm, Timebeing)
        cm.shutdown()


class TestInquirer:
    def test_verify_valid(self):
        cm = ChronomatterV1Serial()
        cal = Calendar(cm.tbid, cm.tbn)
        cm.attach_calendar(cal)
        fortis = cm.stamp(b"hello")
        iq = Inquirer(calendars=[cal])
        assert iq.verify(b"hello", fortis) is True
        cm.shutdown()

    def test_verify_invalid_content(self):
        cm = ChronomatterV1Serial()
        cal = Calendar(cm.tbid, cm.tbn)
        cm.attach_calendar(cal)
        fortis = cm.stamp(b"hello")
        iq = Inquirer(calendars=[cal])
        assert iq.verify(b"wrong", fortis) is False
        cm.shutdown()

    def test_verify_unknown_tbid(self):
        cm = ChronomatterV1Serial()
        cal = Calendar(cm.tbid, cm.tbn)
        cm.attach_calendar(cal)
        fortis = cm.stamp(b"hello")
        iq = Inquirer(calendars=[Calendar(b"\xff" * 32, "unknown")])
        assert iq.verify(b"hello", fortis) is False
        cm.shutdown()

    def test_extends_timebeing(self):
        iq = Inquirer(calendars=[])
        assert isinstance(iq, Timebeing)


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
