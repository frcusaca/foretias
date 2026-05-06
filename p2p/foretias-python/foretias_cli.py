#!/usr/bin/env python3
"""Foretias CLI — Python implementation.

Conforms to: foretias/specs/FORETIAS_CLI_SPEC.md
Commands: serve, stamp, verify, prove, inspect
"""
import argparse
import json
import sys
import time
import socket
from pathlib import Path

from foretias_p2p import (
    PyTimeFamilyServer,
    PyForetis,
    PyCalendar,
)


def cmd_stamp(args):
    """Stamp content via TimeFamilyServer."""
    if args.message and args.message_file:
        print("Error: --message and --message-file are mutually exclusive", file=sys.stderr)
        sys.exit(1)
    if not args.message and not args.message_file:
        print("Error: exactly one of --message or --message-file is required", file=sys.stderr)
        sys.exit(1)

    content = (
        args.message.encode()
        if args.message
        else Path(args.message_file).read_bytes()
    )

    server = PyTimeFamilyServer(listen_addr=args.server)
    foretis = server.stamp(content, args.echo or "")

    out = json.loads(foretis.to_json())
    if args.stamp_output:
        Path(args.stamp_output).write_text(json.dumps(out, indent=2))
    else:
        print(json.dumps(out, indent=2))


def cmd_verify(args):
    """Verify content against a Foretis."""
    if args.message and args.message_file:
        print("Error: --message and --message-file are mutually exclusive", file=sys.stderr)
        sys.exit(1)
    if not args.message and not args.message_file:
        print("Error: exactly one of --message or --message-file is required", file=sys.stderr)
        sys.exit(1)

    content = (
        args.message.encode()
        if args.message
        else Path(args.message_file).read_bytes()
    )

    if args.foretis and args.foretis_file:
        print("Error: --foretis and --foretis-file are mutually exclusive", file=sys.stderr)
        sys.exit(1)
    if not args.foretis and not args.foretis_file:
        print("Error: exactly one of --foretis or --foretis-file is required", file=sys.stderr)
        sys.exit(1)

    foretis_json = args.foretis or Path(args.foretis_file).read_text()

    server = PyTimeFamilyServer(listen_addr=args.server)
    valid = server.verify(content, PyForetis.from_json(foretis_json))

    result = {"valid": valid}
    if args.verify_output:
        Path(args.verify_output).write_text(json.dumps(result, indent=2))
    else:
        print(json.dumps(result, indent=2))


def cmd_prove(args):
    """Fetch calendar slice and verify locally."""
    if args.message and args.message_file:
        print("Error: --message and --message-file are mutually exclusive", file=sys.stderr)
        sys.exit(1)
    if not args.message and not args.message_file:
        print("Error: exactly one of --message or --message-file is required", file=sys.stderr)
        sys.exit(1)

    content = (
        args.message.encode()
        if args.message
        else Path(args.message_file).read_bytes()
    )

    foretis_json = args.foretis or Path(args.foretis_file).read_text()
    foretis = PyForetis.from_json(foretis_json)

    server = PyTimeFamilyServer(listen_addr=args.server)
    slice_ = server.get_calendar_slice(foretis.tick_number, 2)

    if not slice_:
        print(json.dumps({"valid": False, "error": "calendar slice empty"}), file=sys.stdout)
        sys.exit(1)

    valid = server.verify(content, foretis)
    result = {"valid": valid, "tick": foretis.tick_number}
    if args.proof_output:
        Path(args.proof_output).write_text(json.dumps(result, indent=2))
    else:
        print(json.dumps(result, indent=2))


def cmd_inspect(args):
    """Inspect external attestations in a persisted calendar."""
    cal_path = Path(args.calendar_path)
    if not cal_path.exists():
        print(f"Error: {cal_path} not found", file=sys.stderr)
        sys.exit(1)

    data = json.loads(cal_path.read_text())
    ticks = data.get("ticks", [])
    total = 0
    valid = 0
    invalid = 0

    for tick in ticks:
        attestations = tick.get("external_attestations", [])
        total += len(attestations)
        for att in attestations:
            if att.get("foretis", {}).get("signature"):
                valid += 1
            else:
                invalid += 1

    result = {
        "calendar": str(cal_path),
        "ticks": len(ticks),
        "total_attestations": total,
        "valid": valid,
        "invalid": invalid,
    }
    print(json.dumps(result, indent=2))
    sys.exit(0 if invalid == 0 else 1)


def cmd_serve(args):
    """Start the TimeFamilyServer."""
    # Parse address
    addr = args.addr
    host, port = addr.rsplit(":", 1)
    chronon_ns = args.chronon_ns

    # Build server
    persist = args.persist_path if args.persist_path else None

    if args.start_dormant:
        if not persist:
            print("Error: --start-dormant requires --persist-path", file=sys.stderr)
            sys.exit(1)
        server = PyTimeFamilyServer.from_calendar(
            Path(persist).as_posix(), addr
        )
    else:
        server = PyTimeFamilyServer(addr, chronon_ns, persist)

    # Print startup info
    print("Foretias TimeFamilyServer starting...")
    print(f"  Listen : {addr}")
    print(f"  TBN    : {server.get_tbn()}")
    print(f"  TBID   : {server.get_tbid()}")
    if server.is_dormant():
        print("  Mode   : dormant (verify-only)")
    else:
        print(f"  Chronon: {humanize_nanoseconds(chronon_ns)}")

    # P2P info (future — when bindings support libp2p)
    if args.known_servers:
        print(f"  Known Servers : {', '.join(args.known_servers)}")
    if args.max_discovered_peers != 13:
        print(f"  Max Discovered Peers : {args.max_discovered_peers}")

    sys.stdout.flush()

    # Run daemon loop
    try:
        interval = chronon_ns / 1_000_000_000
        while True:
            server.daemon_tick()
            time.sleep(max(interval, 1))
    except KeyboardInterrupt:
        print("\nShutting down...")
        try:
            server.save()
        except Exception:
            pass


