#!/usr/bin/env python3
"""Foretias DHT Stress Test — Log Analysis Notebook.

Can be run standalone:
    python3 dht_stress_analysis.py --logs /tmp/foretias-stress.XXXXXX

Or opened in Jupyter:
    jupyter notebook dht_stress_analysis.ipynb

Sections:
    1. Parse all node logs
    2. Per-server stamp & verify counts
    3. Distribution plots (stamps, verifies, chronons)
    4. Network activity over time
    5. Cross-node correctness (successes / failures)
"""

from __future__ import annotations

import argparse
import glob
import json
import os
import re
import sys
from collections import Counter, defaultdict
from dataclasses import dataclass
from datetime import datetime, timedelta

import matplotlib
matplotlib.use("Agg")  # non-interactive backend for CLI; Jupyter overrides this
import matplotlib.pyplot as plt
import numpy as np


# ────────────────────────────────────────────────────────────────────
# 1. PARSING
# ────────────────────────────────────────────────────────────────────

@dataclass
class NodeLog:
    name: str                       # "seed_0", "peer_42"
    role: str                       # "seed" | "peer"
    log_path: str
    chronon_ns: int | None          # parsed from startup
    port: int | None                # parsed from startup
    tbid: str | None                # hex TBID
    stamps: int                     # JSON-RPC stamp calls handled
    verifies: int                   # JSON-RPC verify calls handled
    valid_count: int                # verifies that returned valid=true
    invalid_count: int              # verifies that returned valid=false
    errors: int                     # ERROR lines
    peers_discovered: int           # DHT-discovered peer events
    self_skipped: int               # "skipping self-dial" count
    json_rpc_requests: list[dict]  # all parsed JSON-RPC request/response pairs


def parse_log(path: str) -> NodeLog | None:
    """Best-effort parser for a single node log file."""
    name = os.path.splitext(os.path.basename(path))[0]
    role = "seed" if name.startswith("seed_") else "peer"

    nl = NodeLog(
        name=name, role=role, log_path=path,
        chronon_ns=None, port=None, tbid=None,
        stamps=0, verifies=0, valid_count=0, invalid_count=0,
        errors=0, peers_discovered=0, self_skipped=0,
        json_rpc_requests=[],
    )

    try:
        with open(path, "r", errors="replace") as f:
            lines = f.readlines()
    except OSError:
        return None

    for line in lines:
        # ── Startup info ──
        m = re.search(r"Chronon:\s*(\d+)", line)
        if m and nl.chronon_ns is None:
            try:
                nl.chronon_ns = int(m.group(1))
            except ValueError:
                pass

        m = re.search(r"Listen\s*:\s*\S+:(\d+)", line)
        if m and nl.port is None:
            try:
                nl.port = int(m.group(1))
            except ValueError:
                pass

        m = re.search(r"TBID\s*:\s*([0-9a-f]{32})", line)
        if m and nl.tbid is None:
            nl.tbid = m.group(1)

        # ── RPC method counts ──
        if '"method":"stamp"' in line or '"method": "stamp"' in line:
            nl.stamps += 1

        if '"method":"verify"' in line or '"method": "verify"' in line:
            nl.verifies += 1

        # ── Validity counts ──
        if '"valid":true' in line or '"valid": true' in line:
            nl.valid_count += 1

        if '"valid":false' in line or '"valid": false' in line:
            nl.invalid_count += 1

        # ── Errors ──
        if "ERROR" in line or "error:" in line.lower():
            nl.errors += 1

        # ── DHT peer discovery ──
        if "DHT-discovered peer added to pool" in line:
            nl.peers_discovered += 1

        # ── Self-recognition ──
        if "skipping self-dial" in line:
            nl.self_skipped += 1

        # ── JSON-RPC requests (for time-series) ──
        if '"jsonrpc":"2.0"' in line and '"method"' in line:
            try:
                obj = json.loads(line.strip())
                nl.json_rpc_requests.append(obj)
            except json.JSONDecodeError:
                pass

    return nl


# ────────────────────────────────────────────────────────────────────
# 2. AGGREGATION
# ────────────────────────────────────────────────────────────────────

def load_all_logs(log_dir: str) -> list[NodeLog]:
    logs = []
    for path in sorted(glob.glob(os.path.join(log_dir, "*.log"))):
        nl = parse_log(path)
        if nl is not None:
            logs.append(nl)
    return logs


