#!/usr/bin/env bash
# ── Foretias DHT Stress Test ────────────────────────────────────────
# Spawns N seed servers (each knows all seeds including itself — the
# Rust code recognizes self and skips self-dial). Then spawns N peer
# clients, each given a shuffled copy of the seed list so they connect
# in random order.
#
# All chronons are drawn uniformly from 0.1 s .. 10.0 s.
#
# After RUNTIME seconds all processes are killed, logs are collected,
# and a summary is printed.  For deep analysis run:
#   python3 dht_stress_analysis.py --logs <LOG_DIR>
#
# Usage: ./dht_stress_test.sh [OPTIONS]
#
set -euo pipefail

# ── Paths ──────────────────────────────────────────────────────────
P2P="${P2P:-/home/hcbusy/webhash/foretias/p2p}"
RUST_BIN="$P2P/target/debug/foretias"

# ── Config (defaults) ──────────────────────────────────────────────
N_SEEDS=3
N_PEERS=100
RUNTIME=300                # seconds (default 5 min)
DHT_NAMESPACE="stress-test"
SEED_PORT_BASE=4100        # seeds use SEED_PORT_BASE..SEED_PORT_BASE+N_SEEDS
PEER_P2P_RANGE="9910..9999"
CHRONON_MIN=100000000      # 0.1 s in ns
CHRONON_MAX=10000000000    # 10.0 s in ns

# ── Parse args ─────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
    case "$1" in
        --n-seeds)          N_SEEDS="$2"; shift 2;;
        --n-peers)          N_PEERS="$2"; shift 2;;
        --runtime)          RUNTIME="$2"; shift 2;;
        --dht-namespace)    DHT_NAMESPACE="$2"; shift 2;;
        --seed-port-base)   SEED_PORT_BASE="$2"; shift 2;;
        --p2p-range)        PEER_P2P_RANGE="$2"; shift 2;;
        --chronon-min-ns)   CHRONON_MIN="$2"; shift 2;;
        --chronon-max-ns)   CHRONON_MAX="$2"; shift 2;;
        *) echo "Unknown arg: $1"; exit 1;;
    esac
done

# ── State ──────────────────────────────────────────────────────────
PIDS=()
LOG_DIR=$(mktemp -d -t foretias-stress.XXXXXX)
trap 'cleanup' EXIT INT TERM

cleanup() {
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo " Shutting down ${#PIDS[@]} servers..."
    for pid in "${PIDS[@]}"; do
        kill "$pid" 2>/dev/null || true
    done
    wait 2>/dev/null || true
    echo " Logs: $LOG_DIR"
    echo " For analysis: python3 $P2P/dht_stress_analysis.py --logs $LOG_DIR"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
}

# ── Helpers ────────────────────────────────────────────────────────
random_ns_in_range() {
    # Returns a random integer in [min, max) using /dev/urandom
    local min=$1 max=$2
    local range=$((max - min))
    local rand_val=$(od -An -tu64 -N8 /dev/urandom | tr -d ' ')
    echo $(( min + rand_val % range ))
}

