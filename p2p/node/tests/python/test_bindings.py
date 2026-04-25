"""Tests for fortias_p2p PyO3 Python bindings.

Exercises: CryptoServer, Fortis, TickRecord, Calendar, TimeFamily.
Run with: python -m pytest tests/python/test_bindings.py -v
"""

import json
import pytest

from fortias_p2p import (
    PyTimeFamily,
    PyCryptoServer,
    PyFortis,
    PyTickRecord,
    PyCalendar,
)


class TestPyCryptoServer:
    """Test the CryptoServer Python binding."""

    def test_create_ed25519(self):
        cs = PyCryptoServer("ed25519")
        assert cs is not None
        assert "CryptoServer" in repr(cs)

    def test_unsupported_curve(self):
        with pytest.raises(ValueError):
            PyCryptoServer("p256")

    def test_sign_verify(self):
        cs = PyCryptoServer("ed25519")
        msg = b"hello world"
        sig = cs.sign(msg)
        assert len(sig) == 64
        pub_key = cs.public_key()
        assert len(pub_key) == 32
        assert cs.verify(bytes(pub_key), msg, bytes(sig)) is True
        assert cs.verify(bytes(pub_key), b"tampered", bytes(sig)) is False

    def test_peer_id(self):
        cs = PyCryptoServer("ed25519")
        pid = cs.peer_id()
        assert len(pid) == 32

    def test_sha256(self):
        cs = PyCryptoServer("ed25519")
        h = cs.sha256(b"test")
        assert len(h) == 32

    def test_blake3_stub(self):
        cs = PyCryptoServer("ed25519")
        with pytest.raises(RuntimeError, match="not implemented"):
            cs.blake3(b"test")


class TestPyTimeFamily:
    """Test the TimeFamily Python binding."""

    def test_create(self):
        tf = PyTimeFamily(tbn="test-node")
        assert "TimeFamily" in repr(tf)
        assert tf.get_tbn() == "test-node"
        assert len(tf.get_tbid()) == 32  # hex string
        assert tf.get_current_tick() == 0

    def test_stamp(self):
        tf = PyTimeFamily(tbn="test-node")
        fortis = tf.stamp(b"hello world")
        assert isinstance(fortis, PyFortis)
        assert fortis.tick_number == 1
        assert len(fortis.content_hash) == 32
        assert len(fortis.signature) == 64
        assert len(fortis.tbid) == 16
        assert fortis.tbn == "test-node"
        assert len(fortis.echo) > 0

    def test_verify_correct(self):
        tf = PyTimeFamily(tbn="test-node")
        fortis = tf.stamp(b"hello world")
        assert tf.verify(b"hello world", fortis) is True

    def test_verify_wrong_content(self):
        tf = PyTimeFamily(tbn="test-node")
        fortis = tf.stamp(b"hello world")
        assert tf.verify(b"wrong content", fortis) is False

    def test_tick_increment(self):
        tf = PyTimeFamily(tbn="test-node")
        f1 = tf.stamp(b"first")
        f2 = tf.stamp(b"second")
        f3 = tf.stamp(b"third")
        assert f1.tick_number == 1
        assert f2.tick_number == 2
        assert f3.tick_number == 3
        assert tf.get_latest_tick() == 3

    def test_get_calendar(self):
        tf = PyTimeFamily(tbn="test-node")
        tf.stamp(b"tick1")
        tf.stamp(b"tick2")
        cal = tf.get_calendar()
        assert isinstance(cal, PyCalendar)
        assert cal.tick_count() == 2
        assert cal.latest_tick() == 2

    def test_get_public_key(self):
        tf = PyTimeFamily()
        pk = tf.get_public_key()
        assert len(pk) == 64  # hex string of 32 bytes

    def test_get_peer_id(self):
        tf = PyTimeFamily()
        pid = tf.get_peer_id()
        assert len(pid) == 64  # hex string of 32 bytes


class TestPyFortis:
    """Test the Fortis Python binding."""

    def test_repr(self):
        tf = PyTimeFamily(tbn="test-node")
        f = tf.stamp(b"hello")
        assert "Fortis" in repr(f)
        assert "test-node" in repr(f)

    def test_to_json(self):
        tf = PyTimeFamily(tbn="test-node")
        f = tf.stamp(b"hello")
        j = json.loads(f.to_json())
        assert "tick_number" in j
        assert "content_hash" in j
        assert "signature" in j
        assert "tbid" in j
        assert "echo" in j
        assert "tbn" in j


class TestPyCalendar:
    """Test the Calendar Python binding."""

    def test_repr(self):
        tf = PyTimeFamily(tbn="my-cal")
        cal = tf.get_calendar()
        assert "my-cal" in repr(cal)

    def test_to_json(self):
        tf = PyTimeFamily(tbn="my-cal")
        tf.stamp(b"data")
        cal = tf.get_calendar()
        j = json.loads(cal.to_json())
        assert "tbid" in j
        assert "tbn" in j
        assert "ticks" in j
        assert len(j["ticks"]) == 1

    def test_latest_tick_empty(self):
        tf = PyTimeFamily()
        cal = tf.get_calendar()
        assert cal.latest_tick() is None
        assert cal.tick_count() == 0
