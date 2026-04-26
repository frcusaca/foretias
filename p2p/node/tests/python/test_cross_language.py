"""Cross-language integration tests: 8 permutations of server/stamp/verify.

Tests all combinations of:
  Server:  Rust CLI (fortias serve) or Python (py_server.py)
  Stamp:   Rust CLI (fortias stamp) or Python (JSON-RPC via socket)
  Verify:  Rust CLI (fortias verify) or Python (JSON-RPC via socket)

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

PROJECT_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
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
    """Find the fortias CLI binary."""
    manifest_dir = os.environ.get("CARGO_MANIFEST_DIR", PROJECT_ROOT)
    for profile in ("release", "debug"):
        path = os.path.join(manifest_dir, "target", profile, "fortias")
        if os.path.exists(path):
            return path
    return "fortias"


def json_rpc_call(port: int, method: str, params: dict) -> dict:
    """Make a JSON-RPC call to the given port via raw socket."""
    with socket.create_connection(("127.0.0.1", port), timeout=5) as s:
        req = json.dumps({"jsonrpc": "2.0", "method": method, "params": params, "id": 1})
        s.sendall((req + "\n").encode())
        data = s.makefile().readline()
        resp = json.loads(data)
        if "error" in resp:
            raise RuntimeError(f"JSON-RPC error: {resp['error']['message']}")
        return resp["result"]


def rust_stamp_cli(port: int, message: str) -> dict:
    """Stamp via Rust CLI, connecting to server on given port."""
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


def rust_verify_cli(port: int, message: str, fortis: dict) -> bool:
    """Verify via Rust CLI, connecting to server on given port."""
    binary = find_binary()
    result = subprocess.run(
        [binary, "verify", "-m", message, "-f", json.dumps(fortis), "-s", f"127.0.0.1:{port}"],
        capture_output=True,
        text=True,
        timeout=10,
    )
    if result.returncode != 0:
        raise RuntimeError(f"Rust verify failed: {result.stderr}")
    return json.loads(result.stdout.strip())["valid"]


def python_stamp_rpc(port: int, message: bytes, echo: str = "") -> dict:
    """Stamp via Python JSON-RPC, connecting to server on given port."""
    return json_rpc_call(port, "stamp", {"content": message.hex(), "echo": echo})


def python_verify_rpc(port: int, message: bytes, fortis: dict) -> bool:
    """Verify via Python JSON-RPC, connecting to server on given port."""
    result = json_rpc_call(port, "verify", {"content": message.hex(), "fortis": fortis})
    return result["valid"]


@pytest.fixture
def rust_server():
    """Start a Rust fortias serve on a random port."""
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
    """Start a Python py_server.py on a random port."""
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


@pytest.mark.parametrize(
    "server_type,stamp_type,verify_type",
    [
        ("rust", "rust", "rust"),
        ("rust", "rust", "python"),
        ("rust", "python", "rust"),
        ("rust", "python", "python"),
        ("python", "rust", "rust"),
        ("python", "rust", "python"),
        ("python", "python", "rust"),
        ("python", "python", "python"),
    ],
    ids=[
        "rust_srv_rust_stamp_rust_verify",
        "rust_srv_rust_stamp_py_verify",
        "rust_srv_py_stamp_rust_verify",
        "rust_srv_py_stamp_py_verify",
        "py_srv_rust_stamp_rust_verify",
        "py_srv_rust_stamp_py_verify",
        "py_srv_py_stamp_rust_verify",
        "py_srv_py_stamp_py_verify",
    ],
)
def test_cross_language(server_type, stamp_type, verify_type, request):
    port = request.getfixturevalue(f"{server_type}_server")

    # Stamp
    if stamp_type == "rust":
        fortis = rust_stamp_cli(port, TEST_CONTENT)
    else:
        fortis = python_stamp_rpc(port, TEST_CONTENT.encode(), TEST_ECHO)

    # Verify correct content
    if verify_type == "rust":
        assert rust_verify_cli(port, TEST_CONTENT, fortis) is True
    else:
        assert python_verify_rpc(port, TEST_CONTENT.encode(), fortis) is True

    # Verify wrong content
    if verify_type == "rust":
        assert rust_verify_cli(port, WRONG_CONTENT, fortis) is False
    else:
        assert python_verify_rpc(port, WRONG_CONTENT.encode(), fortis) is False