def humanize_nanoseconds(ns):
    """Humanize a nanosecond duration into readable English."""
    if ns % 86400_000_000_000 == 0 and ns // 86400_000_000_000 == 1:
        return "1 day"
    if ns % 86400_000_000_000 == 0:
        return f"{ns // 86400_000_000_000} days"
    if ns % 3600_000_000_000 == 0 and ns // 3600_000_000_000 == 1:
        return "1 hour"
    if ns % 3600_000_000_000 == 0:
        return f"{ns // 3600_000_000_000} hours"
    if ns % 60_000_000_000 == 0 and ns // 60_000_000_000 == 1:
        return "1 minute"
    if ns % 60_000_000_000 == 0:
        return f"{ns // 60_000_000_000} minutes"
    if ns % 1_000_000_000 == 0 and ns // 1_000_000_000 == 1:
        return "1 second"
    if ns % 1_000_000_000 == 0:
        return f"{ns // 1_000_000_000} seconds"
    return f"{ns} ns"


def main():
    parser = argparse.ArgumentParser(
        prog="foretias",
        description="Foretias Time Integrity Attestation Service",
    )
    subparsers = parser.add_subparsers(dest="command")

    # ── serve ────────────────────────────────────────────────
    p_serve = subparsers.add_parser("serve", help="Start the TimeFamilyServer")
    p_serve.add_argument("-a", "--addr", default="127.0.0.1:4001", help="JSON-RPC listen address")
    p_serve.add_argument("-c", "--chronon-ns", type=int, default=60_000_000_000, help="Chronon period in nanoseconds")
    p_serve.add_argument("--persist-path", default=None, help="Persist calendar to this directory")
    p_serve.add_argument("--start-dormant", action="store_true", help="Start in verify-only mode")
    p_serve.add_argument("--peer", action="append", default=[], help="Static peer address (repeatable)")
    p_serve.add_argument("--auto-attest-every-chronons", type=int, default=1, help="Attestation frequency")
    p_serve.add_argument("--request-timeout-secs", type=int, default=5, help="RPC timeout")
    p_serve.add_argument("--p2p-listen", default=None, help="libp2p listen multiaddr")
    p_serve.add_argument("--p2p-port-range", default=None, help="Auto-select port range")
    p_serve.add_argument("--p2p-dial", action="append", default=[], help="libp2p peer to dial (repeatable)")
    p_serve.add_argument("-k", "--known-servers", action="append", default=[], help="Known server IP:PORT (repeatable)")
    p_serve.add_argument("--dht-namespace", default="mainnet", help="DHT namespace")
    p_serve.add_argument("--dht-bootstrap", action="append", default=[], help="DHT bootstrap multiaddr (repeatable)")
    p_serve.add_argument("--max-discovered-peers", type=int, default=13, help="Max peers to discover")

    # ── stamp ────────────────────────────────────────────────
    p_stamp = subparsers.add_parser("stamp", help="Stamp content")
    p_stamp.add_argument("-m", "--message", default=None, help="Message to stamp")
    p_stamp.add_argument("-M", "--message-file", default=None, help="Read message from file")
    p_stamp.add_argument("-o", "--stamp-output", default=None, help="Write output to file")
    p_stamp.add_argument("-s", "--server", default="127.0.0.1:4001", help="Server address")
    p_stamp.add_argument("--echo", default="", help="Echo string")

    # ── verify ───────────────────────────────────────────────
    p_verify = subparsers.add_parser("verify", help="Verify content against a Foretis")
    p_verify.add_argument("-m", "--message", default=None, help="Message to verify")
    p_verify.add_argument("-M", "--message-file", default=None, help="Read message from file")
    p_verify.add_argument("-f", "--foretis", default=None, help="Foretis JSON inline")
    p_verify.add_argument("-F", "--foretis-file", default=None, help="Read Foretis from file")
    p_verify.add_argument("-o", "--verify-output", default=None, help="Write output to file")
    p_verify.add_argument("-s", "--server", default="127.0.0.1:4001", help="Server address")

    # ── prove ────────────────────────────────────────────────
    p_prove = subparsers.add_parser("prove", help="Fetch calendar slice and verify locally")
    p_prove.add_argument("-m", "--message", default=None, help="Message to verify")
    p_prove.add_argument("-M", "--message-file", default=None, help="Read message from file")
    p_prove.add_argument("-f", "--foretis", default=None, help="Foretis JSON inline")
    p_prove.add_argument("-F", "--foretis-file", default=None, help="Read Foretis from file")
    p_prove.add_argument("-o", "--proof-output", default=None, help="Write output to file")
    p_prove.add_argument("-s", "--server", default="127.0.0.1:4001", help="Server address")

    # ── inspect ──────────────────────────────────────────────
    p_inspect = subparsers.add_parser("inspect", help="Inspect external attestations in a calendar")
    p_inspect.add_argument("calendar_path", help="Path to calendar JSON file")

    args = parser.parse_args()

    if not args.command:
        parser.print_help()
        sys.exit(1)

    commands = {
        "serve": cmd_serve,
        "stamp": cmd_stamp,
        "verify": cmd_verify,
        "prove": cmd_prove,
        "inspect": cmd_inspect,
    }

    commands[args.command](args)


if __name__ == "__main__":
    main()
