"""Unit tests for fortias.calendar."""

from __future__ import annotations

import json
import tempfile

import pytest

from fortias._timebeing import _genesis_ma, _timebeing
from fortias.calendar import Calendar
from fortias.crypto import generate_keypair, sign
from fortias.models import TickRecord


def _make_valid_genesis(tbid, pk, sk):
    """Create a genesis TickRecord with self-transition MA."""
    genesis_ma = _genesis_ma(tbid, pk)
    forward = sign(genesis_ma, sk)
    backward = sign(genesis_ma, sk)
    return TickRecord(tick_number=0, public_key=pk, forward_fortis=forward, backward_fortis=backward)


class TestAppend:
    def test_append_to_empty(self):
        cal = Calendar(b"tbid", "Time Being test", False, 60)
        _, pk = generate_keypair()
        cal.append(TickRecord(0, pk, b"sig", b"sig"))
        assert len(cal.ticks) == 1

    def test_rejects_equal_tick_number(self):
        cal = Calendar(b"tbid", "Time Being test", False, 60)
        _, pk1 = generate_keypair()
        _, pk2 = generate_keypair()
        cal.append(TickRecord(100, pk1, b"sig", b"sig"))
        with pytest.raises(ValueError, match="strictly increasing"):
            cal.append(TickRecord(100, pk2, b"sig2", b"sig"))

    def test_rejects_earlier_tick_number(self):
        cal = Calendar(b"tbid", "Time Being test", False, 60)
        _, pk1 = generate_keypair()
        _, pk2 = generate_keypair()
        cal.append(TickRecord(200, pk1, b"sig", b"sig"))
        with pytest.raises(ValueError, match="strictly increasing"):
            cal.append(TickRecord(100, pk2, b"sig2", b"sig"))

    def test_accepts_strictly_increasing(self):
        cal = Calendar(b"tbid", "Time Being test", False, 60)
        _, pk1 = generate_keypair()
        _, pk2 = generate_keypair()
        cal.append(TickRecord(100, pk1, b"sig", b"sig"))
        cal.append(TickRecord(200, pk2, b"sig2", b"sig"))
        assert len(cal.ticks) == 2


class TestGet:
    def test_returns_ticks_at_or_after(self):
        cal = Calendar(b"tbid", "Time Being test", False, 60)
        _, pk1 = generate_keypair()
        _, pk2 = generate_keypair()
        _, pk3 = generate_keypair()
        cal.append(TickRecord(100, pk1, b"sig1", b"sig1"))
        cal.append(TickRecord(200, pk2, b"sig2", b"sig1"))
        cal.append(TickRecord(300, pk3, b"sig3", b"sig2"))

        result = cal.get(200, 2)
        assert len(result) == 2
        assert result[0].tick_number == 200
        assert result[1].tick_number == 300

    def test_returns_all_from_start(self):
        cal = Calendar(b"tbid", "Time Being test", False, 60)
        _, pk1 = generate_keypair()
        _, pk2 = generate_keypair()
        cal.append(TickRecord(100, pk1, b"sig1", b"sig1"))
        cal.append(TickRecord(200, pk2, b"sig2", b"sig1"))

        result = cal.get(0, 10)
        assert len(result) == 2

    def test_returns_one(self):
        cal = Calendar(b"tbid", "Time Being test", False, 60)
        _, pk = generate_keypair()
        cal.append(TickRecord(100, pk, b"sig", b"sig"))

        result = cal.get(100, 1)
        assert len(result) == 1
        assert result[0].tick_number == 100


class TestLatest:
    def test_empty_calendar(self):
        cal = Calendar(b"tbid", "Time Being test", False, 60)
        assert cal.latest() is None

    def test_returns_highest_tick(self):
        cal = Calendar(b"tbid", "Time Being test", False, 60)
        _, pk1 = generate_keypair()
        _, pk2 = generate_keypair()
        cal.append(TickRecord(100, pk1, b"sig1", b"sig1"))
        cal.append(TickRecord(200, pk2, b"sig2", b"sig1"))
        assert cal.latest() == 200


