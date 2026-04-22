"""Aggressive / defensive tests — slow, destructive, edge-case coverage.

Tests here are intentionally slower (sleeps, 100-iteration loops, file I/O)
or destructive (tampered data, concurrent writes, thread lifecycle).
Classes tested:
  Calendar           — tampered load, bad hex, non-ascending, negative ticks
  ChronomatterV1     — 100-tick loop, concurrent stamps, dormant load
  ChronomatterV1Serial — chronon boundary (sleep), double shutdown
  Inquirer           — verify with both ticks, verify with single tick
  TimeFamily         — full persistence lifecycle, dormant verify, stress
"""

from __future__ import annotations

import json
import pathlib
import tempfile
import threading
import time

import pytest

from fortias import TimeFamily, Fortis
from fortias.chronomatter import Inquirer, ChronomatterV1, ChronomatterV1Serial
from fortias.calendar import Calendar
from fortias.crypto import generate_keypair
from fortias.models import TickRecord


class TestCalendarDefensive:
    def test_load_invalid_hex_raises(self):
        with tempfile.NamedTemporaryFile(suffix=".json") as f:
            f.write(json.dumps({
                "tbid": "not_valid_hex!!",
                "tbn": "test",
                "ticks": [{"tick_number": 0, "public_key": "aa", "forward_fortis": "bb", "backward_fortis": "cc"}],
            }).encode())
            f.flush()
            with pytest.raises(ValueError, match="valid hex"):
                Calendar.load(f.name)

    def test_load_non_ascending_raises(self):
        with tempfile.NamedTemporaryFile(suffix=".json") as f:
            f.write(json.dumps({
                "tbid": "61626364",
                "tbn": "test",
                "ticks": [
                    {"tick_number": 200, "public_key": "aa" * 32, "forward_fortis": "bb" * 64, "backward_fortis": "cc" * 64},
                    {"tick_number": 100, "public_key": "cc" * 32, "forward_fortis": "dd" * 64, "backward_fortis": "ee" * 64},
                ],
            }).encode())
            f.flush()
            with pytest.raises(ValueError, match="strictly ascending"):
                Calendar.load(f.name)

    def test_load_negative_tick_raises(self):
        with tempfile.NamedTemporaryFile(suffix=".json") as f:
            f.write(json.dumps({
                "tbid": "61626364",
                "tbn": "test",
                "ticks": [{"tick_number": -1, "public_key": "aa" * 32, "forward_fortis": "bb" * 64, "backward_fortis": "cc" * 64}],
            }).encode())
            f.flush()
            with pytest.raises(ValueError, match="non-negative"):
                Calendar.load(f.name)

    def test_load_tampered_chain_raises(self):
        from fortias._timebeing import _genesis_ma, _timebeing
        tbid = b"test-tbid-123456789012345678901234"
        cal = Calendar(tbid, "test")
        sk0, pk0 = generate_keypair()
        gm = _genesis_ma(tbid, pk0)
        cal.append(TickRecord(0, pk0, b"sig", b"sig"))
        r1, sk1 = _timebeing._tick(tbid, cal.ticks[-1], sk0)
        cal.append(r1)
        r2, _ = _timebeing._tick(tbid, r1, sk1)
        cal.append(r2)

        with tempfile.NamedTemporaryFile(suffix=".json") as f:
            cal.save(f.name)
            data = json.loads(f.read())
            data["ticks"][1]["backward_fortis"] = "ff" * 64
            f.seek(0)
            f.write(json.dumps(data).encode())
            f.truncate()
            f.flush()
            with pytest.raises(ValueError, match="integrity"):
                Calendar.load(f.name)

    def test_integrity_broken_chain_reports_failure(self):
        cal = Calendar(b"\x00" * 32, "test")
        _, pk0 = generate_keypair()
        _, pk1 = generate_keypair()
        _, pk_bad = generate_keypair()
        cal.append(TickRecord(0, pk0, b"sig", b"sig"))
        cal.append(TickRecord(100, pk1, b"sig", b"sig"))
        cal.append(TickRecord(200, pk_bad, b"bad_sig", b"bad_sig"))
        ok, failures = cal.integrity_check(return_failures=True)
        assert ok is False
        assert len(failures) > 0

    def test_family_on_calendar(self):
        cal = Calendar(b"\x00" * 32, "test")
        assert cal.family is None
        cal.family = object()
        assert cal.family is not None


