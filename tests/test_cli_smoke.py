"""CLI smoke tests for the foretis command.

Verifies that the CLI exits with expected codes for stamp, verify,
and integrity operations. Uses the Rust-backed PyTimeFamilyServer.
"""
from __future__ import annotations

import glob
import json
import os
import subprocess
import sys
import tempfile

import pytest


def _run_foretis(args: list[str]) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, "-m", "foretias"] + args,
        capture_output=True,
        timeout=30,
    )


def test_stamp_exits_zero():
    result = _run_foretis(["stamp", "-m", "hello"])
    assert result.returncode == 0

    output = json.loads(result.stdout)
    assert "tick_number" in output
    assert "content_hash" in output


def test_stamp_with_file():
    with tempfile.NamedTemporaryFile(mode="w", suffix=".txt", delete=False) as f:
        f.write("file content for stamp")
        f.flush()
        tmp_path = f.name

    try:
        result = _run_foretis(["stamp", "-M", tmp_path])
        assert result.returncode == 0

        output = json.loads(result.stdout)
        assert output["tick_number"] >= 1
    finally:
        os.unlink(tmp_path)


def test_stamp_output_to_file():
    with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as f:
        tmp_path = f.name

    try:
        result = _run_foretis(["stamp", "-m", "output test", "-o", tmp_path])
        assert result.returncode == 0

        with open(tmp_path) as f:
            saved = json.load(f)
        assert "tick_number" in saved
    finally:
        os.unlink(tmp_path)


def test_verify_valid_exits_zero():
    from foretias import TimeFamilyServer

    with tempfile.TemporaryDirectory() as tmpdir:
        cal_dir = os.path.join(tmpdir, "cal")
        stamp_path = os.path.join(tmpdir, "stamp.json")

        server = TimeFamilyServer(persist_path=cal_dir)
        foretis = server.stamp(b"verify me", "")
        with open(stamp_path, "w") as f:
            f.write(foretis.to_json())
        server.save()

        cal_file = glob.glob(os.path.join(cal_dir, "*.json"))[0]
        result = _run_foretis(["verify", "-m", "verify me", "-F", stamp_path, "--persist-path", cal_file])
        assert result.returncode == 0

        output = json.loads(result.stdout)
        assert output["valid"] is True


def test_verify_wrong_content_exits_nonzero():
    from foretias import TimeFamilyServer

    with tempfile.TemporaryDirectory() as tmpdir:
        cal_dir = os.path.join(tmpdir, "cal")
        stamp_path = os.path.join(tmpdir, "stamp.json")

        server = TimeFamilyServer(persist_path=cal_dir)
        foretis = server.stamp(b"original", "")
        with open(stamp_path, "w") as f:
            f.write(foretis.to_json())
        server.save()

        cal_file = glob.glob(os.path.join(cal_dir, "*.json"))[0]
        result = _run_foretis(["verify", "-m", "tampered", "-F", stamp_path, "--persist-path", cal_file])
        assert result.returncode != 0


def test_integrity_exits_zero():
    from foretias import TimeFamilyServer

    with tempfile.TemporaryDirectory() as tmpdir:
        cal_path = os.path.join(tmpdir, "cal")

        server = TimeFamilyServer(persist_path=cal_path)
        server.stamp(b"tick one", "")
        server.stamp(b"tick two", "")
        server.save()

        cal_file = glob.glob(os.path.join(cal_path, "*.json"))[0]
        result = _run_foretis(["integrity", "--persist-path", cal_file])
        assert result.returncode == 0

        output = json.loads(result.stdout)
        assert "all_valid" in output
        assert "pairs_checked" in output


def test_no_command_exits_nonzero():
    result = _run_foretis([])
    assert result.returncode != 0


def test_serve_stub_exits_nonzero():
    result = _run_foretis(["serve"])
    assert result.returncode != 0
