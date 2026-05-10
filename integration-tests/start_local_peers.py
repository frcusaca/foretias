#!/usr/bin/env python3
"""
Foretias Local Peer Manager — Interactive peer spawning and control.

Spawns N foretias serve processes, provides an interactive REPL for
managing them: status, kill-random, kill-tbid, stamp, verify, shutdown.

A background stats thread continuously tails log files for errors,
warnings, and tick counts, printing periodic summaries to stderr.
"""

import argparse
import glob
import json
import os
import random
import re
import shlex
import signal
import subprocess
import sys
import threading
import time


# ── Configuration ────────────────────────────────────────────────────────────

DEFAULT_PEERS = 50
DEFAULT_PORT_BASE = 5000
DEFAULT_P2P_RANGE = "9900..9999"
DEFAULT_CHRONON_NS = 1_000_000_000  # 1 second
DEFAULT_DHT_NAMESPACE = "howto-demo"
DEFAULT_STATS_INTERVAL = 10  # seconds between stat prints
STATS_POLL_INTERVAL = 3  # seconds between log polling
BATCH_SIZE = 10
BATCH_DELAY = 0.5  # seconds between batches


# ── Binary Discovery ─────────────────────────────────────────────────────────

def find_foretias_binary(binary_path):
    """Find the foretias binary, preferring release over debug."""
    if binary_path and os.path.isfile(binary_path):
        return binary_path

    repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    p2p_dir = os.path.join(repo_root, "p2p")

    candidates = [
        os.path.join(p2p_dir, "target", "release", "foretias"),
        os.path.join(p2p_dir, "target", "debug", "foretias"),
    ]

    for path in candidates:
        if os.path.isfile(path) and os.access(path, os.X_OK):
            return path

    print(
        f"ERROR: foretias binary not found.\n"
        f"  Searched: {', '.join(candidates)}\n"
        f"  Run: cd {p2p_dir} && cargo build\n"
        f"  Or specify: --binary /path/to/foretias",
        file=sys.stderr,
    )
    sys.exit(1)


# ── Peer Spawning ────────────────────────────────────────────────────────────

def spawn_peers(args, binary):
    """Spawn N peer processes and return tracking list."""
    peers = []
    persist_base = args.persist_dir or (
        subprocess.check_output(["mktemp", "-d"]).decode().strip()
    )

    # Build address list for --known-servers
    addr_list = [f"127.0.0.1:{args.port_base + i}" for i in range(args.peers)]

    total_batches = (args.peers + BATCH_SIZE - 1) // BATCH_SIZE
    for batch_idx in range(total_batches):
        start = batch_idx * BATCH_SIZE
        end = min(start + BATCH_SIZE, args.peers)

        for i in range(start, end):
            rpc_port = args.port_base + i
            peer_persist = os.path.join(persist_base, f"peer_{i}")
            os.makedirs(peer_persist, exist_ok=True)
            log_path = os.path.join(peer_persist, "server.log")

            # Build --known-servers args (all peers; Rust skips self-dial)
            known_args = []
            for addr in addr_list:
                known_args.extend(["--known-servers", addr])

            cmd = [
                binary,
                "serve",
                "--addr", f"127.0.0.1:{rpc_port}",
                "--persist-path", peer_persist,
                "--chronon-ns", str(args.chronon_ns),
                "--p2p-port-range", args.p2p_range,
                "--dht-namespace", args.dht_namespace,
            ] + known_args

            try:
                with open(log_path, "w") as log_f:
                    proc = subprocess.Popen(
                        cmd, stdout=log_f, stderr=subprocess.STDOUT,
                        preexec_fn=os.setsid,
                    )
                peers.append({
                    "index": i,
                    "tbid": "",
                    "tbn": "",
                    "rpc_port": rpc_port,
                    "p2p_port": 0,
                    "pid": proc.pid,
                    "alive": True,
                    "persist_path": peer_persist,
                    "log_path": log_path,
                    "chronon_ns": args.chronon_ns,
                    "ticks": 0,
                    "start_time": time.time(),
                    "process": proc,
                })
            except Exception as e:
                print(f"  WARNING: Failed to start peer #{i}: {e}", file=sys.stderr)

        if batch_idx < total_batches - 1:
            time.sleep(BATCH_DELAY)

    # Wait for startup output to appear
    print("  Waiting for peers to initialize...", file=sys.stderr)
    time.sleep(3)

    # Parse startup info from logs
    for peer in peers:
        _parse_peer_startup(peer)

    alive_count = sum(1 for p in peers if p["alive"])
    print(
        f"  Spawned {len(peers)} peers, {alive_count} alive.",
        file=sys.stderr,
    )
    return peers


