"""Functional tests for fortias._timebeing pure functions."""

from __future__ import annotations

import pytest

from fortias._timebeing import _timebeing
from fortias.crypto import generate_keypair, sign
from fortias.models import Fortis, TickRecord


def _make_genesis():
    """Create a genesis tick record with self-transition MA."""
    tbid = b"test-tbid-123456789012345678901234"  # 32 bytes
    sk, pk = generate_keypair()
    genesis_ma = (
        tbid
        + b"\x00" * 8
        + pk
        + b"\x00" * 8
        + pk
    )
    genesis_forward = sign(genesis_ma, sk)
    genesis_backward = sign(genesis_ma, sk)
    genesis = TickRecord(
        tick_number=0, public_key=pk,
        forward_fortis=genesis_forward,
        backward_fortis=genesis_backward,
    )
    return tbid, genesis, pk, sk


class TestStamp:
    def test_stamp_returns_fortis(self):
        tbid, genesis, pk, sk = _make_genesis()
        fortis = _timebeing._stamp(
            content=b"hello",
            tbid=tbid,
            tick_number=0,
            private_key=sk,
        )
        assert fortis.tick_number == 0
        assert fortis.tbid == tbid
        assert fortis.echo == ""
        assert len(fortis.my_content_hash) == 32
        assert len(fortis.signature) == 64

    def test_stamp_string_content(self):
        tbid, genesis, pk, sk = _make_genesis()
        fortis = _timebeing._stamp(
            content="hello",
            tbid=tbid,
            tick_number=0,
            private_key=sk,
        )
        assert fortis.tick_number == 0
        assert len(fortis.signature) == 64

    def test_stamp_different_content_different_signatures(self):
        tbid, genesis, pk, sk = _make_genesis()
        f1 = _timebeing._stamp(b"a", tbid, 0, sk)
        f2 = _timebeing._stamp(b"b", tbid, 0, sk)
        assert f1.signature != f2.signature

    def test_stamp_same_content_same_signature(self):
        tbid, genesis, pk, sk = _make_genesis()
        f1 = _timebeing._stamp(b"same", tbid, 0, sk)
        f2 = _timebeing._stamp(b"same", tbid, 0, sk)
        assert f1.signature == f2.signature
        assert f1.my_content_hash == f2.my_content_hash

    def test_stamp_echo_and_tbn(self):
        tbid, genesis, pk, sk = _make_genesis()
        fortis = _timebeing._stamp(
            content=b"msg", tbid=tbid, tick_number=0,
            private_key=sk, echo="my-echo", tbn="Time Being test",
        )
        assert fortis.echo == "my-echo"
        assert fortis.tbn == "Time Being test"


class TestTickTransition:
    def test_tick_generates_new_keypair(self):
        tbid, genesis, pk, sk = _make_genesis()
        new_record, new_sk = _timebeing._tick(tbid, genesis, sk)
        assert new_record.tick_number > 0
        assert new_record.public_key != pk
        assert len(new_record.public_key) == 32
        assert len(new_sk) == 32

    def test_tick_creates_forward_fortis(self):
        tbid, genesis, pk, sk = _make_genesis()
        new_record, new_sk = _timebeing._tick(tbid, genesis, sk)
        assert len(new_record.forward_fortis) == 64

    def test_tick_creates_backward_fortis(self):
        tbid, genesis, pk, sk = _make_genesis()
        new_record, new_sk = _timebeing._tick(tbid, genesis, sk)
        assert new_record.backward_fortis is not None
        assert len(new_record.backward_fortis) == 64

    def test_tick_tick_number_increases(self):
        tbid, genesis, pk, sk = _make_genesis()
        new_record, _ = _timebeing._tick(tbid, genesis, sk)
        assert new_record.tick_number > genesis.tick_number


class TestVerifyPair:
    def test_valid_pair(self):
        tbid, genesis, pk, sk = _make_genesis()
        new_record, new_sk = _timebeing._tick(tbid, genesis, sk)
        assert _timebeing._verify_pair(new_record, genesis, tbid) is True

    def test_mismatched_pair_fails(self):
        tbid = b"test-tbid-123456789012345678901234"
        sk1, pk1 = generate_keypair()
        sk2, pk2 = generate_keypair()
        r1 = TickRecord(100, pk1, b"sign1", b"sign2")
        r2 = TickRecord(200, pk2, b"sign2", b"sign3")
        assert _timebeing._verify_pair(r2, r1, tbid) is False

    def test_backward_fortis_required(self):
        """backward_fortis must never be None."""
        sk, pk = generate_keypair()
        with pytest.raises(TypeError):
            TickRecord(tick_number=1, public_key=pk, forward_fortis=b"sig", backward_fortis=None)

    def test_forward_fortis_required(self):
        """forward_fortis must never be None."""
        sk, pk = generate_keypair()
        with pytest.raises(TypeError):
            TickRecord(tick_number=1, public_key=pk, forward_fortis=None, backward_fortis=b"sig")

    def test_forward_fortis_verified_with_old_key(self):
        """forward_fortis (signed by old key during _tick) verifies against old key."""
        tbid, genesis, pk, sk = _make_genesis()
        new_record, new_sk = _timebeing._tick(tbid, genesis, sk)
        # B.forward_fortis was signed by old key, verifies against old key
        from fortias.crypto import verify
        assert verify(
            _mutual_ack(genesis, new_record),
            new_record.forward_fortis,
            genesis.public_key,
        ) is True

    def test_backward_fortis_verified_with_new_key(self):
        """backward_fortis (signed by new key during _tick) verifies against new key."""
        tbid, genesis, pk, sk = _make_genesis()
        new_record, new_sk = _timebeing._tick(tbid, genesis, sk)
        # B.backward_fortis was signed by new key, verifies against new key
        from fortias.crypto import verify
        assert verify(
            _mutual_ack(genesis, new_record),
            new_record.backward_fortis,
            new_record.public_key,
        ) is True


