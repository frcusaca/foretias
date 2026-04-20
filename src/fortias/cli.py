"""Fortias CLI — ``fortis`` command-line tool."""

from __future__ import annotations

import argparse
import json
import sys


def _hex_to_bytes(value: str) -> bytes:
    return bytes.fromhex(value)


def _load_fortis(path: str):
    """Load a Fortis from a JSON file."""
    data = json.loads(open(path).read())
    from fortias.models import Fortis

    return Fortis(
        tick_number=data["tick_number"],
        my_content_hash=_hex_to_bytes(data["my_content_hash"]),
        signature=_hex_to_bytes(data["signature"]),
        tbid=_hex_to_bytes(data["tbid"]),
        echo=data.get("echo", ""),
        tbn=data.get("tbn", ""),
    )


def _load_calendar(path: str):
    """Load a Calendar from a JSON file."""
    from fortias.calendar import Calendar

    return Calendar.load(path)


def main(argv: list[str] | None = None) -> int:
    """CLI entry point."""
    parser = argparse.ArgumentParser(
        prog="fortis",
        description="Fortias — Time Integrity Attestation",
    )
    sub = parser.add_subparsers(dest="command")

    verify_parser = sub.add_parser("verify", help="Verify a Fortis artifact")
    verify_parser.add_argument(
        "--calendar", required=True, help="Path to calendar.json"
    )
    verify_parser.add_argument(
        "--fortis", required=True, help="Path to fortis.json"
    )
    verify_parser.add_argument(
        "--file", required=True, help="Path to the message file"
    )
    verify_parser.add_argument(
        "--chain",
        action="store_true",
        default=False,
        help="Run chain integrity check on the calendar",
    )

    args = parser.parse_args(argv)

    if args.command is None:
        parser.print_help()
        return 1

    if args.command == "verify":
        # Load calendar
        calendar = _load_calendar(args.calendar)

        # Optional chain check
        if args.chain:
            chain_ok = calendar.integrity_check()
            if not chain_ok:
                print("chain: INVALID")
                return 1
            print("chain: valid")

        # Load Fortis
        fortis = _load_fortis(args.fortis)

        # Read message content
        content = open(args.file, "rb").read()

        # Verify
        result = fortias_verify(content, fortis, calendar)
        if result:
            print("valid")
            return 0
        else:
            print("invalid")
            return 1

    return 0


def fortias_verify(content, fortis, calendar):
    """Verify a Fortis against content and calendar."""
    from fortias._timebeing import _timebeing

    return _timebeing._verify(
        content=content,
        fortis=fortis,
        calendar_ticks=calendar.ticks,
    )


if __name__ == "__main__":
    sys.exit(main())
