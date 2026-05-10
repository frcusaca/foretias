#!/usr/bin/env python3
"""Simple Python TimeFamilyServer for cross-language integration tests."""
import sys
import json
import socket
import threading
from foretias_p2p import PyTimeFamily, PyForetis


def run_server(port: int) -> None:
    tf = PyTimeFamily(tbn="python-test-server")
    server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server.bind(("127.0.0.1", port))
    server.listen(5)

    print(f"Foretias Python TimeFamilyServer starting...")
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
                    foretis = tf.stamp(content, echo)
                    result = json.loads(foretis.to_json())
                    resp = {"jsonrpc": "2.0", "result": result, "id": req_id}
                elif method == "verify":
                    content = bytes.fromhex(params.get("content", ""))
                    foretis_json = params.get("foretis", {})
                    pf = PyForetis.from_json(json.dumps(foretis_json))
                    valid = tf.verify(content, pf)
                    resp = {"jsonrpc": "2.0", "result": {"valid": valid}, "id": req_id}
                elif method == "get_calendar_slice":
                    cal_tick_start = params.get("cal_tick_start", 0)
                    count = params.get("count", 10)
                    cal = tf.get_calendar()
                    # Filter and slice ticks
                    records = []
                    for i in range(cal.tick_count()):
                        t = cal.ticks[i]
                        if t.tick_number >= cal_tick_start:
                            records.append({
                                "tick_number": t.tick_number,
                                "public_key": list(t.public_key),
                                "forward_foretis": list(t.forward_foretis),
                                "backward_foretis": list(t.backward_foretis),
                            })
                            if len(records) >= count:
                                break
                    resp = {"jsonrpc": "2.0", "result": records, "id": req_id}
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