def _parse_peer_startup(peer):
    """Parse TBID, TBN, P2P info from startup log."""
    log_path = peer["log_path"]
    if not os.path.isfile(log_path):
        return

    try:
        content = open(log_path).read()
    except OSError:
        return

    m_tbid = re.search(r"TBID\s*:\s*([0-9a-fA-F]+)", content)
    if m_tbid:
        peer["tbid"] = m_tbid.group(1)

    m_tbn = re.search(r"TBN\s*:\s*(\S+)", content)
    if m_tbn:
        peer["tbn"] = m_tbn.group(1)

    m_p2p = re.search(r"P2P Listen\s*:/ip4/\S+/tcp/(\d+)", content)
    if m_p2p:
        peer["p2p_port"] = int(m_p2p.group(1))

    m_ticks = re.search(r"Tick\s+#?(\d+)", content)
    if m_ticks:
        peer["ticks"] = int(m_ticks.group(1))


# ── Stats Thread ─────────────────────────────────────────────────────────────

class StatsGatherer:
    """Background thread that tails peer logs and tracks errors/warnings/ticks."""

    ERROR_RE = re.compile(r"(error|ERROR|panic|FAILED)", re.IGNORECASE)
    WARN_RE = re.compile(r"(warning|WARN)", re.IGNORECASE)
    TICK_RE = re.compile(r"Tick\s+#?(\d+)")

    def __init__(self, peers, interval, stop_event):
        self.peers = peers
        self.interval = interval
        self.stop_event = stop_event
        self.lock = threading.Lock()
        self.thread = threading.Thread(target=self._run, daemon=True)

        # Per-peer tracking
        self.per_peer = {}
        for p in peers:
            self.per_peer[p["index"]] = {
                "errors": 0,
                "warnings": 0,
                "last_tick": p["ticks"],
                "last_log_pos": 0,
            }
            # Initialize log position to current file size
            lp = p["log_path"]
            if os.path.isfile(lp):
                try:
                    self.per_peer[p["index"]]["last_log_pos"] = os.path.getsize(lp)
                except OSError:
                    pass

        self.start_time = time.time()
        self.aggregate_errors = 0
        self.aggregate_warnings = 0

    def start(self):
        self.thread.start()

    def _run(self):
        last_print = time.time()
        while not self.stop_event.is_set():
            self._poll_all()
            now = time.time()
            if self.interval > 0 and (now - last_print) >= self.interval:
                self._print_summary()
                last_print = now
            self.stop_event.wait(STATS_POLL_INTERVAL)

    def _poll_all(self):
        with self.lock:
            for p in self.peers:
                idx = p["index"]
                if not p["alive"]:
                    continue
                lp = p["log_path"]
                state = self.per_peer[idx]
                try:
                    sz = os.path.getsize(lp)
                except OSError:
                    continue
                if sz <= state["last_log_pos"]:
                    continue

                try:
                    with open(lp, "r") as f:
                        f.seek(state["last_log_pos"])
                        new_lines = f.readlines()
                        state["last_log_pos"] = f.tell()
                except OSError:
                    continue

                for line in new_lines:
                    if self.ERROR_RE.search(line):
                        state["errors"] += 1
                        self.aggregate_errors += 1
                    if self.WARN_RE.search(line):
                        state["warnings"] += 1
                        self.aggregate_warnings += 1
                    mt = self.TICK_RE.search(line)
                    if mt:
                        t = int(mt.group(1))
                        if t > state["last_tick"]:
                            state["last_tick"] = t
                            p["ticks"] = t

    def _print_summary(self):
        with self.lock:
            elapsed = time.time() - self.start_time
            alive = sum(1 for p in self.peers if p["alive"])
            total = len(self.peers)

            # Find highest tick
            highest_tick = 0
            highest_peer = -1
            for p in self.peers:
                if p["ticks"] > highest_tick:
                    highest_tick = p["ticks"]
                    highest_peer = p["index"]

            avg_tps = 0.0
            if elapsed > 0 and alive > 0:
                avg_tps = highest_tick / elapsed

            msg = (
                f"\r--- Live Stats (elapsed {int(elapsed)}s) ---\n"
                f"  Alive: {alive}/{total}  |  Errors: {self.aggregate_errors}  |  Warnings: {self.aggregate_warnings}\n"
                f"  Avg ticks/sec: {avg_tps:.1f}  |  Highest tick: {highest_tick} (peer #{highest_peer})"
            )
            sys.stderr.write(msg + "\n")
            sys.stderr.flush()

    def get_stats_snapshot(self):
        """Return a snapshot of current stats for the REPL."""
        with self.lock:
            result = {}
            for p in self.peers:
                idx = p["index"]
                result[idx] = dict(self.per_peer[idx])
            return {
                "aggregate_errors": self.aggregate_errors,
                "aggregate_warnings": self.aggregate_warnings,
                "per_peer": result,
            }


