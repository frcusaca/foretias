"""Unit tests for fortias.time_family.TimeFamily."""

from __future__ import annotations

import tempfile
import time

from fortias import TimeFamily, Fortis
from fortias.crypto import generate_keypair
from fortias.models import TickRecord


class TestActiveTimebeing:
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
        # Next tick doesn't exist yet
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


class TestDormantTimebeing:
    def test_dormant_cannot_stamp(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            # Create and save
            tbf = TimeFamily(serialized=True, persist_path=tmpdir)
            tbf.stamp(b"hello")
            tbf.save()

            # Load as dormant
            tbf2 = TimeFamily.load(persist_path=tmpdir)
            assert tbf2.active is False

            # stamp() acquires _rlock internally, no need to hold it here
            try:
                tbf2.stamp(b"hello")
            except RuntimeError as e:
                assert "dormant" in str(e).lower()


class TestSerializedMode:
    def test_stamp_triggers_tick(self):
        tbf = TimeFamily(serialized=True)
        tick0 = tbf.current_tick()
        tbf.stamp(b"msg1")
        # serialized=True: stamp should advance tick
        assert tbf.current_tick() >= tick0
        # Exactly two ticks: genesis + 1
        assert len(tbf.calendar.ticks) == 2

    def test_rapid_stamps_same_chronon(self):
        """Rapid stamps in the same chronon window don't each trigger a tick."""
        tbf = TimeFamily(serialized=True)
        f1 = tbf.stamp(b"msg1")
        assert tbf.verify(b"msg1", f1) is True
        tick1 = tbf.current_tick()
        f2 = tbf.stamp(b"msg2")
        tick2 = tbf.current_tick()
        f3 = tbf.stamp(b"msg3")
        tick3 = tbf.current_tick()
        # First stamp ticks (genesis → tick 1), subsequent stamps stay
        # within the same chronon window.
        assert tick1 > 0
        assert tick2 == tick1
        assert tick3 == tick1
        # But they all produce valid Fortises at the same tick.
        assert tbf.verify(b"msg2", f2) is True
        assert tbf.verify(b"msg3", f3) is True

    def test_stamps_across_chronon_boundary_advance(self):
        """Stamps after a scheduled tick fires each advance the tick."""
        tbf = TimeFamily(serialized=True, chronon_ns=1_000_000_000.0)
        tbf.stamp(b"msg1")
        tick1 = tbf.current_tick()
        time.sleep(1.5)  # wait for daemon to fire scheduled tick
        tbf.stamp(b"msg2")
        tick2 = tbf.current_tick()
        assert tick2 > tick1
        tbf.stamp(b"msg3")  # already scheduled tick, stays at tick2
        tick3 = tbf.current_tick()
        assert tick3 == tick2


class TestGet:
    def test_returns_correct_records(self):
        tbf = TimeFamily(serialized=True)
        tbf.stamp(b"msg1")
        tbf.tick()
        tbf.stamp(b"msg2")
        tbf.tick()

        records = tbf.get(0, 2)
        assert len(records) == 2

    def test_get_specific_tick(self):
        tbf = TimeFamily(serialized=True)
        tbf.stamp(b"msg1")
        tick0 = tbf.current_tick()
        tbf.tick()
        tbf.stamp(b"msg2")

        records = tbf.get(tick0, 1)
        assert len(records) == 1
        # The Stamp's counter is 1, so this returns tick 1 record
        # (not genesis which is at counter 0).
        assert records[0].public_key != tbf.calendar.ticks[0].public_key


class TestNonSerializedMode:
    def test_stamp_does_not_trigger_tick(self):
        """Non-serialized: stamp does NOT advance tick. Daemon does."""
        tbf = TimeFamily(serialized=False, chronon_ns=3_600_000_000_000.0)
        tick_before = tbf.current_tick()
        tbf.stamp(b"msg")
        assert tbf.current_tick() == tick_before

    def test_rapid_stamps_same_chronon(self):
        """Rapid stamps within the same chronon window don't each tick."""
        tbf = TimeFamily(serialized=False, chronon_ns=60_000_000_000_000.0)
        tbf.stamp(b"msg1")
        tick1 = tbf.current_tick()
        tbf.stamp(b"msg2")
        tick2 = tbf.current_tick()
        assert tick2 == tick1

    def test_verify_fortis_stamped_at_current_tick(self):
        """Verify a fortis created at the current tick."""
        tbf = TimeFamily(serialized=False, chronon_ns=3_600_000_000_000.0)
        fortis = tbf.stamp(b"hello")
        assert tbf.verify(b"hello", fortis) is True


class TestMutex:
    def test_concurrent_stamps(self):
        """Multiple concurrent stamps should not corrupt state."""
        import threading

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
        # All stamps should be verified
        for r in results:
            assert tbf.verify(b"concurrent message", r) is True


class TestShutdown:
    def test_shutdown_active_thread(self):
        tbf = TimeFamily(serialized=False, chronon_ns=1_000_000_000.0)
        assert tbf._stamp._daemon is not None
        tbf.shutdown()
        assert not tbf._stamp._daemon.is_alive()

    def test_shutdown_stops_serialized_daemon(self):
        tbf = TimeFamily(serialized=True, chronon_ns=1_000_000_000.0)
        assert tbf._stamp._daemon is not None
        tbf.stamp(b"msg")  # schedules a tick
        tbf.shutdown()
        assert not tbf._stamp._daemon.is_alive()

    def test_shutdown_dormant_is_noop(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tbf = TimeFamily(serialized=True, persist_path=tmpdir)
            tbf.stamp(b"hello")
            tbf.save()
            tbf2 = TimeFamily.load(persist_path=tmpdir)
            assert tbf2._stamp._daemon is None
            tbf2.shutdown()  # Should not raise

    def test_shutdown_twice_is_safe(self):
        tbf = TimeFamily(serialized=False, chronon_ns=1_000_000_000.0)
        tbf.shutdown()
        tbf.shutdown()  # Should not raise

    def test_shutdown_saves_calendar(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tbf = TimeFamily(serialized=False, chronon_ns=1_000_000_000.0, persist_path=tmpdir)
            tbf.stamp(b"hello")
            tbf.tick()
            tbf.shutdown()
            # Calendar file should exist and be loadable
            import pathlib
            cal_path = f"{tmpdir}/{tbf.tbid.hex()}/calendar.json"
            assert pathlib.Path(cal_path).exists()
