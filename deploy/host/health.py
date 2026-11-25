"""Basic native deployment checks, executed as the platform service account."""
import argparse
import http.client
import json
import socket
import os
import urllib.request


def monitor_snapshot(path):
    connection = http.client.HTTPConnection("localhost", timeout=5)
    stream = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    stream.settimeout(5)
    try:
        stream.connect(path)
        connection.sock = stream
        connection.request("GET", "/v1/snapshot")
        response = connection.getresponse()
        if response.status != 200:
            raise RuntimeError("Host monitor returned an unsuccessful response")
        payload = json.loads(response.read(2 * 1024 * 1024 + 1))
        snapshot = payload.get("data") or {}
        if not snapshot.get("nodeId") or (snapshot.get("host") or {}).get("memoryTotalBytes", 0) <= 0:
            raise RuntimeError("Host monitor did not return real host memory metrics")
    finally:
        connection.close()
        stream.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--api-port", required=True, type=int)
    parser.add_argument("--web-port", required=True, type=int)
    parser.add_argument("--socket", default="/run/yaya-host-monitor/monitor.sock")
    args = parser.parse_args()
    # Do not send these local checks through a configured HTTP proxy.
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    # Authentication is rendered by OperationAuthGate; there is no /login route.
    for port, path in ((args.api_port, "/healthz"), (args.web_port, "/")):
        with opener.open(f"http://127.0.0.1:{port}{path}", timeout=5) as response:
            if response.status != 200:
                raise RuntimeError("Service health check failed")
    with opener.open(f"http://127.0.0.1:{args.web_port}/operation-api/healthz", timeout=5) as response:
        if response.status != 200 or json.load(response).get("code") != 0:
            raise RuntimeError("Management UI cannot proxy to the native API")
    monitor_snapshot(args.socket)
    path = os.environ.get("YAYA_FILE_AGENT_SOCKET")
    if path:
        connection = http.client.HTTPConnection("localhost", timeout=5)
        stream = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        stream.settimeout(5)
        try:
            stream.connect(path)
            connection.sock = stream
            connection.request("GET", "/v1/health")
            response = connection.getresponse()
            payload = json.loads(response.read(65536))
            if response.status != 200 or (payload.get("data") or {}).get("protocol") != "1":
                raise RuntimeError("File agent protocol or health check failed")
        finally:
            connection.close()
            stream.close()


if __name__ == "__main__":
    main()
