"""Tests for foretias.cli."""

from __future__ import annotations

import json
import tempfile

from foretias._timebeing import _genesis_ma, _timebeing
from foretias.calendar import Calendar
from foretias.cli import _load_foretis, foretias_verify, main
from foretias.crypto import generate_keypair, sign
from foretias.models import TickRecord


def _make_test_files(tbid, content):
    """Create temp calendar and foretis files. Returns (cal_path, foretis_path, content_bytes)."""
    sk, pk = generate_keypair()
    foretis = _timebeing._stamp(
        content=content, tbid=tbid, tick_number=0,
        private_key=sk,
    )
    genesis_ma = _genesis_ma(tbid, pk)
    forward = sign(genesis_ma, sk)
    backward = sign(genesis_ma, sk)
    cal = Calendar(tbid, "Time Being test", ticks=[])
    cal.append(TickRecord(0, pk, forward, backward))

    with tempfile.TemporaryDirectory() as tmpdir:
        import pathlib
        cal_path = f"{tmpdir}/calendar.json"
        cal.save(cal_path)

        foretis_path = f"{tmpdir}/foretis.json"
        foretis_data = {
            "tick_number": foretis.tick_number,
            "my_content_hash": foretis.my_content_hash.hex(),
            "signature": foretis.signature.hex(),
            "tbid": foretis.tbid.hex(),
            "echo": foretis.echo,
            "tbn": foretis.tbn,
        }
        pathlib.Path(foretis_path).write_text(json.dumps(foretis_data))

        content_path = f"{tmpdir}/message.txt"
        content_bytes = content if isinstance(content, bytes) else content.encode("utf-8")
        pathlib.Path(content_path).write_bytes(content_bytes)

        yield cal_path, foretis_path, content_path, content_bytes


class TestLoadForetis:
    def test_load_valid_foretis(self):
        tbid = b"test-tbid-123456789012345678901234"
        for cal_path, foretis_path, content_path, content_bytes in _make_test_files(tbid, b"hello"):
            foretis = _load_foretis(foretis_path)
            assert foretis.tick_number == 0
            assert len(foretis.my_content_hash) == 32
            assert len(foretis.signature) == 64


class TestForetiasVerify:
    def test_valid_foretis(self):
        tbid = b"test-tbid-123456789012345678901234"
        for cal_path, foretis_path, content_path, content_bytes in _make_test_files(tbid, b"hello"):
            cal = Calendar.load(cal_path)
            foretis = _load_foretis(foretis_path)
            assert foretias_verify(content_bytes, foretis, cal) is True

    def test_tampered_content_fails(self):
        tbid = b"test-tbid-123456789012345678901234"
        for cal_path, foretis_path, content_path, content_bytes in _make_test_files(tbid, b"hello"):
            cal = Calendar.load(cal_path)
            foretis = _load_foretis(foretis_path)
            assert foretias_verify(b"world", foretis, cal) is False


class TestCLI:
    def test_verify_valid(self):
        tbid = b"test-tbid-123456789012345678901234"
        for cal_path, foretis_path, content_path, content_bytes in _make_test_files(tbid, b"hello"):
            rc = main(["verify", "--calendar", cal_path, "--foretis", foretis_path, "--file", content_path])
            assert rc == 0

    def test_verify_invalid(self):
        tbid = b"test-tbid-123456789012345678901234"
        for cal_path, foretis_path, content_path, content_bytes in _make_test_files(tbid, b"hello"):
            # Tamper with the content
            import pathlib
            pathlib.Path(content_path).write_bytes(b"tampered")
            rc = main(["verify", "--calendar", cal_path, "--foretis", foretis_path, "--file", content_path])
            assert rc == 1

    def test_no_command(self):
        rc = main([])
        assert rc == 1
