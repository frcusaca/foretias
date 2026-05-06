#!/usr/bin/env bash
# ── Foretias DHT Stress Test ────────────────────────────────────────
# Spawns 1 bootstrap node + N peers with auto-port selection. The
# bootstrap node is NOT more trusted — it's just the entry point for
# DHT discovery. All peers are equal once connected.
# Then runs cross-node stamp/verify rounds exercising the new TBID
# index DHT path, and mirror_request rounds between peer pairs.
#
# Prerequisites:
#   foretias binary (Rust debug build at target/debug/foretias)
#
# Usage: ./dht_stress_test.sh [--n-peers 100] [--rounds 50] [--interval 3] [--timeout 60] [--mirror-rounds 5]
#
set -euo pipefail

# ── Paths ──────────────────────────────────────────────────────────
P2P="/home/hcbusy/webhash/foretias/p2p"
RUST_BIN="$P2P/target/debug/foretias"

# ── Config ─────────────────────────────────────────────────────────
N_PEERS=100
ROUNDS=50
INTERVAL=3
TIMEOUT=60
MIRROR_ROUNDS=5
BOOTSTRAP_PORT=4100

# Parse args
while [[ $# -gt 0 ]]; do
    case "$1" in
        --n-peers)      N_PEERS="$2"; shift 2;;
        --rounds)       ROUNDS="$2"; shift 2;;
        --interval)     INTERVAL="$2"; shift 2;;
        --timeout)      TIMEOUT="$2"; shift 2;;
        --mirror-rounds) MIRROR_ROUNDS="$2"; shift 2;;
        *) echo "Unknown arg: $1"; exit 1;;
    esac
done

# ── State ──────────────────────────────────────────────────────────
PIDS=()
PEER_PORTS=()
PEER_TBIDS=()
PEER_LOG_DIRS=()
PASS=0
FAIL=0
STAMPS_DIR=$(mktemp -d -t foretias.XXXXXX)
BOOTSTRAP_PERSIST="$STAMPS_DIR/bootstrap"
mkdir -p "$BOOTSTRAP_PERSIST"

trap 'cleanup' EXIT INT TERM
cleanup() {
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo " Shutting down ${#PIDS[@]} servers..."
    for pid in "${PIDS[@]}"; do
        kill "$pid" 2>/dev/null || true
    done
    wait 2>/dev/null || true
    rm -rf "$STAMPS_DIR"
    echo " Done."
}

ok()  { echo "  ✓ $1"; ((PASS++)) || true; }
fail() { echo "  ✗ $1 — $2"; ((FAIL++)) || true; }

