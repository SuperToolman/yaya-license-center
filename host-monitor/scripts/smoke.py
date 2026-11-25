"""Real Linux/UDS/host/GPU smoke test; does not install Docker or mutate containers."""
import argparse
import http.client
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import tempfile
import time


class UnixHTTP(http.client.HTTPConnection):
    def __init__(self, path):
        super().__init__("localhost", timeout=5)
        self.path = str(path)

    def connect(self):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(self.timeout)
        self.sock.connect(self.path)


def request(path, endpoint, method="GET"):
    connection = UnixHTTP(path)
    try:
        connection.request(method, endpoint)
        response = connection.getresponse()
        return response.status, json.loads(response.read())
    finally:
        connection.close()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--monitor", "--agent", dest="monitor", required=True)
    parser.add_argument("--require-gpu", action="store_true")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="yaya-ops-smoke-") as directory:
        root = Path(directory)
        path = root / "monitor.sock"
        environment = dict(os.environ)
        environment.update(
            YAYA_HOST_MONITOR_SOCKET=str(path),
            YAYA_HOST_MONITOR_DATA_DIR=str(root / "state"),
            YAYA_HOST_MONITOR_ALLOWED_UID=str(os.getuid()),
        )
        report = {"linux_uds": False, "stable_identity": False, "writes_rejected": False}
        first_id = None
        for attempt in range(2):
            process = subprocess.Popen([args.monitor], env=environment, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
            try:
                deadline = time.monotonic() + 10
                while not path.exists():
                    if process.poll() is not None or time.monotonic() > deadline:
                        raise RuntimeError("Host monitor did not start: " + process.stderr.read(2000).decode(errors="replace"))
                    time.sleep(0.05)
                status, hello = request(path, "/v1/hello")
                assert status == 200 and hello["data"]["protocolVersion"] == 1
                node_id = hello["data"]["nodeId"]
                if attempt:
                    assert node_id == first_id, "Node identity changed after restart"
                    report["stable_identity"] = True
                    continue
                first_id = node_id
                report["linux_uds"] = True
                assert request(path, "/v1/executions", "POST")[0] == 405
                report["writes_rejected"] = True
                _, first = request(path, "/v1/snapshot")
                assert first["data"]["host"]["cpuPercent"] is None
                time.sleep(5.2)
                _, result = request(path, "/v1/snapshot")
                metrics = result["data"]
                assert metrics["nodeId"] == node_id
                assert metrics["host"]["memoryTotalBytes"] > 0
                assert metrics["host"]["cpuPercent"] is not None
                assert metrics["sequence"] > first["data"]["sequence"]
                report["host_metrics"] = True
                report["gpu"] = metrics["gpuCapability"]
                report["gpu_models"] = [gpu["name"] for gpu in metrics["gpus"]]
                report["gpu_memory_total_bytes"] = [gpu["memoryTotalBytes"] for gpu in metrics["gpus"]]
                if args.require_gpu:
                    assert metrics["gpuCapability"]["available"] and metrics["gpus"], metrics["gpuCapability"]
                report["docker"] = hello["data"]["docker"]
                _, containers = request(path, "/v1/containers")
                if not hello["data"]["docker"]["available"]:
                    assert containers["error"]["code"] == "DOCKER_UNAVAILABLE"
            finally:
                process.send_signal(signal.SIGINT)
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)
                process.stderr.close()
        print(json.dumps(report, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
