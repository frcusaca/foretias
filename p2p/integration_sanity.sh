#!/usr/bin/env bash
# ── Foretias Cross-Language Integration Sanity Script ─────────────
# Spawns servers in Rust, Python, Java; stamps on random servers;
# verifies on different servers. Cross-language roundtrips.
#
# Prerequisites:
#   foretias binary (Rust debug build)
#   foretias CLI (Python, via foretias_p2p module)
#   foretias.Cli + libforetias_java.so (Java)
#
# Usage: ./integration_sanity.sh [--n-peers 5] [--rounds 10] [--interval 3]
#
set -euo pipefail

# ── Paths ──────────────────────────────────────────────────────────
P2P="/home/hcbusy/webhash/foretias/p2p"
RUST_BIN="$P2P/target/debug/foretias"
JAVA_CP="target/classes:target/test-classes:/home/hcbusy/.m2/repository/com/google/code/gson/gson/2.10.1/gson-2.10.1.jar"
JAVA_LIB="$P2P/target/release"

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
SERVER_TYPES=()
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
echo " Foretias Cross-Language Integration Sanity"
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
SERVER_TYPES+=("rust")
sleep 2
echo "    PID=$!  addr=127.0.0.1:$RPC_BASE"

# Peer servers — mix of Rust, Python, Java
TYPES=("rust" "python" "rust" "java" "python")
for ((i = 1; i < N_PEERS; i++)); do
    PORT=$((RPC_BASE + i))
    TYPE="${TYPES[$((i % ${#TYPES[@]}))]}"
    echo "[${i}/$((N_PEERS-1))] Starting peer #$i ($TYPE) on :$PORT ..."

    case "$TYPE" in
        rust)
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
            ;;
        python)
            PERSIST="$STAMPS_DIR/peer_$i"
            mkdir -p "$PERSIST"
            PYTHONPATH="$P2P/foretias-python" \
            foretias serve \
                --addr "127.0.0.1:$PORT" \
                --persist-path "$PERSIST" \
                --known-servers "127.0.0.1:$RPC_BASE" \
                --chronon-ns 6000000000 \
                > "$STAMPS_DIR/peer_$i.log" 2>&1 &
            PIDS+=($!)
            ;;
        java)
            # Java CLI is self-contained — no server process needed for stamp/verify
            # We still track it as a "peer" for stamp/verify selection
            ;;
    esac
    SERVER_PORTS+=("$PORT")
    SERVER_TYPES+=("$TYPE")
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

stamp_python() {
    local port="$1" msg="$2"
    PYTHONPATH="$P2P/foretias-python" \
    foretias stamp --message "$msg" --server "127.0.0.1:$port" 2>/dev/null | head -1 || echo ""
}

verify_python() {
    local port="$1" msg="$2" foretis_json="$3"
    PYTHONPATH="$P2P/foretias-python" \
    foretias verify --message "$msg" --foretis "$foretis_json" --server "127.0.0.1:$port" 2>/dev/null | head -1 || echo ""
}

stamp_java() {
    local msg="$1"
    cd "$P2P/foretias-java" && LD_LIBRARY_PATH="$JAVA_LIB" \
    java --enable-native-access=ALL-UNNAMED -cp "$JAVA_CP" \
        foretias.Cli stamp --message "$msg" 2>/dev/null | grep '^{' || echo ""
}

verify_java() {
    local msg="$1" foretis_json="$2" pubkey="$3"
    cd "$P2P/foretias-java" && LD_LIBRARY_PATH="$JAVA_LIB" \
    java --enable-native-access=ALL-UNNAMED -cp "$JAVA_CP" \
        foretias.Cli verify --message "$msg" --foretis "$foretis_json" --pubkey "$pubkey" 2>/dev/null | grep '^{' || echo ""
}

extract_pubkey_from_stamp() {
    local foretis_json="$1"
    echo "$foretis_json" | grep -oP '"pubkey":"([^"]*)"' | head -1 | cut -d'"' -f4
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

    STAMPER_TYPE="${SERVER_TYPES[$STAMPER_IDX]}"
    STAMPER_PORT="${SERVER_PORTS[$STAMPER_IDX]}"
    VERIFIER_TYPE="${SERVER_TYPES[$VERIFIER_IDX]}"
    VERIFIER_PORT="${SERVER_PORTS[$VERIFIER_IDX]}"

    MSG="round-$round-$(date +%s)-${RANDOM}"

    echo "  Stamp:  $STAMPER_TYPE :$STAMPER_PORT"
    echo "  Verify: $VERIFIER_TYPE :$VERIFIER_PORT"
    echo "  Msg:    $MSG"

    # ── Stamp ─────────────────────────────────────────────────
    STAMP_JSON=""
    case "$STAMPER_TYPE" in
        rust)
            STAMP_JSON=$(stamp_rust "$STAMPER_PORT" "$MSG")
            ;;
        python)
            STAMP_JSON=$(stamp_python "$STAMPER_PORT" "$MSG")
            ;;
        java)
            STAMP_JSON=$(stamp_java "$MSG")
            ;;
    esac

    if [ -z "$STAMP_JSON" ]; then
        fail "stamp" "$STAMPER_TYPE did not return JSON"
        echo ""
        continue
    fi

    # Extract content_hash for verification
    CONTENT_HASH=$(echo "$STAMP_JSON" | grep -oP '"content_hash":"([^"]*)"' | cut -d'"' -f4 || echo "")
    ok "stamp"

    # ── Verify ────────────────────────────────────────────────
    VERIFY_JSON=""
    case "$VERIFIER_TYPE" in
        rust)
            VERIFY_JSON=$(verify_rust "$VERIFIER_PORT" "$MSG" "$STAMP_JSON")
            ;;
        python)
            VERIFY_JSON=$(verify_python "$VERIFIER_PORT" "$MSG" "$STAMP_JSON")
            ;;
        java)
            PUBKEY=$(extract_pubkey_from_stamp "$STAMP_JSON")
            if [ -z "$PUBKEY" ]; then
                fail "verify" "cannot extract pubkey from Java stamp for verification"
                echo ""
                continue
            fi
            VERIFY_JSON=$(verify_java "$MSG" "$STAMP_JSON" "$PUBKEY")
            ;;
    esac

    if [ -z "$VERIFY_JSON" ]; then
        fail "verify" "$VERIFIER_TYPE did not return JSON"
        echo ""
        continue
    fi

    VALID=$(echo "$VERIFY_JSON" | grep -oP '"valid":\s*(true|false)' | head -1 | awk '{print $2}')
    if [ "$VALID" = "true" ]; then
        ok "verify ($STAMPER_TYPE→$VERIFIER_TYPE)"
    else
        fail "verify ($STAMPER_TYPE→$VERIFIER_TYPE)" "got valid=false"
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