class TestChronomatterV1Aggressive:
    def test_many_ticks_and_stamps(self):
        cm = ChronomatterV1(chronon_ns=3_600_000_000_000.0)
        cal = Calendar(cm.tbid, cm.tbn)
        cm.attach_calendar(cal)
        iq = Inquirer(calendars=[cal])

        stamps = []
        for i in range(100):
            f = cm.stamp(f"message {i}")
            stamps.append((f"message {i}", f))
            cm.tick()

        for content, fortis in stamps:
            assert iq.verify(content, fortis) is True

        assert cal.integrity_check() is True
        cm.shutdown()

    def test_concurrent_stamps(self):
        cm = ChronomatterV1(chronon_ns=3_600_000_000_000.0)
        cal = Calendar(cm.tbid, cm.tbn)
        cm.attach_calendar(cal)
        iq = Inquirer(calendars=[cal])

        results = []
        errors = []

        def stamp_and_append():
            try:
                f = cm.stamp(b"concurrent message")
                results.append(f)
            except Exception as e:
                errors.append(e)

        threads = [threading.Thread(target=stamp_and_append) for _ in range(10)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()

        assert len(errors) == 0
        assert len(results) == 10
        for f in results:
            assert iq.verify(b"concurrent message", f) is True
        cm.shutdown()

    def test_dormant_cannot_stamp(self):
        cm = ChronomatterV1(chronon_ns=3_600_000_000_000.0)
        cal = Calendar(cm.tbid, cm.tbn)
        cm.attach_calendar(cal)
        cm._active = False
        with pytest.raises(RuntimeError, match="dormant"):
            cm.stamp(b"hello")
        cm.shutdown()


class TestChronomatterV1SerialAggressive:
    def test_chronon_boundary(self):
        cm = ChronomatterV1Serial(chronon_ns=1_000_000_000.0)
        cal = Calendar(cm.tbid, cm.tbn)
        cm.attach_calendar(cal)
        cm.stamp(b"msg1")
        tick1 = cm.current_tick
        time.sleep(1.5)  # wait for daemon to fire scheduled tick
        cm.stamp(b"msg2")
        tick2 = cm.current_tick
        assert tick2 > tick1
        cm.stamp(b"msg3")
        assert cm.current_tick == tick2
        cm.shutdown()

    def test_shutdown_twice_safe(self):
        cm = ChronomatterV1Serial()
        cm.shutdown()
        cm.shutdown()  # must not raise

    def test_shutdown_stops_daemon(self):
        cm = ChronomatterV1Serial(chronon_ns=1_000_000_000.0)
        cm.stamp(b"msg")
        assert cm._daemon is not None
        cm.shutdown()
        assert not cm._daemon.is_alive()

    def test_many_ticks_serial(self):
        cm = ChronomatterV1Serial()
        cal = Calendar(cm.tbid, cm.tbn)
        cm.attach_calendar(cal)
        iq = Inquirer(calendars=[cal])

        stamps = []
        for i in range(50):
            f = cm.stamp(f"msg {i}")
            stamps.append((f"msg {i}", f))
            cm.tick()

        for content, fortis in stamps:
            assert iq.verify(content, fortis) is True
        cm.shutdown()


class TestInquirerAggressive:
    def test_verify_with_chain_integrity(self):
        cm = ChronomatterV1Serial()
        cal = Calendar(cm.tbid, cm.tbn)
        cm.attach_calendar(cal)
        cm.stamp(b"msg1")
        cm.tick()
        fortis = cm.stamp(b"msg2")
        iq = Inquirer(calendars=[cal])
        assert iq.verify(b"msg2", fortis) is True
        cm.shutdown()

    def test_lookup_surrounding_ticks_boundary(self):
        cm = ChronomatterV1Serial()
        cal = Calendar(cm.tbid, cm.tbn)
        cm.attach_calendar(cal)
        cm.stamp(b"msg")
        cm.tick()
        iq = Inquirer(calendars=[cal])

        # Index out of range
        prev, curr = iq._lookup_surrounding_ticks(999, cal)
        assert prev is None
        assert curr is None

        # Negative index
        prev, curr = iq._lookup_surrounding_ticks(-1, cal)
        assert prev is None
        assert curr is None

        # First tick
        prev, curr = iq._lookup_surrounding_ticks(0, cal)
        assert prev is None
        assert curr is not None

        cm.shutdown()


class TestTimeFamilyPersistence:
    def test_create_stamp_save_load_verify(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tbf = TimeFamily(serialized=True, persist_path=tmpdir)
            fortis = tbf.stamp(b"hello world")
            assert tbf.verify(b"hello world", fortis) is True
            tbf.save()
            tbf.shutdown()

            tbf2 = TimeFamily.load(persist_path=tmpdir)
            assert tbf2.active is False
            assert tbf2.verify(b"hello world", fortis) is True

    def test_dormant_cannot_stamp(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tbf = TimeFamily(serialized=True, persist_path=tmpdir)
            tbf.stamp(b"hello")
            tbf.save()
            tbf.shutdown()

            tbf2 = TimeFamily.load(persist_path=tmpdir)
            with pytest.raises(RuntimeError, match="dormant"):
                tbf2.stamp(b"world")

    def test_chain_integrity_after_reload(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tbf = TimeFamily(serialized=True, persist_path=tmpdir)
            tbf.stamp(b"msg1")
            tbf.tick()
            tbf.stamp(b"msg2")
            tbf.save()
            tbf.shutdown()

            tbf2 = TimeFamily.load(persist_path=tmpdir)
            assert tbf2.calendar().integrity_check() is True


class TestTimeFamilyStress:
    def test_many_ticks_and_stamps(self):
        tbf = TimeFamily(serialized=True)
        stamps = []
        for i in range(100):
            f = tbf.stamp(f"message {i}")
            stamps.append((f"message {i}", f))
            tbf.tick()

        for content, fortis in stamps:
            assert tbf.verify(content, fortis) is True
        assert tbf.calendar.integrity_check() is True

    def test_save_load_reverify(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tbf = TimeFamily(serialized=True, persist_path=tmpdir)
            stamps = []
            for i in range(50):
                f = tbf.stamp(f"pre-reload msg {i}")
                stamps.append((f"pre-reload msg {i}", f))
                tbf.tick()
            tbf.save()
            tbf.shutdown()

            tbf2 = TimeFamily.load(persist_path=tmpdir)
            for content, fortis in stamps:
                assert tbf2.verify(content, fortis) is True


class TestTimeFamilyShutdown:
    def test_shutdown_active_thread(self):
        tbf = TimeFamily(serialized=False, chronon_ns=1_000_000_000.0)
        assert tbf._stamp._daemon is not None
        tbf.shutdown()
        assert not tbf._stamp._daemon.is_alive()

    def test_shutdown_stops_serialized_daemon(self):
        tbf = TimeFamily(serialized=True, chronon_ns=1_000_000_000.0)
        tbf.stamp(b"msg")
        tbf.shutdown()
        assert not tbf._stamp._daemon.is_alive()

    def test_shutdown_dormant_is_noop(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tbf = TimeFamily(serialized=True, persist_path=tmpdir)
            tbf.stamp(b"hello")
            tbf.save()
            tbf.shutdown()
            tbf2 = TimeFamily.load(persist_path=tmpdir)
            assert tbf2._stamp._daemon is None
            tbf2.shutdown()  # must not raise

    def test_shutdown_twice_is_safe(self):
        tbf = TimeFamily(serialized=False, chronon_ns=1_000_000_000.0)
        tbf.shutdown()
        tbf.shutdown()

    def test_shutdown_saves_calendar(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tbf = TimeFamily(serialized=False, chronon_ns=1_000_000_000.0, persist_path=tmpdir)
            tbf.stamp(b"hello")
            tbf.tick()
            tbf.shutdown()
            cal_path = f"{tmpdir}/{tbf.tbid.hex()}/calendar.json"
            assert pathlib.Path(cal_path).exists()

    def test_concurrent_stamps(self):
        tbf = TimeFamily(serialized=True)
        results = []
        errors = []

        def stamp_and_append():
            try:
                f = tbf.stamp(b"concurrent message")
                results.append(f)
            except Exception as e:
                errors.append(e)

        threads = [threading.Thread(target=stamp_and_append) for _ in range(5)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()

        assert len(errors) == 0
        assert len(results) == 5
        for f in results:
            assert tbf.verify(b"concurrent message", f) is True
