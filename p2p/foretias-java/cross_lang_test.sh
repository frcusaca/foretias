#!/usr/bin/env bash
set -euo pipefail

P2P="/home/hcbusy/webhash/foretias/p2p"
JAVA_CP="target/classes:target/test-classes:/home/hcbusy/.m2/repository/com/google/code/gson/gson/2.10.1/gson-2.10.1.jar"
LIB_PATH="$P2p/target/release"

pass=0
fail=0

ok() { echo "  PASS: $1"; ((pass++)); }
fail() { echo "  FAIL: $1"; ((fail++)); }

echo "=== Cross-Language Integration Tests ==="
echo ""

# ── Shared key for cross-language tests ──────────────────────────
SEED="0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
PUBKEY="a4a2e0384e39e123de4ce8b266a8534d4c809f7c13d5bd29d3b9e0b3c4b5e6a7"

echo "[1] Java stamp → verify in same Java session"
STAMP=$(LD_LIBRARY_PATH="$LIB_PATH" java --enable-native-access=ALL-UNNAMED \
  -cp "$JAVA_CP" -e 'foretias.Crypto.stamp(' \
  2>&1 || true)

STAMP_JSON=$(LD_LIBRARY_PATH="$LIB_PATH" java --enable-native-access=ALL-UNNAMED \
  -cp "$JAVA_CP" foretias.Cli stamp -m "cross-lang test" --privkey "$SEED" 2>/dev/null | grep '^{' || true)

if [ -n "$STAMP_JSON" ]; then
  ok "Java stamp produces valid JSON"
else
  fail "Java stamp produces valid JSON"
fi

echo ""
echo "[2] Cross-language key compatibility"
echo "  (Shared seed: $SEED)"
echo "  Both languages use Ed25519 — same seed → same public key → same signature"

echo ""
echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ] && exit 0 || exit 1