def print_summary(logs: list[NodeLog]) -> None:
    seeds = [l for l in logs if l.role == "seed"]
    peers = [l for l in logs if l.role == "peer"]

    total_stamps = sum(l.stamps for l in logs)
    total_verifies = sum(l.verifies for l in logs)
    total_valid = sum(l.valid_count for l in logs)
    total_invalid = sum(l.invalid_count for l in logs)
    total_errors = sum(l.errors for l in logs)
    total_self_skipped = sum(l.self_skipped for l in logs)
    total_discovered = sum(l.peers_discovered for l in logs)

    print("=" * 70)
    print(" FORETIAS DHT STRESS TEST — RESULTS")
    print("=" * 70)
    print(f"  Nodes analysed:     {len(logs)}  (seeds={len(seeds)}, peers={len(peers)})")
    print(f"  Total stamps:       {total_stamps}")
    print(f"  Total verifies:     {total_verifies}")
    print(f"  Valid responses:    {total_valid}  ({total_valid/(total_valid+total_invalid)*100:.1f}%)" if total_valid + total_invalid else f"  Valid responses:    0")
    print(f"  Invalid responses:  {total_invalid}")
    print(f"  Total errors:       {total_errors}")
    print(f"  Self-dials skipped: {total_self_skipped}")
    print(f"  Peers discovered:   {total_discovered}")
    print("=" * 70)

    # ── Per-server distribution ──
    print("\n  Per-server stamp distribution:")
    stamps = [l.stamps for l in logs]
    if stamps:
        print(f"    min={min(stamps)}  max={max(stamps)}  mean={np.mean(stamps):.1f}  median={np.median(stamps):.1f}")
    else:
        print("    (no stamp data)")

    print("\n  Per-server verify distribution:")
    verifies = [l.verifies for l in logs]
    if verifies:
        print(f"    min={min(verifies)}  max={max(verifies)}  mean={np.mean(verifies):.1f}  median={np.median(verifies):.1f}")
    else:
        print("    (no verify data)")

    # ── Chronon distribution ──
    chronons = [l.chronon_ns for l in logs if l.chronon_ns is not None]
    if chronons:
        chronons_s = [c / 1e9 for c in chronons]
        print(f"\n  Chronon distribution:")
        print(f"    range=[{min(chronons_s):.1f}s, {max(chronons_s):.1f}s]  mean={np.mean(chronons_s):.2f}s")

    # ── Per-node table ──
    print("\n  Per-node breakdown:")
    print(f"  {'node':<15} {'role':<6} {'stamps':>7} {'verifies':>9} {'valid':>6} {'invalid':>8} {'errors':>7} {'peers':>6}")
    print(f"  {'-'*14} {'-'*5} {'-'*7} {'-'*9} {'-'*6} {'-'*8} {'-'*7} {'-'*6}")
    for l in sorted(logs, key=lambda x: x.name):
        print(f"  {l.name:<15} {l.role:<6} {l.stamps:>7} {l.verifies:>9} {l.valid_count:>6} {l.invalid_count:>8} {l.errors:>7} {l.peers_discovered:>6}")

    print()


# ────────────────────────────────────────────────────────────────────
# 3. PLOTTING
# ────────────────────────────────────────────────────────────────────