# ── REPL Commands ────────────────────────────────────────────────────────────

def check_peer_alive(peer):
    """Check if a peer process is still running."""
    try:
        os.kill(peer["pid"], 0)
        return True
    except OSError:
        return False


def cmd_status(peers):
    """Print status table of all peers."""
    header = f"{'Idx':>4}  {'TBID':>16}  {'Port':>6}  {'Alive':>6}  {'Ticks':>6}"
    print(header)
    print("-" * len(header))
    for p in sorted(peers, key=lambda x: x["index"]):
        if not p["alive"]:
            continue
        if not check_peer_alive(p):
            p["alive"] = False
        tbid_short = p["tbid"][:16] if p["tbid"] else "--------"
        print(
            f"{p['index']:>4}  {tbid_short:>16}  {p['rpc_port']:>6}  "
            f"{'YES':>6}  {p['ticks']:>6}")


def cmd_stats(peers, stats_gatherer):
    """Print full per-peer stats breakdown."""
    snapshot = stats_gatherer.get_stats_snapshot()
    print(f"\nAggregate: errors={snapshot['aggregate_errors']} warnings={snapshot['aggregate_warnings']}\n")
    header = f"{'Idx':>4}  {'Errors':>6}  {'Warns':>6}  {'Ticks':>6}  {'TBID':>16}"
    print(header)
    print("-" * len(header))
    for p in sorted(peers, key=lambda x: x["index"]):
        idx = p["index"]
        ps = snapshot["per_peer"].get(idx, {})
        tbid_short = p["tbid"][:16] if p["tbid"] else "--------"
        print(
            f"{idx:>4}  {ps.get('errors', 0):>6}  {ps.get('warnings', 0):>6}  "
            f"{ps.get('last_tick', p['ticks']):>6}  {tbid_short:>16}"
        )


