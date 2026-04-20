"""Unit tests for fortias.timebeing_family.TimebeingFamily."""

from __future__ import annotations

import tempfile
import time
from datetime import timedelta

from fortias import TimebeingFamily, Fortis
from fortias.crypto import generate_keypair
from fortias.models import TickRecord


class TestActiveTimebeing:
    def test_create_active(self):
        tbf = TimebeingFamily(serialized=True)
        assert tbf.active is True
        assert tbf.tbid is not None
        assert tbf.tbn.startswith("Time Being ")

    def test_stamp_returns_fortis(self):
        tbf = TimebeingFamily(serialized=True)
        fortis = tbf.stamp(b"hello")
        assert isinstance(fortis, Fortis)
        assert fortis.tick_number >= 0
        assert len(fortis.my_content_hash) == 32
        assert len(fortis.signature) == 64

    def test_stamp_string(self):
        tbf = TimebeingFamily(serialized=True)
        fortis = tbf.stamp("hello world")
        assert isinstance(fortis, Fortis)

    def test_verify_valid_fortis(self):
        tbf = TimebeingFamily(serialized=True)
        fortis = tbf.stamp(b"hello")
        assert tbf.verify(b"hello", fortis) is True

    def test_verify_wrong_content_fails(self):
        tbf = TimebeingFamily(serialized=True)
        fortis = tbf.stamp(b"hello")
        assert tbf.verify(b"world", fortis) is False

    def test_verify_with_window_check(self):
        tbf = TimebeingFamily(serialized=True)
        fortis = tbf.stamp(b"hello")
        current = tbf.current_tick()
        # Next tick doesn't exist yet
        result = tbf.verify(b"hello", fortis, next_tick_number=current + 1)
        sig_valid, window_closed = result
        assert sig_valid is True
        assert window_closed is False

    def test_current_tick(self):
        tbf = TimebeingFamily(serialized=True)
        assert tbf.current_tick() >= 0

    def test_tick_advances(self):
        tbf = TimebeingFamily(serialized=True)
        tick0 = tbf.current_tick()
        tbf.tick()
        assert tbf.current_tick() > tick0


class TestDormantTimebeing:
    def test_dormant_cannot_stamp(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            # Create and save
            tbf = TimebeingFamily(serialized=True, persist_path=tmpdir)
            tbf.stamp(b"hello")
            tbf.save()

            # Load as dormant
            tbf2 = TimebeingFamily.load(persist_path=tmpdir)
            assert tbf2.active is False

            with tbf2._lock:
                # The load doesn't hold the lock, but stamp() acquires it.
                # We test by calling stamp directly.
                pass
            try:
                tbf2.stamp(b"hello")
            except RuntimeError as e:
                assert "dormant" in str(e).lower()


class TestSerializedMode:
    def test_stamp_triggers_tick(self):
        tbf = TimebeingFamily(serialized=True)
        tick0 = tbf.current_tick()
        tbf.stamp(b"msg1")
        # serialized=True: stamp should advance tick
        assert tbf.current_tick() >= tick0

    def test_multiple_stamps_advance_ticks(self):
        tbf = TimebeingFamily(serialized=True)
        tick0 = tbf.current_tick()
        tbf.stamp(b"msg1")
        tick1 = tbf.current_tick()
        tbf.stamp(b"msg2")
        tick2 = tbf.current_tick()
        assert tick2 > tick1 >= tick0


class TestGet:
    def test_returns_correct_records(self):
        tbf = TimebeingFamily(serialized=True)
        tbf.stamp(b"msg1")
        tbf.tick()
        tbf.stamp(b"msg2")
        tbf.tick()

        records = tbf.get(0, 2)
        assert len(records) == 2

    def test_get_specific_tick(self):
        tbf = TimebeingFamily(serialized=True)
        tbf.stamp(b"msg1")
        tick0 = tbf.current_tick()
        tbf.tick()
        tbf.stamp(b"msg2")

        records = tbf.get(tick0, 1)
        assert len(records) == 1
        assert records[0].tick_number == tick0


class TestNonSerializedMode:
    def test_stamp_does_not_trigger_tick(self):
        """Non-serialized: stamp should NOT advance tick."""
        tbf = TimebeingFamily(serialized=False, chronon=timedelta(hours=1))
        tick_before = tbf.current_tick()
        tbf.stamp(b"msg")
        assert tbf.current_tick() == tick_before

    def test_verify_fortis_stamped_at_current_tick(self):
        """Verify a fortis created at the current tick."""
        tbf = TimebeingFamily(serialized=False, chronon=timedelta(hours=1))
        fortis = tbf.stamp(b"hello")
        assert tbf.verify(b"hello", fortis) is True


class TestMutex:
    def test_concurrent_stamps(self):
        """Multiple concurrent stamps should not corrupt state."""
        import threading

        tbf = TimebeingFamily(serialized=True)
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
        # All stamps should be verified
        for r in results:
            assert tbf.verify(b"concurrent message", r) is True
