"""Cross-validation tests: verify Rust and Python paths produce identical results.

Exercises the PyO3 bindings (PyTimeFamily, PyCryptoServer, PyForetis) to ensure
the Rust stamp/verify pipeline is deterministic and consistent across code paths.

Run with: python -m pytest integration-tests/ -v
"""

import json
import pytest

from foretias_p2p import PyTimeFamily, PyForetis, PyCryptoServer


class TestCrossValidation:
    """Cross-validation between PyTimeFamily and PyCryptoServer code paths."""

    def test_stamp_produces_identical_hash(self):
        """Stamp 'hello' via PyTimeFamily, compute sha256 via PyCryptoServer, assert same hash.

        The content_hash embedded in the Foretis stamp MUST be the SHA-256 of the
        original content. We verify this independently using PyCryptoServer.sha256().
        """
        content = b"hello"
        tf = PyTimeFamily(tbn="cross-val-hash")
        cs = PyCryptoServer("ed25519")

        foretis = tf.stamp(content)
        expected_hash = cs.sha256(content)

        assert foretis.content_hash == expected_hash, (
            f"PyTimeFamily hash {foretis.content_hash.hex()} != "
            f"PyCryptoServer hash {expected_hash.hex()}"
        )

    def test_stamp_verify_roundtrip(self):
        """Stamp via PyTimeFamily, verify via the same instance, assert True.

        The canonical roundtrip: stamp content, then verify the same content
        against the returned Foretis. Must return True. Verifying against
        different content must return False.
        """
        content = b"roundtrip content"
        tf = PyTimeFamily(tbn="cross-val-roundtrip")

        foretis = tf.stamp(content)
        assert tf.verify(content, foretis) is True
        assert tf.verify(b"different content", foretis) is False

    def test_foretis_json_roundtrip(self):
        """Stamp, serialize to JSON, deserialize with from_json, verify still valid.

        Tests that PyForetis.to_json() and PyForetis.from_json() are lossless
        with respect to cryptographic verification.
        """
        content = b"json roundtrip payload"
        tf = PyTimeFamily(tbn="cross-val-json")

        foretis = tf.stamp(content)
        json_str = foretis.to_json()

        # Parse JSON to verify structure
        parsed = json.loads(json_str)
        assert "tick_number" in parsed
        assert "content_hash" in parsed
        assert "signature" in parsed

        # Deserialize back to PyForetis
        foretis_restored = PyForetis.from_json(json_str)

        # Verification must still succeed after roundtrip
        assert tf.verify(content, foretis_restored) is True

    def test_multi_tick_integrity(self):
        """Create 10 ticks, verify all stamps from tick 0-9 are valid.

        Stamps 10 messages, collects all Foretis records, then verifies each
        one independently. Tick numbers must be sequential (1-10).
        """
        tf = PyTimeFamily(tbn="cross-val-multi")
        content_list = [f"message-{i}".encode() for i in range(10)]
        foretis_list = []

        for content in content_list:
            foretis = tf.stamp(content)
            foretis_list.append((content, foretis))

        # Verify tick numbers are sequential
        for i, (_, foretis) in enumerate(foretis_list):
            assert foretis.tick_number == i + 1, (
                f"Expected tick {i + 1}, got {foretis.tick_number}"
            )

        # Verify all stamps
        for content, foretis in foretis_list:
            assert tf.verify(content, foretis) is True, (
                f"Verification failed for tick {foretis.tick_number}"
            )

        # Latest tick should be 10
        assert tf.get_latest_tick() == 10
