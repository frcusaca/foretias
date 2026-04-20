"""Stress tests for fortias.TimebeingFamily."""

from __future__ import annotations

import tempfile

from fortias import TimebeingFamily


class TestStress:
    def test_many_ticks_and_stamps(self):
        """Generate 100+ ticks, stamp multiple messages, verify all."""
        tbf = TimebeingFamily(serialized=True)

        stamps = []
        for i in range(100):
            f = tbf.stamp(f"message {i}")
            stamps.append((f"message {i}", f))
            tbf.tick()

        # Verify all stamps
        for content, fortis in stamps:
            assert tbf.verify(content, fortis) is True

        # Verify chain integrity
        assert tbf.calendar.integrity_check() is True

    def test_tampered_content_fails(self):
        """Tampered messages should fail verification."""
        tbf = TimebeingFamily(serialized=True)
        fortis = tbf.stamp(b"original")

        assert tbf.verify(b"original", fortis) is True
        assert tbf.verify(b"tampered", fortis) is False

    def test_save_load_reverify(self):
        """Save calendar, reload, re-verify all stamps."""
        with tempfile.TemporaryDirectory() as tmpdir:
            tbf = TimebeingFamily(serialized=True, persist_path=tmpdir)

            stamps = []
            for i in range(50):
                f = tbf.stamp(f"pre-reload msg {i}")
                stamps.append((f"pre-reload msg {i}", f))
                tbf.tick()
            tbf.save()

            # Reload dormant
            tbf2 = TimebeingFamily.load(persist_path=tmpdir)

            # Verify all stamps through the dormant instance
            for content, fortis in stamps:
                assert tbf2.verify(content, fortis) is True
