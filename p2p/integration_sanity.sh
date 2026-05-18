#!/usr/bin/env bash
# ── Foretias Integration Sanity Script ─────────────────────────────
# Spawns servers in Rust; stamps on random servers;
# verifies on different servers.
#
# Scope reduction: Python/Java bindings removed from active scope.
# See foretias/specs/SCOPE_REDUCTION_SPEC.md for details.
#
# Prerequisites:
#   foretias binary (Rust debug build)
#
# Usage: ./integration_sanity.sh [--n-peers 5] [--rounds 10] [--interval 3]
#
set -euo pipefail

# ── Paths ──────────────────────────────────────────────────────────
P2P="/home/hcbusy/webhash/foretias/p2p"
RUST_BIN="$P2P/target/debug/foretias"

# ── Config ─────────────────────────────────────────────────────────
N_PEERS=5
ROUNDS=10
INTERVAL=3
RPC_BASE=4100

for arg in "$@"; do
    case "$arg" in
        --n-peers)  N_PEERS=$(arg_at "$@");;
        --rounds)   ROUNDS=$(arg_at "$@");;
        --interval) INTERVAL=$(arg_at "$@");;
    esac
done

arg_at() {
    local found=0
    for a in "$@"; do
        if [ "$found" -eq 1 ]; then echo "$a"; return; fi
        [ "$a" = "$1" ] && found=1
    done
}

# ── State ──────────────────────────────────────────────────────────
PIDS=()
SERVER_PORTS=()
PASS=0
FAIL=0
STAMPS_DIR=$(mktemp -d)

trap 'cleanup' EXIT
cleanup() {
    echo ""
    echo "═══════════════════════════════════════════════════════"
    echo " Shutting down servers..."
    for pid in "${PIDS[@]}"; do
        kill "$pid" 2>/dev/null || true
    done
    wait 2>/dev/null || true
    rm -rf "$STAMPS_DIR"
    echo " Done."
}

ok() { echo "  ✓ PASS: $1"; ((PASS++)) || true; }
fail() { echo "  ✗ FAIL: $1 — $2"; ((FAIL++)) || true; }

# ── Server Spawning ───────────────────────────────────────────────
echo "═══════════════════════════════════════════════════════"
echo " Foretias Integration Sanity (Rust only)"
echo " Peers: $N_PEERS | Rounds: $ROUNDS | Interval: ${INTERVAL}s"
echo "═══════════════════════════════════════════════════════"
echo ""

# Bootstrap node (Rust) on port 4100
echo "[1/...] Starting bootstrap node (Rust) on :$((RPC_BASE)) ..."
RUST_PERSIST="$STAMPS_DIR/bootstrap"
mkdir -p "$RUST_PERSIST"
"$RUST_BIN" serve \
    --addr "127.0.0.1:$RPC_BASE" \
    --p2p-port-range "9900..9999" \
    --persist-path "$RUST_PERSIST" \
    --chronon-ns 6000000000 \
    > "$STAMPS_DIR/bootstrap.log" 2>&1 &
PIDS+=($!)
SERVER_PORTS+=("$RPC_BASE")
sleep 2
echo "    PID=$!  addr=127.0.0.1:$RPC_BASE"

# Peer servers — all Rust
for ((i = 1; i < N_PEERS; i++)); do
    PORT=$((RPC_BASE + i))
    echo "[${i}/$((N_PEERS-1))] Starting peer #$i (Rust) on :$PORT ..."

    PERSIST="$STAMPS_DIR/peer_$i"
    mkdir -p "$PERSIST"
    "$RUST_BIN" serve \
        --addr "127.0.0.1:$PORT" \
        --p2p-port-range "9900..9999" \
        --persist-path "$PERSIST" \
        --known-servers "127.0.0.1:$RPC_BASE" \
        --chronon-ns 6000000000 \
        > "$STAMPS_DIR/peer_$i.log" 2>&1 &
    PIDS+=($!)
    SERVER_PORTS+=("$PORT")
    sleep 1
done

echo ""
echo "═══════════════════════════════════════════════════════"
echo " Servers started. Waiting ${INTERVAL}s for initialization..."
sleep "$INTERVAL"

# ── Helper Functions ───────────────────────────────────────────────

stamp_rust() {
    local port="$1" msg="$2"
    "$RUST_BIN" stamp --message "$msg" --server "127.0.0.1:$port" 2>/dev/null | grep '^{' || echo ""
}

verify_rust() {
    local port="$1" msg="$2" foretis_json="$3"
    "$RUST_BIN" verify --message "$msg" --foretis "$foretis_json" --server "127.0.0.1:$port" 2>/dev/null | grep '^{' || echo ""
}

random_server_index() {
    echo $((RANDOM % ${#SERVER_PORTS[@]}))
}

# ── Integration Rounds ─────────────────────────────────────────────
echo "═══════════════════════════════════════════════════════"
echo " Running $ROUNDS integration rounds..."
echo "═══════════════════════════════════════════════════════"
echo ""

for ((round = 1; round <= ROUNDS; round++)); do
    echo "--- Round $round / $ROUNDS ---"

    STAMPER_IDX=$(random_server_index)
    VERIFIER_IDX=$(random_server_index)

    while [ "$VERIFIER_IDX" -eq "$STAMPER_IDX" ]; do
        VERIFIER_IDX=$(random_server_index)
    done

    STAMPER_PORT="${SERVER_PORTS[$STAMPER_IDX]}"
    VERIFIER_PORT="${SERVER_PORTS[$VERIFIER_IDX]}"

    MSG="round-$round-$(date +%s)-${RANDOM}"

    echo "  Stamp:  Rust :$STAMPER_PORT"
    echo "  Verify: Rust :$VERIFIER_PORT"
    echo "  Msg:    $MSG"

    # ── Stamp ─────────────────────────────────────────────────
    STAMP_JSON=$(stamp_rust "$STAMPER_PORT" "$MSG")

    if [ -z "$STAMP_JSON" ]; then
        fail "stamp" "Rust did not return JSON"
        echo ""
        continue
    fi

    # Extract content_hash for verification
    CONTENT_HASH=$(echo "$STAMP_JSON" | grep -oP '"content_hash":"([^"]*)"' | cut -d'"' -f4 || echo "")
    ok "stamp"

    # ── Verify ────────────────────────────────────────────────
    VERIFY_JSON=$(verify_rust "$VERIFIER_PORT" "$MSG" "$STAMP_JSON")

    if [ -z "$VERIFY_JSON" ]; then
        fail "verify" "Rust did not return JSON"
        echo ""
        continue
    fi

    VALID=$(echo "$VERIFY_JSON" | grep -oP '"valid":\s*(true|false)' | head -1 | awk '{print $2}')
    if [ "$VALID" = "true" ]; then
        ok "verify (Rust→Rust)"
    else
        fail "verify (Rust→Rust)" "got valid=false"
    fi

    echo ""
    [ "$round" -lt "$ROUNDS" ] && sleep "$INTERVAL"
done

# ── Final Report ───────────────────────────────────────────────────
echo "═══════════════════════════════════════════════════════"
TOTAL=$((PASS + FAIL))
echo " RESULTS: $PASS/$TOTAL passed, $FAIL/$TOTAL failed"
echo "═══════════════════════════════════════════════════════"

[ "$FAIL" -eq 0 ] && exit 0 || exit 1
