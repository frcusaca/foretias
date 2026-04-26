#!/usr/bin/env python3
"""Simple Python TimeFamilyServer for cross-language integration tests."""
import sys
import json
import socket
import threading
from fortias_p2p import PyTimeFamily, PyFortis


def run_server(port: int) -> None:
    tf = PyTimeFamily(tbn="python-test-server")
    server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server.bind(("127.0.0.1", port))
    server.listen(5)

    print(f"Fortias Python TimeFamilyServer starting...")
    print(f"  Listen : 127.0.0.1:{port}")
    print(f"  TBN    : {tf.get_tbn()}")
    print(f"  TBID   : {tf.get_tbid()}")
    sys.stdout.flush()

    while True:
        conn, _addr = server.accept()

        def handle(c):
            try:
                data = b""
                while b"\n" not in data:
                    chunk = c.recv(4096)
                    if not chunk:
                        return
                    data += chunk
                req = json.loads(data.decode().strip())
                method = req.get("method", "")
                params = req.get("params", {})
                req_id = req.get("id")

                if method == "stamp":
                    content = bytes.fromhex(params.get("content", ""))
                    echo = params.get("echo", "")
                    fortis = tf.stamp(content, echo)
                    result = json.loads(fortis.to_json())
                    resp = {"jsonrpc": "2.0", "result": result, "id": req_id}
                elif method == "verify":
                    content = bytes.fromhex(params.get("content", ""))
                    fortis_json = params.get("fortis", {})
                    pf = PyFortis.from_json(json.dumps(fortis_json))
                    valid = tf.verify(content, pf)
                    resp = {"jsonrpc": "2.0", "result": {"valid": valid}, "id": req_id}
                else:
                    resp = {"jsonrpc": "2.0", "error": {"code": -32601, "message": "Method not found"}, "id": req_id}

                c.sendall((json.dumps(resp) + "\n").encode())
            except Exception as e:
                print(f"Error handling request: {e}", file=sys.stderr)
                sys.stderr.flush()
            finally:
                c.close()

        t = threading.Thread(target=handle, args=(conn,))
        t.daemon = True
        t.start()


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 4002
    run_server(port)