# ── Helpers ────────────────────────────────────────────────────────
random_server_index() {
    echo $(( RANDOM % ${#PEER_PORTS[@]} ))
}

extract_tbid_from_log() {
    local log_file="$1"
    grep -oP 'TBID\s*:\s*\K[0-9a-f]+' "$log_file" | head -1
}

extract_port_from_log() {
    local log_file="$1"
    grep -oP 'listening on \S+:\K[0-9]+' "$log_file" | head -1
}

json_rpc_call() {
    local port="$1" method="$2" params="$3"
    local body="{\"jsonrpc\":\"2.0\",\"method\":\"${method}\",\"params\":${params},\"id\":1}"
    # Use python3 for reliable TCP (socat may not be installed)
    python3 -c "
import socket, sys
s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
s.settimeout(3)
try:
    s.connect(('127.0.0.1', $port))
    s.sendall(sys.argv[1].encode())
    s.shutdown(socket.SHUT_WR)
    resp = b''
    while True:
        chunk = s.recv(4096)
        if not chunk:
            break
        resp += chunk
        if b'\n' in resp:
            break
    print(resp.decode().strip())
except Exception as e:
    pass
" "$body" 2>/dev/null | head -1
}

# ── Main ───────────────────────────────────────────────────────────
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo " Foretias DHT Stress Test"
echo " Peers: $N_PEERS | Rounds: $ROUNDS | Mirror rounds: $MIRROR_ROUNDS | Timeout: ${TIMEOUT}s"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

if [ ! -x "$RUST_BIN" ]; then
    echo "ERROR: foretias binary not found at $RUST_BIN"
    echo "Run: cd $P2P && cargo build"
    exit 1
fi

# ── Step 1: Start Trust Server ─────────────────────────────────────
echo "[1] Starting bootstrap node on :$BOOTSTRAP_PORT ..."
"$RUST_BIN" serve \
    --addr "127.0.0.1:$BOOTSTRAP_PORT" \
    --persist-path "$BOOTSTRAP_PERSIST" \
    --chronon-ns 6000000000 \
    --p2p-listen "/ip4/0.0.0.0/tcp/9900" \
    --dht-namespace "stress-test" \
    > "$STAMPS_DIR/bootstrap.log" 2>&1 &
PIDS+=($!)
sleep 2

BOOTSTRAP_TPID=$(grep -oP 'TBID\s*:\s*\K[0-9a-f]+' "$STAMPS_DIR/bootstrap.log" 2>/dev/null | head -n1 || echo "")
if [ -z "$BOOTSTRAP_TPID" ]; then
    echo "WARNING: Could not extract TBID from bootstrap node log"
    echo "  Log: $(tail -5 "$STAMPS_DIR/bootstrap.log")"
fi
echo "    PID=${PIDS[0]}, TBID=$BOOTSTRAP_TPID"
echo ""

# ── Step 2: Start Peer Servers ────────────────────────────────────
echo "[2] Starting $N_PEERS peer servers (auto-port, bootstrap=$BOOTSTRAP_PORT) ..."
BATCH_SIZE=10
BATCH_NUM=0
START_TIME=$(date +%s)

# Create per-peer log directories
FORETIAS_LOG_BASE="$HOME/.foretias/logs"
mkdir -p "$FORETIAS_LOG_BASE"

for ((i = 1; i <= N_PEERS; i++)); do
    PERSIST="$STAMPS_DIR/peer_$i"
    mkdir -p "$PERSIST"

    # Per-peer log directory
    PEER_LOG_DIR="$FORETIAS_LOG_BASE/peer_$i"
    mkdir -p "$PEER_LOG_DIR"
    PEER_LOG_DIRS+=("$PEER_LOG_DIR")

    "$RUST_BIN" serve \
        --addr "127.0.0.1:0" \
        --persist-path "$PERSIST" \
        --chronon-ns 6000000000 \
        --p2p-port-range "9910..9999" \
        --known-servers "127.0.0.1:9900" \
        --dht-namespace "stress-test" \
        --max-discovered-peers 13 \
        > "$STAMPS_DIR/peer_$i.log" 2>&1 &
    PIDS+=($!)

    # Track discovered port from log after a short delay
    sleep 0.1

    # Batch progress display
    if (( i % BATCH_SIZE == 0 )); then
        BATCH_NUM=$((BATCH_NUM + 1))
        echo "    Batch $BATCH_SIZE: $i/$N_PEERS peers spawned"
    fi
done

ELAPSED=$(( $(date +%s) - START_TIME ))
echo ""
echo "    All $N_PEERS peers spawned in ${ELAPSED}s"
echo "    Total processes: ${#PIDS[@]}"
echo "    Per-peer log dirs: $FORETIAS_LOG_BASE/"
echo ""

# ── Step 3: Wait for DHT Convergence ──────────────────────────────
echo "[3] Waiting ${TIMEOUT}s for DHT convergence ..."
sleep "$TIMEOUT"

# Extract peer ports from logs
for ((i = 1; i <= N_PEERS; i++)); do
    LOG="$STAMPS_DIR/peer_$i.log"
    if [ -f "$LOG" ]; then
        PORT=$(grep -oP 'listening on \S+:\K[0-9]+' "$LOG" 2>/dev/null | head -1 || echo "")
        TBID=$(grep -oP 'TBID\s*:\s*\K[0-9a-f]+' "$LOG" 2>/dev/null | head -1 || echo "")
        PEER_PORTS+=("$PORT")
        PEER_TBIDS+=("$TBID")
    else
        PEER_PORTS+=("0")
        PEER_TBIDS+=("")
    fi
done

# Count healthy peers (non-zero port)
HEALTHY=0
for port in "${PEER_PORTS[@]}"; do
    if [ "$port" != "0" ] && [ -n "$port" ]; then
        ((HEALTHY++)) || true
    fi
done
echo ""
echo "    Healthy peers: $HEALTHY / $N_PEERS"
echo ""

if [ "$HEALTHY" -lt 2 ]; then
    echo "ERROR: Too few healthy peers ($HEALTHY). Cannot proceed."
    echo "Check logs in $STAMPS_DIR/"
    exit 1
fi

# ── Step 4: Verify Trust Server Discovered Peers ─────────────────
echo "[4] Checking bootstrap node peer discovery via collision_status ..."
STATUS=$(json_rpc_call "$BOOTSTRAP_PORT" "collision_status" "{}")
if [ -n "$STATUS" ]; then
    echo "    Bootstrap node status: $STATUS"
else
    echo "    WARNING: Could not reach bootstrap node on :$BOOTSTRAP_PORT"
fi
echo ""

# ── Step 5: Cross-Node Stamp/Verify Rounds ────────────────────────
echo "[5] Running $ROUNDS cross-node stamp/verify rounds ..."
echo "    (stamp on random peer, verify on DIFFERENT peer via DHT TBID lookup)"
echo ""

# Build list of valid peer indices
VALID_INDICES=()
for ((i = 0; i < ${#PEER_PORTS[@]}; i++)); do
    if [ "${PEER_PORTS[$i]}" != "0" ] && [ -n "${PEER_PORTS[$i]}" ]; then
        VALID_INDICES+=("$i")
    fi
done

VALID_COUNT=${#VALID_INDICES[@]}
if [ "$VALID_COUNT" -lt 2 ]; then
    echo "ERROR: Need at least 2 valid peers for cross-node tests"
    exit 1
fi

for ((round = 1; round <= ROUNDS; round++)); do
    # Pick two different peers
    STAMPER_IDX=${VALID_INDICES[$(( RANDOM % VALID_COUNT ))]}
    VERIFIER_IDX=${VALID_INDICES[$(( RANDOM % VALID_COUNT ))]}
    while [ "$VERIFIER_IDX" -eq "$STAMPER_IDX" ]; do
        VERIFIER_IDX=${VALID_INDICES[$(( RANDOM % VALID_COUNT ))]}
    done

    STAMPER_PORT="${PEER_PORTS[$STAMPER_IDX]}"
    STAMPER_TPID="${PEER_TBIDS[$STAMPER_IDX]}"
    VERIFIER_PORT="${PEER_PORTS[$VERIFIER_IDX]}"
    VERIFIER_TPID="${PEER_TBIDS[$VERIFIER_IDX]}"

    MSG="stress-$round-$(date +%s)"

    # ── Stamp ──
    STAMP_PARAMS="{\"content\":\"$(echo -n "$MSG" | xxd -p)\",\"echo\":\"stress-test\"}"
    STAMP_JSON=$(json_rpc_call "$STAMPER_PORT" "stamp" "$STAMP_PARAMS")

    if [ -z "$STAMP_JSON" ]; then
        fail "stamp #$round" "peer $STAMPER_IDX (:$STAMPER_PORT) unreachable"
        [ "$round" -lt "$ROUNDS" ] && sleep 1
        continue
    fi

    # Extract tick_number for the verify
    TICK_NUM=$(echo "$STAMP_JSON" | grep -oP '"tick_number":\s*\K[0-9]+' || echo "")
    STAMP_TPID=$(echo "$STAMP_JSON" | grep -oP '"tbid":"\K[0-9a-f]+' || echo "")

    # ── Verify cross-node ──
    VERIFY_PARAMS="{\"content\":\"$(echo -n "$MSG" | xxd -p)\",\"foretis\":$STAMP_JSON,\"cross_node\":true}"
    VERIFY_JSON=$(json_rpc_call "$VERIFIER_PORT" "verify" "$VERIFY_PARAMS")

    if [ -z "$VERIFY_JSON" ]; then
        fail "verify #$round" "peer $VERIFIER_IDX (:$VERIFIER_PORT) unreachable"
        [ "$round" -lt "$ROUNDS" ] && sleep 1
        continue
    fi

    VALID=$(echo "$VERIFY_JSON" | grep -oP '"valid":\s*(true|false)' | awk '{print $2}' || echo "")
    METHOD=$(echo "$VERIFY_JSON" | grep -oP '"method":"\K[^"]+' || echo "")

    if [ "$VALID" = "true" ]; then
        ok "round #$round (TBID $STAMP_TPID → $VERIFIER_TPID, method=$METHOD)"
    else
        fail "round #$round" "valid=$VALID, method=$METHOD (TBID $STAMP_TPID → $VERIFIER_TPID)"
    fi

    [ "$round" -lt "$ROUNDS" ] && sleep "$INTERVAL"
done

# ── Step 6: Mirror Rounds ─────────────────────────────────────────
echo ""
echo "[6] Running $MIRROR_ROUNDS mirror_request rounds ..."
echo "    (mutual mirror_request between peer pairs, ship_batch + ship_ack + reconcile)"
echo ""

MIRROR_PASS=0
MIRROR_FAIL=0

mirror_ok()  { echo "  ✓ $1"; ((MIRROR_PASS++)) || true; }
mirror_fail() { echo "  ✗ $1 — $2"; ((MIRROR_FAIL++)) || true; }

for ((mround = 1; mround <= MIRROR_ROUNDS; mround++)); do
    # Pick two different valid peers
    IDX_A=${VALID_INDICES[$(( RANDOM % VALID_COUNT ))]}
    IDX_B=${VALID_INDICES[$(( RANDOM % VALID_COUNT ))]}
    while [ "$IDX_B" -eq "$IDX_A" ]; do
        IDX_B=${VALID_INDICES[$(( RANDOM % VALID_COUNT ))]}
    done

    PORT_A="${PEER_PORTS[$IDX_A]}"
    PORT_B="${PEER_PORTS[$IDX_B]}"
    TBID_A="${PEER_TBIDS[$IDX_A]}"
    TBID_B="${PEER_TBIDS[$IDX_B]}"

    echo "    Mirror round #$mround: peer $IDX_A (:$PORT_A, $TBID_A) ↔ peer $IDX_B (:$PORT_B, $TBID_B)"

    # ── Mirror request: A → B ──
    MIRROR_AB_PARAMS="{\"tbid\":\"${TBID_B}\",\"my_addr\":\"127.0.0.1:${PORT_A}\",\"my_tbid\":\"${TBID_A}\"}"
    MIRROR_AB=$(json_rpc_call "$PORT_A" "mirror_request" "$MIRROR_AB_PARAMS")

    # ── Mirror request: B → A (mutual) ──
    MIRROR_BA_PARAMS="{\"tbid\":\"${TBID_A}\",\"my_addr\":\"127.0.0.1:${PORT_B}\",\"my_tbid\":\"${TBID_B}\"}"
    MIRROR_BA=$(json_rpc_call "$PORT_B" "mirror_request" "$MIRROR_BA_PARAMS")

    # Check both responses have status: accept
    STATUS_AB=$(echo "$MIRROR_AB" | grep -oP '"status":\s*"\K[^"]+' || echo "")
    STATUS_BA=$(echo "$MIRROR_BA" | grep -oP '"status":\s*"\K[^"]+' || echo "")

    if [ "$STATUS_AB" != "accept" ] || [ "$STATUS_BA" != "accept" ]; then
        mirror_fail "mirror_request #$mround" "A→B=$STATUS_AB, B→A=$STATUS_BA"
        [ "$mround" -lt "$MIRROR_ROUNDS" ] && sleep 1
        continue
    fi
    mirror_ok "mirror_request #$mround (A→B=$STATUS_AB, B→A=$STATUS_BA)"

    # ── Ship batch: A requests B's calendar ──
    SHIP_PARAMS="{\"tick_start\":0,\"count\":10}"
    SHIP_JSON=$(json_rpc_call "$PORT_A" "ship_batch" "$SHIP_PARAMS")

    if [ -z "$SHIP_JSON" ]; then
        mirror_fail "ship_batch #$mround" "peer $IDX_A (:$PORT_A) unreachable"
        [ "$mround" -lt "$MIRROR_ROUNDS" ] && sleep 1
        continue
    fi

    # ── Ship ack: B acknowledges the records ──
    SHIP_ACK_PARAMS="{\"records\":$SHIP_JSON}"
    SHIP_ACK=$(json_rpc_call "$PORT_B" "ship_ack" "$SHIP_ACK_PARAMS")
    ACK_STATUS=$(echo "$SHIP_ACK" | grep -oP '"status":\s*"\K[^"]+' || echo "")
    if [ -n "$ACK_STATUS" ]; then
        mirror_ok "ship_ack #$mround (status=$ACK_STATUS)"
    else
        mirror_fail "ship_ack #$mround" "no response from peer $IDX_B (:$PORT_B)"
    fi

    # ── Mirror reconcile: both sides ──
    RECONCILE_A=$(json_rpc_call "$PORT_A" "mirror_reconcile" "{}")
    RECONCILE_B=$(json_rpc_call "$PORT_B" "mirror_reconcile" "{}")

    RECON_STATUS_A=$(echo "$RECONCILE_A" | grep -oP '"status":\s*"\K[^"]+' || echo "")
    RECON_STATUS_B=$(echo "$RECONCILE_B" | grep -oP '"status":\s*"\K[^"]+' || echo "")

    if [ -n "$RECON_STATUS_A" ] && [ -n "$RECON_STATUS_B" ]; then
        mirror_ok "reconcile #$mround (A=$RECON_STATUS_A, B=$RECON_STATUS_B)"
    else
        mirror_fail "reconcile #$mround" "A=$RECON_STATUS_A, B=$RECON_STATUS_B"
    fi

    [ "$mround" -lt "$MIRROR_ROUNDS" ] && sleep "$INTERVAL"
done

# Accumulate mirror results into totals
PASS=$((PASS + MIRROR_PASS))
FAIL=$((FAIL + MIRROR_FAIL))

# ── Final Report ──────────────────────────────────────────────────
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
TOTAL=$((PASS + FAIL))
echo " DHT STRESS TEST RESULTS"
echo "   Healthy peers:     $HEALTHY / $N_PEERS"
echo "   Rounds executed:   $ROUNDS"
echo "   Mirror rounds:     $MIRROR_ROUNDS"
echo "   Passed:            $PASS / $TOTAL"
echo "   Failed:            $FAIL / $TOTAL"
if [ "$TOTAL" -gt 0 ]; then
    PCT=$(( PASS * 100 / TOTAL ))
    echo "   Success rate:      ${PCT}%"
fi
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo " Logs: $STAMPS_DIR/"
echo " Per-peer logs: $FORETIAS_LOG_BASE/"
echo ""

echo "ERROR summary:"
grep -rh "ERROR:" "$STAMPS_DIR"/*.log 2>/dev/null | sort | uniq -c | sort -rn | head -20 || true
echo ""

[ "$FAIL" -eq 0 ] && exit 0 || exit 1
