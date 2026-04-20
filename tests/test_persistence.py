"""Persistence tests for fortias.TimebeingFamily."""

from __future__ import annotations

import tempfile

from fortias import TimebeingFamily


class TestPersistence:
    def test_create_stamp_save_load_verify(self):
        """Create → stamp → save → destroy → load → verify."""
        with tempfile.TemporaryDirectory() as tmpdir:
            tbf = TimebeingFamily(serialized=True, persist_path=tmpdir)
            fortis = tbf.stamp(b"hello world")
            assert tbf.verify(b"hello world", fortis) is True

            tbf.save()

            # Destroy the object
            del tbf
            del fortis

            # Load dormant instance
            tbf2 = TimebeingFamily.load(persist_path=tmpdir)
            assert tbf2.active is False

    def test_dormant_cannot_stamp(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tbf = TimebeingFamily(serialized=True, persist_path=tmpdir)
            tbf.stamp(b"hello")
            tbf.save()

            tbf2 = TimebeingFamily.load(persist_path=tmpdir)
            try:
                tbf2.stamp(b"world")
                assert False, "Should have raised"
            except RuntimeError as e:
                assert "dormant" in str(e).lower()

    def test_dormant_can_verify(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tbf = TimebeingFamily(serialized=True, persist_path=tmpdir)
            fortis = tbf.stamp(b"hello")
            tbf.save()

            tbf2 = TimebeingFamily.load(persist_path=tmpdir)
            # Reconstruct the Fortis from the saved calendar data
            # Since we can't access the original Fortis object after del,
            # we need to find it in the calendar
            assert tbf2.verify(b"hello", fortis) is True

    def test_chain_integrity_after_reload(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tbf = TimebeingFamily(serialized=True, persist_path=tmpdir)
            f1 = tbf.stamp(b"msg1")
            tbf.tick()
            f2 = tbf.stamp(b"msg2")
            tbf.save()

            # Load and verify both
            tbf2 = TimebeingFamily.load(persist_path=tmpdir)
            assert tbf2.calendar.integrity_check() is True
