"""Aggressive / defensive tests — component-level.

Slow, destructive, edge-case coverage for Calendar, ChronomatterV1,
ChronomatterV1Serial, and Inquirer in isolation.
"""

from __future__ import annotations

import json
import tempfile
import threading
import time
from unittest.mock import MagicMock

import pytest

from foretias import TimeFamily, Foretis
from foretias.chronomatter import Inquirer, ChronomatterV1, ChronomatterV1Serial
from foretias.calendar import Calendar
from foretias.crypto import generate_keypair
from foretias.models import TickRecord


class TestCalendarDefensive:
    def test_load_invalid_hex_raises(self):
        with tempfile.NamedTemporaryFile(suffix=".json") as f:
            f.write(json.dumps({
                "tbid": "not_valid_hex!!",
                "tbn": "test",
                "ticks": [{"tick_number": 0, "public_key": "aa", "forward_foretis": "bb", "backward_foretis": "cc"}],
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
                    {"tick_number": 200, "public_key": "aa" * 32, "forward_foretis": "bb" * 64, "backward_foretis": "cc" * 64},
                    {"tick_number": 100, "public_key": "cc" * 32, "forward_foretis": "dd" * 64, "backward_foretis": "ee" * 64},
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
                "ticks": [{"tick_number": -1, "public_key": "aa" * 32, "forward_foretis": "bb" * 64, "backward_foretis": "cc" * 64}],
            }).encode())
            f.flush()
            with pytest.raises(ValueError, match="non-negative"):
                Calendar.load(f.name)

    def test_load_tampered_chain_raises(self):
        from foretias._timebeing import _genesis_ma, _timebeing
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
            data["ticks"][1]["backward_foretis"] = "ff" * 64
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
    def _make_cm(self):
        cm = ChronomatterV1(chronon_ns=3_600_000_000_000.0)
        cal = Calendar(cm.tbid, cm.tbn)
        cm._family = MagicMock()
        cm._family._get_calendar.return_value = cal
        return cm, cal

    def test_many_ticks_and_stamps(self):
        cm, cal = self._make_cm()

        stamps = []
        for i in range(100):
            f = cm.stamp(f"message {i}")
            stamps.append((f"message {i}", f))
            cm.tick()

        assert cal.integrity_check() is True
        cm.shutdown()

    def test_dormant_cannot_stamp(self):
        cm, _ = self._make_cm()
        cm._active = False
        with pytest.raises(RuntimeError, match="dormant"):
            cm.stamp(b"hello")
        cm.shutdown()


class TestChronomatterV1SerialAggressive:
    def _make_cm(self):
        cm = ChronomatterV1Serial(chronon_ns=1_000_000_000.0)
        cal = Calendar(cm.tbid, cm.tbn)
        cm._family = MagicMock()
        cm._family._get_calendar.return_value = cal
        return cm, cal

    def test_chronon_boundary(self):
        cm, _ = self._make_cm()
        cm.stamp(b"msg1")
        tick1 = cm.current_tick
        time.sleep(1.5)
        cm.stamp(b"msg2")
        tick2 = cm.current_tick
        assert tick2 > tick1
        cm.stamp(b"msg3")
        assert cm.current_tick == tick2
        cm.shutdown()

    def test_shutdown_twice_safe(self):
        cm = ChronomatterV1Serial()
        cm.shutdown()
        cm.shutdown()

    def test_shutdown_stops_daemon(self):
        cm, _ = self._make_cm()
        cm.stamp(b"msg")
        assert cm._daemon is not None
        cm.shutdown()
        assert not cm._daemon.is_alive()


class TestInquirerAggressive:
    def test_verify_with_chain_integrity(self):
        tbf = TimeFamily(serialized=True)
        tbf.stamp(b"msg1")
        tbf.tick()
        foretis = tbf.stamp(b"msg2")
        assert tbf.inquirer().verify(b"msg2", foretis) is True

    def test_lookup_surrounding_ticks_boundary(self):
        tbf = TimeFamily(serialized=True)
        cal = tbf.calendar()
        iq = tbf.inquirer()

        tbf.stamp(b"msg")
        tbf.tick()

        prev, curr = iq._lookup_surrounding_ticks(999, cal)
        assert prev is None
        assert curr is None

        prev, curr = iq._lookup_surrounding_ticks(-1, cal)
        assert prev is None
        assert curr is None

        prev, curr = iq._lookup_surrounding_ticks(0, cal)
        assert prev is None
        assert curr is not None