shuffle_array() {
    # Fisher-Yates shuffle, prints shuffled array on stdout (one per line)
    local arr=("$@")
    local n=${#arr[@]}
    for ((i = n - 1; i > 0; i--)); do
        local j=$(( RANDOM % (i + 1) ))
        local tmp="${arr[$i]}"
        arr[$i]="${arr[$j]}"
        arr[$j]="$tmp"
    done
    printf '%s\n' "${arr[@]}"
}

# ── Pre-flight ────────────────────────────────────────────────────
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo " Foretias DHT Stress Test"
echo " Seeds: $N_SEEDS | Peers: $N_PEERS | Runtime: ${RUNTIME}s"
echo " DHT Namespace: $DHT_NAMESPACE"
echo " Chronon range: $(( CHRONON_MIN / 1000000000 )).$(( (CHRONON_MIN % 1000000000) / 1000000 ))s – $(( CHRONON_MAX / 1000000000 )).$(( (CHRONON_MAX % 1000000000) / 1000000 ))s"
echo " Log dir: $LOG_DIR"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

if [ ! -x "$RUST_BIN" ]; then
    echo "ERROR: foretias binary not found at $RUST_BIN"
    echo "Run: cd $P2P && cargo build"
    exit 1
fi

# ── Build seed address list ────────────────────────────────────────
SEED_ADDRS=()
for ((s = 0; s < N_SEEDS; s++)); do
    PORT=$((SEED_PORT_BASE + s))
    SEED_ADDRS+=("127.0.0.1:$PORT")
done

# ── Step 1: Start Seed Servers ────────────────────────────────────
# Each seed knows ALL seeds (including itself — Rust skips self-dial).
echo "[1] Starting $N_SEEDS seed servers..."
for ((s = 0; s < N_SEEDS; s++)); do
    PORT=$((SEED_PORT_BASE + s))
    PERSIST="$LOG_DIR/seed_$s"
    mkdir -p "$PERSIST"
    CHRONON_NS=$(random_ns_in_range "$CHRONON_MIN" "$CHRONON_MAX")

    # Build known-servers args (all seeds)
    KNOWN_ARGS=()
    for addr in "${SEED_ADDRS[@]}"; do
        KNOWN_ARGS+=("--known-servers" "$addr")
    done

    "$RUST_BIN" serve \
        --addr "127.0.0.1:$PORT" \
        --persist-path "$PERSIST" \
        --chronon-ns "$CHRONON_NS" \
        --p2p-port-range "$PEER_P2P_RANGE" \
        --dht-namespace "$DHT_NAMESPACE" \
        "${KNOWN_ARGS[@]}" \
        > "$LOG_DIR/seed_$s.log" 2>&1 &
    PIDS+=($!)
    echo "    Seed $s  port=$PORT  chronon=${CHRONON_NS}ns  PID=${PIDS[-1]}"
done
sleep 3
echo ""

# ── Step 2: Start Peer Clients ────────────────────────────────────
# Each peer gets a SHUFFLED copy of the seed list (random connection order).
echo "[2] Starting $N_PEERS peer clients..."
BATCH_SIZE=20
for ((i = 1; i <= N_PEERS; i++)); do
    PERSIST="$LOG_DIR/peer_$i"
    mkdir -p "$PERSIST"
    CHRONON_NS=$(random_ns_in_range "$CHRONON_MIN" "$CHRONON_MAX")

    # Shuffle seed addresses for this peer
    SHUFFLED=$(shuffle_array "${SEED_ADDRS[@]}")
    KNOWN_ARGS=()
    while IFS= read -r addr; do
        KNOWN_ARGS+=("--known-servers" "$addr")
    done <<< "$SHUFFLED"

    "$RUST_BIN" serve \
        --addr "127.0.0.1:0" \
        --persist-path "$PERSIST" \
        --chronon-ns "$CHRONON_NS" \
        --p2p-port-range "$PEER_P2P_RANGE" \
        --dht-namespace "$DHT_NAMESPACE" \
        "${KNOWN_ARGS[@]}" \
        > "$LOG_DIR/peer_$i.log" 2>&1 &
    PIDS+=($!)

    # Batch progress
    if (( i % BATCH_SIZE == 0 )) || (( i == N_PEERS )); then
        echo "    Peers $i/$N_PEERS spawned"
    fi
done
echo ""
TOTAL_PROCS=${#PIDS[@]}
echo "    Total processes: $TOTAL_PROCS ($N_SEEDS seeds + $N_PEERS peers)"
echo ""

# ── Step 3: Run ───────────────────────────────────────────────────
echo "[3] Running for ${RUNTIME}s..."
echo "    (press Ctrl+C to stop early)"
echo ""

# Track start time for activity reporting
START_TIME=$(date +%s)

# Simple progress bar
for ((elapsed = 0; elapsed < RUNTIME; elapsed += 10)); do
    # Check if any seeds are still alive
    SEED_ALIVE=0
    for ((s = 0; s < N_SEEDS; s++)); do
        SPID=${PIDS[$s]}
        if kill -0 "$SPID" 2>/dev/null; then
            ((SEED_ALIVE++))
        fi
    done

    REMAINING=$((RUNTIME - elapsed))
    echo "  [${elapsed}s/${RUNTIME}s] seeds_alive=$SEED_ALIVE/${N_SEEDS}  remaining=${REMAINING}s"

    if [ "$elapsed" -lt "$((RUNTIME - 10))" ]; then
        sleep 10
    fi
done

echo ""

# ── Step 4: Graceful Shutdown ─────────────────────────────────────
echo "[4] Stopping all servers..."
for pid in "${PIDS[@]}"; do
    kill "$pid" 2>/dev/null || true
done
# Give processes a moment to flush logs
sleep 2
# Force kill any stragglers
for pid in "${PIDS[@]}"; do
    kill -9 "$pid" 2>/dev/null || true
done
wait 2>/dev/null || true
echo "    All stopped."
echo ""

# ── Step 5: Quick Summary ─────────────────────────────────────────
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo " QUICK SUMMARY"
TOTAL_LOGS=$(find "$LOG_DIR" -name '*.log' | wc -l)
echo "   Log files:       $TOTAL_LOGS"
echo "   Log directory:   $LOG_DIR"
echo ""
echo " Per-node line counts:"
for f in "$LOG_DIR"/*.log; do
    LINES=$(wc -l < "$f")
    printf "   %-30s %6d lines\n" "$(basename "$f")" "$LINES"
done
echo ""
echo " For detailed analysis run:"
echo "   python3 $P2P/dht_stress_analysis.py --logs $LOG_DIR"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
