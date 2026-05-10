"""Foretias CLI — ``foretis`` command-line tool (Rust-backed)."""
from __future__ import annotations

import argparse
import json
import sys

from foretias_p2p import PyTimeFamilyServer, PyForetis


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="foretis", description="Foretias CLI (Rust-backed)")
    sub = parser.add_subparsers(dest="command")

    # stamp
    stamp_p = sub.add_parser("stamp", help="Stamp content")
    stamp_p.add_argument("-m", "--message", help="Message text")
    stamp_p.add_argument("-M", "--message-file", help="Read message from file")
    stamp_p.add_argument("-o", "--output", help="Write Foretis JSON to file")
    stamp_p.add_argument("--persist-path", help="Calendar persistence directory")
    stamp_p.add_argument("--echo", default="", help="Echo field")

    # verify
    verify_p = sub.add_parser("verify", help="Verify a Foretis")
    verify_p.add_argument("-m", "--message", help="Message text")
    verify_p.add_argument("-M", "--message-file", help="Read message from file")
    verify_p.add_argument("-f", "--foretis", help="Foretis JSON (inline or file path)")
    verify_p.add_argument("-F", "--foretis-file", help="Load Foretis from file")
    verify_p.add_argument("--persist-path", help="Calendar persistence directory (dormant mode)")

    # integrity
    integrity_p = sub.add_parser("integrity", help="Chain integrity check")
    integrity_p.add_argument("--start", type=int, default=None, help="Start tick")
    integrity_p.add_argument("--end", type=int, default=None, help="End tick")
    integrity_p.add_argument("--persist-path", required=True, help="Calendar directory (dormant mode)")

    # serve (stub - requires async)
    sub.add_parser("serve", help="Start server (use Rust binary: foretias serve)")

    args = parser.parse_args(argv)
    if args.command is None:
        parser.print_help()
        return 1

    try:
        if args.command == "stamp":
            return cmd_stamp(args)
        elif args.command == "verify":
            return cmd_verify(args)
        elif args.command == "integrity":
            return cmd_integrity(args)
        elif args.command == "serve":
            print("Use the Rust binary: cargo run -- serve", file=sys.stderr)
            return 1
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        return 1

    return 0


def cmd_stamp(args) -> int:
    if args.message_file:
        content = open(args.message_file, "rb").read()
    elif args.message:
        content = args.message.encode()
    else:
        print("Error: provide -m or -M", file=sys.stderr)
        return 1

    server = PyTimeFamilyServer(
        persist_path=getattr(args, "persist_path", None),
    )
    foretis = server.stamp(content, args.echo)
    j = json.loads(foretis.to_json())

    if args.output:
        with open(args.output, "w") as f:
            json.dump(j, f, indent=2)
    else:
        print(json.dumps(j, indent=2))

    return 0


def cmd_verify(args) -> int:
    # Load content
    if getattr(args, "message_file", None):
        content = open(args.message_file, "rb").read()
    elif getattr(args, "message", None):
        content = args.message.encode()
    else:
        print("Error: provide -m or -M", file=sys.stderr)
        return 1

    # Load Foretis
    if getattr(args, "foretis_file", None):
        foretis_json = json.loads(open(args.foretis_file).read())
    else:
        foretis_json = json.loads(args.foretis)

    pf = PyForetis.from_json(json.dumps(foretis_json))

    # Create server in dormant mode (verify only)
    persist_path = getattr(args, "persist_path", None)
    if persist_path:
        server = PyTimeFamilyServer.from_calendar(calendar_path=persist_path)
    else:
        print("Error: --persist-path required for verify", file=sys.stderr)
        return 1

    valid = server.verify(content, pf)
    print(json.dumps({"valid": valid}))
    return 0 if valid else 1


def cmd_integrity(args) -> int:
    server = PyTimeFamilyServer.from_calendar(
        calendar_path=args.persist_path,
    )
    result = json.loads(server.integrity_check(
        start=args.start,
        end=args.end,
    ))
    print(json.dumps(result, indent=2))
    return 0 if result.get("all_valid") else 1


if __name__ == "__main__":
    sys.exit(main())