def _mutual_ack(A, B, tbid=b"test-tbid-123456789012345678901234"):
    """Build mutual_acknowledgement for two tick records."""
    return (
        tbid
        + A.tick_number.to_bytes(8, "big")
        + A.public_key
        + B.tick_number.to_bytes(8, "big")
        + B.public_key
    )


class TestVerifyChain:
    def test_single_record_chain(self):
        tbid, genesis, pk, sk = _make_genesis()
        assert _timebeing._verify_chain([genesis], tbid) is True

    def test_two_record_chain(self):
        tbid, genesis, pk, sk = _make_genesis()
        new_record, _ = _timebeing._tick(tbid, genesis, sk)
        assert _timebeing._verify_chain([genesis, new_record], tbid) is True

    def test_three_record_chain(self):
        tbid, genesis, pk, sk = _make_genesis()
        r2, sk2 = _timebeing._tick(tbid, genesis, sk)
        # _tick returns (record, new_private_key)
        r3, _ = _timebeing._tick(tbid, r2, sk2)
        assert _timebeing._verify_chain([genesis, r2, r3], tbid) is True

    def test_broken_chain_fails(self):
        tbid = b"test-tbid-123456789012345678901234"
        sk1, pk1 = generate_keypair()
        sk2, pk2 = generate_keypair()
        r1 = TickRecord(100, pk1, b"fake1", b"fake2")
        r2 = TickRecord(200, pk2, b"fake2", b"fake3")
        assert _timebeing._verify_chain([r1, r2], tbid) is False

    def test_return_failures(self):
        tbid, genesis, pk, sk = _make_genesis()
        r2, sk2 = _timebeing._tick(tbid, genesis, sk)
        _, broken_pk = generate_keypair()
        broken = TickRecord(r2.tick_number + 1, broken_pk, b"bad", b"bad")
        ok, failures = _timebeing._verify_chain([genesis, r2, broken], tbid, return_failures=True)
        assert ok is False
        assert 1 in failures

    def test_empty_chain(self):
        assert _timebeing._verify_chain([], b"tbid") is True


class TestVerify:
    def test_valid_fortis(self):
        tbid, genesis, pk, sk = _make_genesis()
        fortis = _timebeing._stamp(b"hello", tbid, 0, sk)
        result = _timebeing._verify(b"hello", fortis, [genesis])
        assert result is True

    def test_valid_fortis_string_content(self):
        tbid, genesis, pk, sk = _make_genesis()
        fortis = _timebeing._stamp("hello", tbid, 0, sk)
        result = _timebeing._verify("hello", fortis, [genesis])
        assert result is True

    def test_wrong_content_fails(self):
        tbid, genesis, pk, sk = _make_genesis()
        fortis = _timebeing._stamp(b"hello", tbid, 0, sk)
        result = _timebeing._verify(b"world", fortis, [genesis])
        assert result is False

    def test_wrong_key_fails(self):
        tbid, genesis, pk, sk = _make_genesis()
        other_sk, other_pk = generate_keypair()
        fortis = _timebeing._stamp(b"hello", tbid, 0, other_sk)
        result = _timebeing._verify(b"hello", fortis, [genesis])
        assert result is False

    def test_verify_with_next_tick_none_when_active(self):
        tbid, genesis, pk, sk = _make_genesis()
        fortis = _timebeing._stamp(b"hello", tbid, 0, sk)
        result = _timebeing._verify(b"hello", fortis, [genesis], next_tick_number=1)
        sig_valid, window_closed = result
        assert sig_valid is True
        assert window_closed is False

    def test_verify_with_next_tick_exists(self):
        tbid, genesis, pk, sk = _make_genesis()
        fortis = _timebeing._stamp(b"hello", tbid, 0, sk)
        new_record, _ = _timebeing._tick(tbid, genesis, sk)
        result = _timebeing._verify(
            b"hello", fortis, [genesis, new_record],
            next_tick_number=new_record.tick_number,
        )
        sig_valid, window_closed = result
        assert sig_valid is True
        assert window_closed is True

    def test_verify_with_invalid_sig_returns_none_window(self):
        tbid, genesis, pk, sk = _make_genesis()
        fortis = _timebeing._stamp(b"hello", tbid, 0, sk)
        tampered = Fortis(
            tick_number=fortis.tick_number,
            my_content_hash=fortis.my_content_hash,
            signature=b"\x00" * 64,
            tbid=fortis.tbid,
            echo=fortis.echo,
            tbn=fortis.tbn,
        )
        result = _timebeing._verify(b"hello", tampered, [genesis], next_tick_number=1)
        sig_valid, window_closed = result
        assert sig_valid is False
        assert window_closed is None

    def test_verify_missing_tick(self):
        tbid, genesis, pk, sk = _make_genesis()
        fortis = _timebeing._stamp(b"hello", tbid, 999, sk)
        result = _timebeing._verify(b"hello", fortis, [genesis])
        assert result is False
