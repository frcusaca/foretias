"""Shim import and basic roundtrip tests.

Verifies that the foretias shim correctly re-exports from foretias_p2p
and that basic stamp/verify operations work through the shim.
"""
from __future__ import annotations

import json
import pytest


def test_shim_exports_time_family_server():
    from foretias import TimeFamilyServer
    assert TimeFamilyServer is not None


def test_shim_exports_time_family():
    from foretias import TimeFamily
    assert TimeFamily is not None


def test_shim_exports_crypto_server():
    from foretias import CryptoServer
    assert CryptoServer is not None


def test_shim_exports_foretis():
    from foretias import Foretis
    assert Foretis is not None


def test_shim_exports_tick_record():
    from foretias import TickRecord
    assert TickRecord is not None


def test_shim_exports_calendar():
    from foretias import Calendar
    assert Calendar is not None


def test_shim_exports_version():
    from foretias import __version__
    assert __version__ is not None
    assert isinstance(__version__, str)
    assert len(__version__) > 0


def test_stamp_and_verify_roundtrip_via_server():
    """Stamp and verify using TimeFamilyServer (full server stack)."""
    from foretias import TimeFamilyServer

    server = TimeFamilyServer()
    foretis = server.stamp(b"hello world", "")

    assert foretis is not None
    assert foretis.tick_number >= 1
    assert foretis.content_hash is not None
    assert len(foretis.signature) > 0
    assert foretis.tbid is not None

    j = json.loads(foretis.to_json())
    assert "tick_number" in j
    assert "content_hash" in j
    assert "signature" in j

    valid = server.verify(b"hello world", foretis)
    assert valid is True


def test_verify_wrong_content_fails_via_server():
    """Verify with wrong content must return False."""
    from foretias import TimeFamilyServer

    server = TimeFamilyServer()
    foretis = server.stamp(b"correct content", "")
    valid = server.verify(b"wrong content", foretis)
    assert valid is False


def test_stamp_and_verify_roundtrip_via_time_family():
    """Stamp and verify using TimeFamily (lighter orchestrator)."""
    from foretias import TimeFamily

    tf = TimeFamily(tbn="test-shim")
    foretis = tf.stamp(b"test data", "echo-test")

    assert foretis.tick_number >= 1
    valid = tf.verify(b"test data", foretis)
    assert valid is True


def test_verify_wrong_content_fails_via_time_family():
    """TimeFamily verify with wrong content must return False."""
    from foretias import TimeFamily

    tf = TimeFamily(tbn="test-shim-fail")
    foretis = tf.stamp(b"original", "")
    valid = tf.verify(b"tampered", foretis)
    assert valid is False


def test_calendar_has_ticks_after_stamp():
    """Calendar should contain tick records after stamping."""
    from foretias import TimeFamily

    tf = TimeFamily(tbn="test-cal")
    tf.stamp(b"tick one", "")
    tf.stamp(b"tick two", "")

    cal = tf.get_calendar()
    assert cal.tick_count() >= 2


def test_crypto_server_sign_and_verify():
    """CryptoServer sign and verify roundtrip."""
    from foretias import CryptoServer

    cs = CryptoServer("ed25519")
    sig = bytes(cs.sign(b"message"))
    pk = bytes(cs.public_key())

    assert len(sig) > 0
    assert len(pk) == 32

    ok = cs.verify(pk, b"message", sig)
    assert ok is True


def test_crypto_server_wrong_message_fails():
    """CryptoServer verify with wrong message must fail."""
    from foretias import CryptoServer

    cs = CryptoServer("ed25519")
    sig = bytes(cs.sign(b"original"))
    pk = bytes(cs.public_key())

    ok = cs.verify(pk, b"tampered", sig)
    assert ok is False


def test_foretis_json_roundtrip():
    """PyForetis to_json and from_json roundtrip."""
    from foretias import TimeFamily

    tf = TimeFamily(tbn="json-test")
    foretis = tf.stamp(b"json roundtrip", "")
    json_str = foretis.to_json()

    restored = foretis.__class__.from_json(json_str)
    assert restored.tick_number == foretis.tick_number
    assert restored.echo == foretis.echo


def test_server_is_not_dormant():
    """Fresh TimeFamilyServer should not be dormant."""
    from foretias import TimeFamilyServer

    server = TimeFamilyServer()
    assert server.is_dormant() is False


def test_persist_and_dormant_verify():
    """Persist calendar, load in dormant mode, verify."""
    import glob
    import os
    import tempfile
    from foretias import TimeFamilyServer

    tmpdir = tempfile.mkdtemp()
    cal_path = os.path.join(tmpdir, "cal")

    server = TimeFamilyServer(persist_path=cal_path)
    foretis = server.stamp(b"persistent data", "")
    server.save()

    cal_file = glob.glob(os.path.join(cal_path, "*.json"))[0]

    dormant = TimeFamilyServer.from_calendar(calendar_path=cal_file)
    assert dormant.is_dormant() is True

    valid = dormant.verify(b"persistent data", foretis)
    assert valid is True
