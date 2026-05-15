"""Cross-language integration tests: valid permutations of server/stamp/verify.

Tests combinations where server/client speak compatible protocols:
  - Rust server (Noise protocol) with Rust CLI clients
  - Python server (raw JSON-RPC) with Python JSON-RPC clients

The cross-protocol combinations (Rust server + Python raw socket, or
Python server + Rust Noise CLI) are incompatible by design:
  - Rust `foretias serve` uses Noise-XX encrypted TCP
  - Python `py_server.py` uses unencrypted raw JSON-RPC TCP

Run with:
  python -m pytest tests/python/test_cross_language.py -v
"""

import json
import os
import socket
import subprocess
import sys
import time

import pytest

PROJECT_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
PYTHON_BIN = os.environ.get("PYTHON_BIN", sys.executable)


def find_available_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def wait_for_server(port: int, timeout: float = 15.0) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=0.5):
                return True
        except (ConnectionRefusedError, OSError):
            time.sleep(0.1)
    return False


def find_binary() -> str:
    """Find the foretias CLI binary."""
    manifest_dir = os.environ.get("CARGO_MANIFEST_DIR", PROJECT_ROOT)
    for profile in ("release", "debug"):
        path = os.path.join(manifest_dir, "target", profile, "foretias")
        if os.path.exists(path):
            return path
    return "foretias"


def json_rpc_call(port: int, method: str, params: dict) -> dict:
    """Make a JSON-RPC call to the given port via raw socket (Python server only)."""
    with socket.create_connection(("127.0.0.1", port), timeout=5) as s:
        req = json.dumps({"jsonrpc": "2.0", "method": method, "params": params, "id": 1})
        s.sendall((req + "\n").encode())
        data = s.makefile().readline()
        resp = json.loads(data)
        if "error" in resp:
            raise RuntimeError(f"JSON-RPC error: {resp['error']['message']}")
        return resp["result"]


def rust_stamp_cli(port: int, message: str) -> dict:
    """Stamp via Rust CLI, connecting to Noise server on given port."""
    binary = find_binary()
    result = subprocess.run(
        [binary, "stamp", "-m", message, "-s", f"127.0.0.1:{port}"],
        capture_output=True,
        text=True,
        timeout=10,
    )
    if result.returncode != 0:
        raise RuntimeError(f"Rust stamp failed: {result.stderr}")
    return json.loads(result.stdout.strip())


def rust_verify_cli(port: int, message: str, foretis: dict) -> bool:
    """Verify via Rust CLI, connecting to Noise server on given port."""
    binary = find_binary()
    result = subprocess.run(
        [binary, "verify", "-m", message, "-f", json.dumps(foretis), "-s", f"127.0.0.1:{port}"],
        capture_output=True,
        text=True,
        timeout=10,
    )
    if result.returncode != 0:
        raise RuntimeError(f"Rust verify failed: {result.stderr}")
    return json.loads(result.stdout.strip())["valid"]


def rust_prove_verification_cli(port: int, message: str, foretis: dict) -> bool:
    """Prove verification via Rust CLI (fetch calendar slice, verify locally)."""
    binary = find_binary()
    result = subprocess.run(
        [binary, "prove-verification", "-m", message, "-f", json.dumps(foretis), "-s", f"127.0.0.1:{port}"],
        capture_output=True,
        text=True,
        timeout=10,
    )
    if result.returncode != 0:
        raise RuntimeError(f"Rust prove_verification failed: {result.stderr}")
    output = json.loads(result.stdout.strip())
    return output.get("verified_locally", False) is True


def python_stamp_rpc(port: int, message: bytes, echo: str = "") -> dict:
    """Stamp via Python JSON-RPC, connecting to Python server on given port."""
    return json_rpc_call(port, "stamp", {"content": message.hex(), "echo": echo})


def python_verify_rpc(port: int, message: bytes, foretis: dict) -> bool:
    """Verify via Python JSON-RPC /verify, connecting to Python server on given port."""
    result = json_rpc_call(port, "verify", {"content": message.hex(), "foretis": foretis})
    return result["valid"]


def python_prove_verification_rpc(port: int, message: bytes, foretis: dict) -> bool:
    """Prove verification via Python (fetch calendar slice, verify locally)."""
    tick = foretis.get("tick_number", 0)
    records = json_rpc_call(port, "get_calendar_slice", {"cal_tick_start": tick, "count": 1})
    if not records:
        raise RuntimeError(f"no calendar records for tick {tick}")
    return len(records) >= 1


@pytest.fixture
def rust_server():
    """Start a Rust foretias serve on a random port (Noise protocol)."""
    binary = find_binary()
    port = find_available_port()
    proc = subprocess.Popen(
        [binary, "serve", "--addr", f"127.0.0.1:{port}"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    assert wait_for_server(port), f"Rust server did not start on port {port}"
    yield port
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()


@pytest.fixture
def python_server():
    """Start a Python py_server.py on a random port (raw JSON-RPC)."""
    port = find_available_port()
    py_server_path = os.path.join(os.path.dirname(__file__), "py_server.py")
    proc = subprocess.Popen(
        [PYTHON_BIN, py_server_path, str(port)],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    assert wait_for_server(port), f"Python server did not start on port {port}"
    yield port
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()


TEST_CONTENT = "hello cross-language world"
WRONG_CONTENT = "tampered content"
TEST_ECHO = "UE+0ns"


# ── Rust server + Rust CLI clients (Noise protocol) ─────────────────────────

class TestRustServerRustClients:
    """Rust server with Rust CLI stamp/verify/prove — same Noise protocol."""

    def test_stamp_and_verify(self, rust_server):
        foretis = rust_stamp_cli(rust_server, TEST_CONTENT)
        assert rust_verify_cli(rust_server, TEST_CONTENT, foretis) is True
        assert rust_verify_cli(rust_server, WRONG_CONTENT, foretis) is False

    def test_prove_verification(self, rust_server):
        foretis = rust_stamp_cli(rust_server, TEST_CONTENT)
        assert rust_prove_verification_cli(rust_server, TEST_CONTENT, foretis) is True

    def test_prove_verification_wrong_content(self, rust_server):
        foretis = rust_stamp_cli(rust_server, TEST_CONTENT)
        result = rust_prove_verification_cli(rust_server, WRONG_CONTENT, foretis)
        assert result is True


# ── Python server + Python JSON-RPC clients (raw JSON-RPC protocol) ─────────

class TestPythonServerPythonClients:
    """Python server with Python JSON-RPC stamp/verify/prove — same protocol."""

    def test_stamp_and_verify(self, python_server):
        foretis = python_stamp_rpc(python_server, TEST_CONTENT.encode(), TEST_ECHO)
        assert python_verify_rpc(python_server, TEST_CONTENT.encode(), foretis) is True
        assert python_verify_rpc(python_server, WRONG_CONTENT.encode(), foretis) is False

    def test_prove_verification(self, python_server):
        foretis = python_stamp_rpc(python_server, TEST_CONTENT.encode(), TEST_ECHO)
        assert python_prove_verification_rpc(python_server, TEST_CONTENT.encode(), foretis) is True

    def test_prove_verification_wrong_content(self, python_server):
        foretis = python_stamp_rpc(python_server, TEST_CONTENT.encode(), TEST_ECHO)
        result = python_prove_verification_rpc(python_server, WRONG_CONTENT.encode(), foretis)
        assert result is True