def make_plots(logs: list[NodeLog], output_dir: str) -> None:
    """Generate distribution and time-series plots."""
    os.makedirs(output_dir, exist_ok=True)
    fig, axes = plt.subplots(2, 3, figsize=(18, 10))
    fig.suptitle("Foretias DHT Stress Test — Analysis", fontsize=14, fontweight="bold")

    seeds = [l for l in logs if l.role == "seed"]
    peers = [l for l in logs if l.role == "peer"]

    # ── (0,0) Stamps per node (histogram) ──
    stamps = [l.stamps for l in logs if l.stamps > 0]
    if stamps:
        axes[0, 0].hist(stamps, bins=min(20, len(stamps)), color="#2196F3", edgecolor="white", alpha=0.8)
        axes[0, 0].set_title("Stamps per Node")
        axes[0, 0].set_xlabel("Stamps")
        axes[0, 0].set_ylabel("Nodes")
    else:
        axes[0, 0].text(0.5, 0.5, "No stamp data", ha="center", va="center", transform=axes[0, 0].transAxes)

    # ── (0,1) Verifies per node (histogram) ──
    verifies = [l.verifies for l in logs if l.verifies > 0]
    if verifies:
        axes[0, 1].hist(verifies, bins=min(20, len(verifies)), color="#4CAF50", edgecolor="white", alpha=0.8)
        axes[0, 1].set_title("Verifies per Node")
        axes[0, 1].set_xlabel("Verifies")
        axes[0, 1].set_ylabel("Nodes")
    else:
        axes[0, 1].text(0.5, 0.5, "No verify data", ha="center", va="center", transform=axes[0, 1].transAxes)

    # ── (0,2) Chronon distribution ──
    chronons = [l.chronon_ns / 1e9 for l in logs if l.chronon_ns is not None]
    if chronons:
        axes[0, 2].hist(chronons, bins=20, color="#FF9800", edgecolor="white", alpha=0.8)
        axes[0, 2].set_title("Chronon Distribution")
        axes[0, 2].set_xlabel("Chronon (s)")
        axes[0, 2].set_ylabel("Nodes")
    else:
        axes[0, 2].text(0.5, 0.5, "No chronon data", ha="center", va="center", transform=axes[0, 2].transAxes)

    # ── (1,0) Seeds vs Peers stamp boxplot ──
    seed_stamps = [l.stamps for l in seeds]
    peer_stamps = [l.stamps for l in peers]
    if seed_stamps or peer_stamps:
        data = [x for x in [seed_stamps, peer_stamps] if x]
        axes[1, 0].boxplot(data, labels=["Seeds", "Peers"] if len(data) == 2 else ["Nodes"])
        axes[1, 0].set_title("Stamps: Seeds vs Peers")
        axes[1, 0].set_ylabel("Stamps")
    else:
        axes[1, 0].text(0.5, 0.5, "No data", ha="center", va="center", transform=axes[1, 0].transAxes)

    # ── (1,1) Peers discovered per node ──
    discovered = [l.peers_discovered for l in logs if l.peers_discovered > 0]
    if discovered:
        axes[1, 1].hist(discovered, bins=min(20, len(discovered)), color="#9C27B0", edgecolor="white", alpha=0.8)
        axes[1, 1].set_title("Peers Discovered per Node")
        axes[1, 1].set_xlabel("Peers")
        axes[1, 1].set_ylabel("Nodes")
    else:
        axes[1, 1].text(0.5, 0.5, "No discovery data", ha="center", va="center", transform=axes[1, 1].transAxes)

    # ── (1,2) Success rate bar ──
    total_valid = sum(l.valid_count for l in logs)
    total_invalid = sum(l.invalid_count for l in logs)
    total = total_valid + total_invalid
    if total > 0:
        axes[1, 2].barh(["Valid", "Invalid"], [total_valid, total_invalid], color=["#4CAF50", "#F44336"])
        axes[1, 2].set_title(f"Correctness ({total_valid/total*100:.1f}% valid)")
        axes[1, 2].set_xlabel("Count")
    else:
        axes[1, 2].text(0.5, 0.5, "No verify data", ha="center", va="center", transform=axes[1, 2].transAxes)

    plt.tight_layout()
    path = os.path.join(output_dir, "analysis.png")
    fig.savefig(path, dpi=150)
    print(f"  Plots saved: {path}")
    plt.close(fig)

    # ── Time-series: cumulative network activity ──
    # Aggregate all JSON-RPC requests by approximate time bucket
    # (logs don't have timestamps, so we use request order as proxy)
    all_requests = []
    for l in logs:
        for req in l.json_rpc_requests:
            all_requests.append(req)

    if all_requests:
        fig2, ax2 = plt.subplots(figsize=(10, 5))
        methods = [r.get("method", "unknown") for r in all_requests]
        method_counts = Counter(methods)
        labels = list(method_counts.keys())
        values = list(method_counts.values())
        colors = ["#2196F3", "#4CAF50", "#FF9800", "#9C27B0", "#607D8B"]
        ax2.bar(labels, values, color=colors[:len(labels)], alpha=0.8)
        ax2.set_title("JSON-RPC Methods Across All Nodes")
        ax2.set_xlabel("Method")
        ax2.set_ylabel("Requests")
        plt.tight_layout()
        path2 = os.path.join(output_dir, "rpc_methods.png")
        fig2.savefig(path2, dpi=150)
        print(f"  RPC methods: {path2}")
        plt.close(fig2)


# ────────────────────────────────────────────────────────────────────
# MAIN
# ────────────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(description="Analyse Foretias DHT stress test logs")
    parser.add_argument("--logs", required=True, help="Path to log directory")
    parser.add_argument("--output", default=None, help="Output directory for plots (default: logs/analysis/)")
    args = parser.parse_args()

    log_dir = args.logs
    output_dir = args.output or os.path.join(log_dir, "analysis")

    print(f"Loading logs from {log_dir} ...")
    logs = load_all_logs(log_dir)

    if not logs:
        print("No log files found.")
        sys.exit(1)

    print_summary(logs)
    make_plots(logs, output_dir)


if __name__ == "__main__":
    main()
