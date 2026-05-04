"""Aggressive / defensive tests — TimeFamily orchestrator.

Persistence, stress, concurrency, and shutdown edge cases for the full system.
"""

from __future__ import annotations

import pathlib
import tempfile
import threading

import pytest

from foretias import TimeFamily


class TestTimeFamilyPersistence:
    def test_create_stamp_save_load_verify(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tbf = TimeFamily(serialized=True, persist_path=tmpdir)
            foretis = tbf.stamp(b"hello world")
            assert tbf.verify(b"hello world", foretis) is True
            tbf.save()
            tbf.shutdown()

            tbf2 = TimeFamily.load(persist_path=tmpdir)
            assert tbf2.active is False
            assert tbf2.verify(b"hello world", foretis) is True

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

        for content, foretis in stamps:
            assert tbf.verify(content, foretis) is True
        assert tbf.calendar().integrity_check() is True

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
            for content, foretis in stamps:
                assert tbf2.verify(content, foretis) is True


class TestTimeFamilyShutdown:
    def test_shutdown_active_thread(self):
        tbf = TimeFamily(serialized=False, chronon_ns=1_000_000_000.0)
        assert tbf._chronomatter._daemon is not None
        tbf.shutdown()
        assert not tbf._chronomatter._daemon.is_alive()

    def test_shutdown_stops_serialized_daemon(self):
        tbf = TimeFamily(serialized=True, chronon_ns=1_000_000_000.0)
        tbf.stamp(b"msg")
        tbf.shutdown()
        assert not tbf._chronomatter._daemon.is_alive()

    def test_shutdown_dormant_is_noop(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tbf = TimeFamily(serialized=True, persist_path=tmpdir)
            tbf.stamp(b"hello")
            tbf.save()
            tbf.shutdown()
            tbf2 = TimeFamily.load(persist_path=tmpdir)
            assert tbf2._chronomatter._daemon is None
            tbf2.shutdown()

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
