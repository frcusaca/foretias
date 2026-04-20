"""Unit tests for fortias.models."""

from __future__ import annotations

from dataclasses import FrozenInstanceError

import pytest

from fortias.models import Fortis, TickRecord


class TestTickRecordFrozen:
    def test_immutable(self):
        r = TickRecord(0, b"pk", b"ff", b"bf")
        with pytest.raises(FrozenInstanceError):
            r.tick_number = 1  # type: ignore[misc]

    def test_equality(self):
        a = TickRecord(1, b"pk", b"ff", b"bf")
        b = TickRecord(1, b"pk", b"ff", b"bf")
        c = TickRecord(2, b"pk", b"ff", b"bf")
        assert a == b
        assert a != c

    def test_hashable(self):
        a = TickRecord(1, b"pk", b"ff", b"bf")
        b = TickRecord(1, b"pk", b"ff", b"bf")
        assert hash(a) == hash(b)

    def test_backward_fortis_none_raises(self):
        with pytest.raises(TypeError):
            TickRecord(0, b"pk", b"ff", None)

    def test_regular_record_with_backward_fortis(self):
        r = TickRecord(1, b"pk", b"ff", b"bf")
        assert r.backward_fortis == b"bf"


class TestFortisFrozen:
    def test_immutable(self):
        f = Fortis(0, b"hash", b"sig", b"tbid", "echo", "tbn")
        with pytest.raises(FrozenInstanceError):
            f.tick_number = 1  # type: ignore[misc]

    def test_equality(self):
        a = Fortis(0, b"hash", b"sig", b"tbid", "echo", "tbn")
        b = Fortis(0, b"hash", b"sig", b"tbid", "echo", "tbn")
        assert a == b

    def test_hashable(self):
        a = Fortis(0, b"hash", b"sig", b"tbid", "echo", "tbn")
        b = Fortis(0, b"hash", b"sig", b"tbid", "echo", "tbn")
        assert hash(a) == hash(b)

    def test_default_echo(self):
        f = Fortis(0, b"hash", b"sig", b"tbid", "", "tbn")
        assert f.echo == ""

    def test_fields_populated(self):
        f = Fortis(42, b"abc", b"def", b"tbid123", "hello", "Time Being abc")
        assert f.tick_number == 42
        assert f.my_content_hash == b"abc"
        assert f.signature == b"def"
        assert f.tbid == b"tbid123"
        assert f.echo == "hello"
        assert f.tbn == "Time Being abc"
