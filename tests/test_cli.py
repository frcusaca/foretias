"""Tests for fortias.cli."""

from __future__ import annotations

import json
import tempfile

from fortias._timebeing import _timebeing
from fortias.calendar import Calendar
from fortias.cli import _load_fortis, fortias_verify, main
from fortias.crypto import generate_keypair, sign
from fortias.models import TickRecord


def _make_test_files(tbid, content):
    """Create temp calendar and fortis files. Returns (cal_path, fortis_path, content_bytes)."""
    sk, pk = generate_keypair()
    fortis = _timebeing._stamp(
        content=content, tbid=tbid, tick_number=0,
        private_key=sk,
    )
    genesis_ma = (
        tbid
        + b"\x00" * 8
        + pk
        + b"\x00" * 8
        + pk
    )
    forward = sign(genesis_ma, sk)
    backward = sign(genesis_ma, sk)
    cal = Calendar(tbid, "Time Being test", True, 60, ticks=[])
    cal.append(TickRecord(0, pk, forward, backward))

    with tempfile.TemporaryDirectory() as tmpdir:
        import pathlib
        cal_path = f"{tmpdir}/calendar.json"
        cal.save(cal_path)

        fortis_path = f"{tmpdir}/fortis.json"
        fortis_data = {
            "tick_number": fortis.tick_number,
            "my_content_hash": fortis.my_content_hash.hex(),
            "signature": fortis.signature.hex(),
            "tbid": fortis.tbid.hex(),
            "echo": fortis.echo,
            "tbn": fortis.tbn,
        }
        pathlib.Path(fortis_path).write_text(json.dumps(fortis_data))

        content_path = f"{tmpdir}/message.txt"
        content_bytes = content if isinstance(content, bytes) else content.encode("utf-8")
        pathlib.Path(content_path).write_bytes(content_bytes)

        yield cal_path, fortis_path, content_path, content_bytes


class TestLoadFortis:
    def test_load_valid_fortis(self):
        tbid = b"test-tbid-123456789012345678901234"
        for cal_path, fortis_path, content_path, content_bytes in _make_test_files(tbid, b"hello"):
            fortis = _load_fortis(fortis_path)
            assert fortis.tick_number == 0
            assert len(fortis.my_content_hash) == 32
            assert len(fortis.signature) == 64


class TestFortiasVerify:
    def test_valid_fortis(self):
        tbid = b"test-tbid-123456789012345678901234"
        for cal_path, fortis_path, content_path, content_bytes in _make_test_files(tbid, b"hello"):
            cal = Calendar.load(cal_path)
            fortis = _load_fortis(fortis_path)
            assert fortias_verify(content_bytes, fortis, cal) is True

    def test_tampered_content_fails(self):
        tbid = b"test-tbid-123456789012345678901234"
        for cal_path, fortis_path, content_path, content_bytes in _make_test_files(tbid, b"hello"):
            cal = Calendar.load(cal_path)
            fortis = _load_fortis(fortis_path)
            assert fortias_verify(b"world", fortis, cal) is False


class TestCLI:
    def test_verify_valid(self):
        tbid = b"test-tbid-123456789012345678901234"
        for cal_path, fortis_path, content_path, content_bytes in _make_test_files(tbid, b"hello"):
            rc = main(["verify", "--calendar", cal_path, "--fortis", fortis_path, "--file", content_path])
            assert rc == 0

    def test_verify_invalid(self):
        tbid = b"test-tbid-123456789012345678901234"
        for cal_path, fortis_path, content_path, content_bytes in _make_test_files(tbid, b"hello"):
            # Tamper with the content
            import pathlib
            pathlib.Path(content_path).write_bytes(b"tampered")
            rc = main(["verify", "--calendar", cal_path, "--fortis", fortis_path, "--file", content_path])
            assert rc == 1

    def test_no_command(self):
        rc = main([])
        assert rc == 1