def cmd_kill_random(peers, n_or_pct_str):
    """Kill N random alive peers, or N% of alive peers."""
    alive = [p for p in peers if p["alive"]]
    if not alive:
        print("  No alive peers to kill.")
        return

    if n_or_pct_str.endswith("%"):
        pct = int(n_or_pct_str[:-1])
        count = max(1, len(alive) * pct // 100)
    else:
        count = int(n_or_pct_str)

    count = min(count, len(alive))
    targets = random.sample(alive, count)

    for p in targets:
        try:
            os.killpg(os.getpgid(p["pid"]), signal.SIGTERM)
        except ProcessLookupError:
            pass
        p["alive"] = False

    killed_indices = [p["index"] for p in targets]
    time.sleep(0.5)  # Allow processes to actually terminate
    print(f"  Killed {count} peers: indices {killed_indices}")


def cmd_kill_tbid(peers, prefix):
    """Kill peers matching TBID prefix."""
    matches = [p for p in peers if p["alive"] and p["tbid"].startswith(prefix)]
    if not matches:
        print(f"  No alive peers with TBID starting with '{prefix}'.")
        return

    for p in matches:
        try:
            os.killpg(os.getpgid(p["pid"]), signal.SIGTERM)
        except ProcessLookupError:
            pass
        p["alive"] = False

    killed_indices = [p["index"] for p in matches]
    print(f"  Killed {len(matches)} peers: indices {killed_indices}")


def cmd_stamp(binary, port, message):
    """Run foretias stamp against a peer."""
    cmd = [binary, "stamp", "-m", message, "-s", f"127.0.0.1:{port}"]
    print(f"  $ {' '.join(cmd)}")
    result = subprocess.run(cmd, capture_output=True, text=True, timeout=10)
    if result.stdout:
        print(result.stdout.rstrip())
    if result.returncode != 0 and result.stderr:
        print(f"  Error: {result.stderr.strip()}")
    return result.stdout


def cmd_verify(binary, port, message, stamp_file):
    """Run foretias verify against a peer."""
    # Expand globs in stamp file path
    expanded = glob.glob(stamp_file)
    if not expanded:
        print(f"  Error: no file matches '{stamp_file}'")
        return ""
    stamp_file = expanded[0]
    cmd = [binary, "verify", "-m", message, "-F", stamp_file, "-s", f"127.0.0.1:{port}"]
    print(f"  $ {' '.join(cmd)}")
    result = subprocess.run(cmd, capture_output=True, text=True, timeout=10)
    if result.stdout:
        print(result.stdout.rstrip())
    if result.returncode != 0 and result.stderr:
        print(f"  Error: {result.stderr.strip()}")
    return result.stdout


def cmd_shutdown(peers, session_start, stamps_made, verifies_done):
    """Graceful shutdown of all remaining peers."""
    alive = [p for p in peers if p["alive"]]
    for p in alive:
        try:
            os.killpg(os.getpgid(p["pid"]), signal.SIGTERM)
        except ProcessLookupError:
            pass
        p["alive"] = False

    # Wait briefly for graceful shutdown
    time.sleep(1)

    # Force kill stragglers
    for p in peers:
        if p["alive"]:
            try:
                os.killpg(os.getpgid(p["pid"]), signal.SIGKILL)
            except ProcessLookupError:
                pass
            p["alive"] = False

    duration = time.time() - session_start
    mins, secs = divmod(int(duration), 60)

    print("")
    print("=" * 40)
    print("=== Session Summary ===")
    print(f"Peers started:    {len(peers)}")
    print(f"Peers killed:     {len(peers) - sum(1 for p in peers if p['alive'])}")
    print(f"Peers remaining:  {sum(1 for p in peers if p['alive'])}")
    print(f"Stamps made:      {stamps_made}")
    print(f"Verifications:    {verifies_done}")
    print(f"Duration:         {mins}m {secs}s")
    print("=" * 40)


# ── REPL Loop ────────────────────────────────────────────────────────────────

HELP_TEXT = """
Commands:
  status                  Show peer status table
  stats                   Show detailed per-peer stats (errors, warnings, ticks)
  sleep <seconds>         Pause for N seconds
  kill-random N|N%        Kill N random alive peers (or N% of alive)
  kill-tbid <hex-prefix>  Kill peers matching TBID prefix
  stamp <port> <message>  Stamp a message via the given peer port
  verify <port> <msg> <stamp.json>  Verify a stamp via the given peer port
  help                    Show this help
  shutdown / quit         Graceful shutdown and summary
"""


def repl_loop(peers, binary, stats_gatherer):
    """Interactive REPL for managing peers."""
    session_start = time.time()
    stamps_made = 0
    verifies_done = 0

    print(HELP_TEXT)

    while True:
        try:
            line = input("foretias> ").strip()
        except (EOFError, KeyboardInterrupt):
            print("\n  Shutting down...")
            break

        if not line:
            continue

        parts = line.split(None, 1)
        cmd = parts[0].lower()
        args_str = parts[1] if len(parts) > 1 else ""

        if cmd in ("shutdown", "quit", "exit"):
            cmd_shutdown(peers, session_start, stamps_made, verifies_done)
            break

        elif cmd == "help":
            print(HELP_TEXT)

        elif cmd == "status":
            cmd_status(peers)

        elif cmd == "stats":
            cmd_stats(peers, stats_gatherer)

        elif cmd == "kill-random":
            cmd_kill_random(peers, args_str.strip())

        elif cmd == "sleep":
            try:
                secs = float(args_str.strip())
                print(f"  Sleeping for {secs}s...")
                time.sleep(secs)
            except ValueError:
                print("  Usage: sleep <seconds>")

        elif cmd == "kill-tbid":
            cmd_kill_tbid(peers, args_str.strip())

        elif cmd == "stamp":
            try:
                sp = shlex.split(args_str.strip())
            except ValueError:
                sp = args_str.strip().split(None, 1)
            if len(sp) < 2:
                print("  Usage: stamp <port> <message>")
                continue
            stamp_json = cmd_stamp(binary, sp[0], " ".join(sp[1:]))
            if stamp_json:
                # Save to temp file for later verify
                tmp = f"/tmp/foretias_stamp_{int(time.time())}.json"
                with open(tmp, "w") as f:
                    f.write(stamp_json.strip())
                print(f"  Saved to: {tmp}")
            stamps_made += 1

        elif cmd == "verify":
            # Use shlex to properly handle quoted messages
            try:
                sp = shlex.split(args_str.strip())
            except ValueError:
                sp = args_str.strip().split(None, 2)
            if len(sp) < 3:
                print("  Usage: verify <port> <message> <stamp.json>")
                continue
            cmd_verify(binary, sp[0], sp[1], sp[2])
            verifies_done += 1

        else:
            print(f"  Unknown command: {cmd}. Type 'help' for options.")


# ── Main ─────────────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(
        description="Foretias Local Peer Manager — spawn and manage local peers",
    )
    parser.add_argument("--peers", type=int, default=DEFAULT_PEERS, help="Number of peers to spawn")
    parser.add_argument("--port-base", type=int, default=DEFAULT_PORT_BASE, help="Starting RPC port")
    parser.add_argument("--p2p-range", type=str, default=DEFAULT_P2P_RANGE, help="P2P port range")
    parser.add_argument("--chronon-ns", type=int, default=DEFAULT_CHRONON_NS, help="Chronon period in ns")
    parser.add_argument("--dht-namespace", type=str, default=DEFAULT_DHT_NAMESPACE, help="DHT namespace")
    parser.add_argument("--binary", type=str, default=None, help="Path to foretias binary")
    parser.add_argument("--persist-dir", type=str, default=None, help="Base directory for persist paths")
    parser.add_argument("--stats-interval", type=int, default=DEFAULT_STATS_INTERVAL, help="Seconds between stat prints (0 disables)")

    args = parser.parse_args()
    binary = find_foretias_binary(args.binary)

    print("=" * 50)
    print(" Foretias Local Peer Manager")
    print(f" Peers: {args.peers} | Port base: {args.port_base}")
    print(f" Chronon: {args.chronon_ns / 1_000_000_000:.1f}s | DHT: {args.dht_namespace}")
    print("=" * 50)
    print("")

    print("[1] Spawning peers...", file=sys.stderr)
    peers = spawn_peers(args, binary)

    # Start stats thread
    stop_event = threading.Event()
    stats = StatsGatherer(peers, args.stats_interval, stop_event)
    stats.start()

    print("[2] Entering REPL. Type 'help' for commands.", file=sys.stderr)
    print("")

    # Handle SIGINT for clean exit
    def sigint_handler(signum, frame):
        stop_event.set()
        print("\n  Caught interrupt, shutting down...", file=sys.stderr)
        cmd_shutdown(peers, stats.start_time, 0, 0)
        sys.exit(0)

    signal.signal(signal.SIGINT, sigint_handler)

    try:
        repl_loop(peers, binary, stats)
    finally:
        stop_event.set()


if __name__ == "__main__":
    main()