class TestPersistence:
    def test_save_load_roundtrip(self):
        tbid = b"test-tbid-123456789012345678901234"
        cal = Calendar(tbid, "Time Being test", True, 60)
        sk0, pk0 = generate_keypair()
        cal.append(_make_valid_genesis(tbid, pk0, sk0))

        r1, sk1 = _timebeing._tick(tbid, cal.ticks[-1], sk0)
        cal.append(r1)

        with tempfile.NamedTemporaryFile(suffix=".json") as f:
            cal.save(f.name)
            loaded = Calendar.load(f.name)

        assert loaded.tbid == tbid
        assert loaded.tbn == "Time Being test"
        assert loaded.serialized is True
        assert loaded.chronon_seconds == 60
        assert len(loaded.ticks) == 2
        assert loaded.ticks[0].tick_number == 0
        assert loaded.ticks[0].public_key == pk0
        assert loaded.ticks[0].forward_fortis is not None
        assert loaded.ticks[0].backward_fortis is not None
        assert loaded.ticks[1].public_key == r1.public_key

    def test_save_load_hex_encoding(self):
        cal = Calendar(b"tbid", "Time Being test", False, 60)
        _, pk = generate_keypair()
        cal.append(TickRecord(42, pk, b"sig", b"sig"))

        with tempfile.NamedTemporaryFile(suffix=".json") as f:
            cal.save(f.name)
            raw = json.loads(f.read())

        assert raw["ticks"][0]["public_key"] == pk.hex()
        assert raw["ticks"][0]["forward_fortis"] == b"sig".hex()

    def test_load_invalid_hex_raises(self):
        with tempfile.NamedTemporaryFile(suffix=".json") as f:
            f.write(json.dumps({
                "tbid": "not_valid_hex!!",
                "tbn": "test",
                "serialized": False,
                "chronon_seconds": 60,
                "ticks": [{"tick_number": 0, "public_key": "abc", "forward_fortis": "def", "backward_fortis": "ef"}],
            }).encode())
            f.flush()
            with pytest.raises(ValueError, match="valid hex strings"):
                Calendar.load(f.name)

    def test_load_non_ascending_tick_numbers_raises(self):
        with tempfile.NamedTemporaryFile(suffix=".json") as f:
            f.write(json.dumps({
                "tbid": "61626364",
                "tbn": "test",
                "serialized": False,
                "chronon_seconds": 60,
                "ticks": [
                    {"tick_number": 200, "public_key": "aa" * 32, "forward_fortis": "bb" * 64, "backward_fortis": "cc" * 64},
                    {"tick_number": 100, "public_key": "cc" * 32, "forward_fortis": "dd" * 64, "backward_fortis": "ee" * 64},
                ],
            }).encode())
            f.flush()
            with pytest.raises(ValueError, match="strictly ascending"):
                Calendar.load(f.name)

    def test_load_negative_tick_number_raises(self):
        with tempfile.NamedTemporaryFile(suffix=".json") as f:
            f.write(json.dumps({
                "tbid": "61626364",
                "tbn": "test",
                "serialized": False,
                "chronon_seconds": 60,
                "ticks": [{"tick_number": -1, "public_key": "aa" * 32, "forward_fortis": "bb" * 64, "backward_fortis": "cc" * 64}],
            }).encode())
            f.flush()
            with pytest.raises(ValueError, match="non-negative"):
                Calendar.load(f.name)

    def test_load_chain_integrity_failure_raises(self):
        tbid = b"test-tbid-123456789012345678901234"
        cal = Calendar(tbid, "Time Being test", False, 60)
        sk0, pk0 = generate_keypair()
        cal.append(_make_valid_genesis(tbid, pk0, sk0))

        r1, sk1 = _timebeing._tick(tbid, cal.ticks[-1], sk0)
        cal.append(r1)

        r2, _ = _timebeing._tick(tbid, r1, sk1)
        cal.append(r2)

        with tempfile.NamedTemporaryFile(suffix=".json") as f:
            cal.save(f.name)
            # Tamper with the backward_fortis of tick 1
            data = json.loads(f.read())
            data["ticks"][1]["backward_fortis"] = "ff" * 64
            f.seek(0)
            f.write(json.dumps(data).encode())
            f.truncate()
            f.flush()
            with pytest.raises(ValueError, match="integrity check failed"):
                Calendar.load(f.name)


class TestIntegrityCheck:
    def test_empty_calendar(self):
        cal = Calendar(b"tbid", "Time Being test", False, 60)
        assert cal.integrity_check() is True

    def test_single_record(self):
        cal = Calendar(b"tbid", "Time Being test", False, 60)
        _, pk = generate_keypair()
        cal.append(TickRecord(0, pk, b"sig", b"sig"))
        # Single record: no pairs to verify
        assert cal.integrity_check() is True

    def test_valid_chain(self):
        tbid = b"test-tbid-123456789012345678901234"
        cal = Calendar(tbid, "Time Being test", False, 60)
        sk0, pk0 = generate_keypair()
        cal.append(_make_valid_genesis(tbid, pk0, sk0))

        r1, _ = _timebeing._tick(tbid, cal.ticks[-1], sk0)
        cal.append(r1)

        assert cal.integrity_check() is True

    def test_three_record_chain(self):
        tbid = b"test-tbid-123456789012345678901234"
        cal = Calendar(tbid, "Time Being test", False, 60)
        sk0, pk0 = generate_keypair()
        cal.append(_make_valid_genesis(tbid, pk0, sk0))

        r1, sk1 = _timebeing._tick(tbid, cal.ticks[-1], sk0)
        cal.append(r1)

        r2, _ = _timebeing._tick(tbid, r1, sk1)
        cal.append(r2)

        assert cal.integrity_check() is True

    def test_return_failures(self):
        tbid = b"test-tbid-123456789012345678901234"
        cal = Calendar(tbid, "Time Being test", False, 60)
        sk0, pk0 = generate_keypair()
        cal.append(_make_valid_genesis(tbid, pk0, sk0))

        r1, sk1 = _timebeing._tick(tbid, cal.ticks[-1], sk0)
        cal.append(r1)

        _, broken_pk = generate_keypair()
        cal.append(TickRecord(r1.tick_number + 1, broken_pk, b"bad_sig", b"bad_sig"))

        ok, failures = cal.integrity_check(return_failures=True)
        assert ok is False
        assert 1 in failures
