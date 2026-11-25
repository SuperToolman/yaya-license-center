"""Root-only isolated Linux IPC test. Never installs units or touches platform data.

Usage: python3 file_agent_e2e.py /absolute/path/to/test/api-binary
"""
import hashlib
import http.client
import http.cookiejar
import json
import os
from pathlib import Path
import pwd
import signal
import shutil
import socket
try:
    import sqlite3
except ImportError:
    sqlite3 = None
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request


def main():
    assert os.geteuid() == 0, "root is required for the isolated permission test"
    binary = str(Path(sys.argv[1]).resolve())
    account = pwd.getpwnam("yaya-operation-center")
    with tempfile.TemporaryDirectory(prefix="yaya-file-agent-test-", dir="/tmp") as folder:
        root = Path(folder)
        root.chmod(0o755)
        executable = root / "test-api"
        shutil.copyfile(binary, executable)
        executable.chmod(0o755)
        binary = str(executable)
        data = root / "api-data"
        data.mkdir()
        os.chown(data, account.pw_uid, account.pw_gid)
        managed = root / "managed"
        managed.mkdir(mode=0o700)
        (managed / "protected.txt").write_text("root-owned-file\n")
        (managed / "nested").mkdir(mode=0o700)
        (managed / "nested/content").write_bytes(b"nested-content")
        (managed / "escape").symlink_to(root / "api-data")
        ipc = root / "ipc"
        ipc.mkdir(mode=0o750)
        os.chown(ipc, 0, account.pw_gid)
        sock = str(ipc / "files.sock")
        # Verify the non-root API cannot read the target directly.
        assert subprocess.run(["runuser", "-u", account.pw_name, "--", "test", "-r", str(managed / "protected.txt")]).returncode != 0
        key = root / "private.pem"
        public = root / "public.pem"
        subprocess.run(["openssl", "genrsa", "-out", str(key), "2048"], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        subprocess.run(["openssl", "rsa", "-in", str(key), "-pubout", "-out", str(public)], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        reserved = socket.socket()
        reserved.bind(("127.0.0.1", 0))
        port = reserved.getsockname()[1]
        reserved.close()
        base = f"http://127.0.0.1:{port}"
        origin = "http://localhost:8778"
        password = os.urandom(24).hex()
        env = {key: value for key, value in os.environ.items() if not key.startswith(("YAYA_", "LICENSE_", "ALIYUN_"))}
        env.update({"YAYA_FILE_AGENT_SOCKET": sock, "YAYA_FILE_AGENT_ALLOWED_UID": str(account.pw_uid), "YAYA_FILES_ROOT": str(managed), "YAYA_OPERATION_CENTER_WEB_ORIGIN": origin})
        api_env = dict(env, LICENSE_SIGNING_PRIVATE_KEY_PEM=key.read_text(), LICENSE_SIGNING_PUBLIC_KEY_PEM=public.read_text(),
                       YAYA_OPERATION_CENTER_INITIAL_ADMIN_PASSWORD=password, YAYA_OPERATION_CENTER_INITIAL_ADMIN_USERNAME="admin",
                       YAYA_OPERATION_CENTER_DB_PATH=str(data / "test.sqlite3"), YAYA_OPERATION_CENTER_HOST="127.0.0.1",
                       YAYA_OPERATION_CENTER_PORT=str(port), YAYA_OPS_ENABLED="false")
        logs = open(root / "test.log", "w+")
        processes = []
        try:
            agent = subprocess.Popen([binary, "--file-agent"], env=env, stdout=logs, stderr=logs, start_new_session=True,
                                     preexec_fn=lambda: os.setgid(account.pw_gid))
            processes.append(agent)
            deadline = time.monotonic() + 20
            while not Path(sock).exists():
                assert agent.poll() is None, "file agent exited"
                assert time.monotonic() < deadline, "file agent startup timed out"
                time.sleep(0.1)
            # Root is not the allowed API peer, despite being able to open the socket.
            peer = socket.socket(socket.AF_UNIX)
            peer.settimeout(3)
            peer.connect(sock)
            try:
                peer.sendall(b"GET /v1/health HTTP/1.1\r\nHost: local\r\n\r\n")
                assert peer.recv(4096) == b"", "unexpected root peer acceptance"
            except (BrokenPipeError, ConnectionResetError):
                pass
            finally:
                peer.close()
            # The trusted UID still needs a per-request internal context for file routes.
            probe = "import socket; s=socket.socket(socket.AF_UNIX); s.connect(%r); s.sendall(b'GET /api/ops/local/files HTTP/1.1\\r\\nHost: local\\r\\nConnection: close\\r\\n\\r\\n'); print(s.recv(4096).decode())" % sock
            raw = subprocess.check_output(["runuser", "-u", account.pw_name, "--", sys.executable, "-c", probe], text=True)
            assert "403 Forbidden" in raw
            api = subprocess.Popen(["runuser", "-u", account.pw_name, "--", binary], env=api_env, cwd=data, stdout=logs, stderr=logs, start_new_session=True)
            processes.append(api)
            client = urllib.request.build_opener(urllib.request.ProxyHandler({}), urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))

            def request(path, body=None, method=None, expected=200, with_origin=True, content_type="application/json"):
                if body is not None and not isinstance(body, bytes):
                    body = json.dumps(body).encode()
                headers = {"Content-Type": content_type}
                if with_origin:
                    headers["Origin"] = origin
                req = urllib.request.Request(base + path, data=body, headers=headers, method=method)
                try:
                    with client.open(req, timeout=30) as response:
                        status, payload = response.status, response.read()
                except urllib.error.HTTPError as error:
                    status, payload = error.code, error.read()
                assert status == expected, (path, status, payload[:1000])
                return payload

            deadline = time.monotonic() + 25
            while True:
                try:
                    request("/healthz")
                    break
                except urllib.error.URLError:
                    assert api.poll() is None and time.monotonic() < deadline, "test API failed to start"
                    time.sleep(0.2)
            request("/api/ops/local/files", expected=401)
            request("/api/session", {"username": "admin", "password": password})
            listing = json.loads(request("/api/ops/local/files"))["data"]
            assert any(item["name"] == "protected.txt" for item in listing["items"])
            preview = json.loads(request("/api/ops/local/files/preview?path=/protected.txt"))["data"]["text"]
            assert preview == "root-owned-file\n"
            request("/api/ops/local/files?path=../", expected=400)
            request("/api/ops/local/files/preview?path=/escape/test.sqlite3", expected=400)
            request("/api/ops/local/files/mutate", {"action": "create", "path": "/", "name": "no-origin"}, with_origin=False, expected=403)
            request("/api/ops/local/files/mutate", {"action": "exec", "path": "/", "name": "x"}, expected=400)
            request("/api/ops/local/files/mutate", {"action": "mkdir", "path": "/", "name": "new"})
            request("/api/ops/local/files/mutate", {"action": "create", "path": "/new", "name": "empty"})
            boundary = "yaya-file-agent-test-boundary"
            content = os.urandom(2 * 1024 * 1024)
            multipart = f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="binary.bin"\r\nContent-Type: application/octet-stream\r\n\r\n'.encode() + content + f"\r\n--{boundary}--\r\n".encode()
            request("/api/ops/local/files/upload?path=/new", multipart, content_type=f"multipart/form-data; boundary={boundary}")
            downloaded = request("/api/ops/local/files/download?path=/new/binary.bin")
            assert hashlib.sha256(downloaded).digest() == hashlib.sha256(content).digest()
            assert json.loads(request("/api/ops/local/files/download?path=/new/binary.bin&check=true"))["data"]["readable"]
            size = json.loads(request("/api/ops/local/files/size?path=/new"))["data"]
            assert size["bytes"] == len(content)
            listing = json.loads(request("/api/ops/local/files?path=/new"))["data"]["items"]
            item = next(entry for entry in listing if entry["name"] == "binary.bin")
            request("/api/ops/local/files/mutate", {"action": "rename", "path": item["path"], "name": "renamed.bin", "expectedRevision": item["revision"]})
            props = json.loads(request("/api/ops/local/files/properties?path=/new/renamed.bin"))["data"]
            assert props["size"] == len(content)
            listing = json.loads(request("/api/ops/local/files?path=/new"))["data"]["items"]
            for item in listing:
                request("/api/ops/local/files/mutate", {"action": "delete", "path": item["path"], "expectedRevision": item["revision"]})
            # Keep audit in the API database; the root process never opens it.
            query = "SELECT action,actor_user_id,resource_path FROM ops_audit_events WHERE action LIKE 'files.agent.%'"
            if sqlite3:
                with sqlite3.connect(data / "test.sqlite3") as db:
                    rows = db.execute(query).fetchall()
            else:
                # The bundled CentOS 7 Python may lack _sqlite3; the installed
                # Node 22 runtime includes a modern SQLite reader.
                code = "const {DatabaseSync}=require('node:sqlite');const d=new DatabaseSync(process.argv[1],{readOnly:true});console.log(JSON.stringify(d.prepare(process.argv[2]).all().map(r=>[r.action,r.actor_user_id,r.resource_path])));d.close();"
                rows = json.loads(subprocess.check_output(["node", "--no-warnings", "-e", code, str(data / "test.sqlite3"), query], text=True))
            assert any(row[0] == "files.agent.rename.completed" and row[1] for row in rows)
            assert any(row[0] == "files.agent.upload.completed" for row in rows)
            request("/api/users", {"username": "finance-test", "password": password, "displayName": "test", "role": "platform_finance", "organizationId": "test"})
            admin_client = client
            client = urllib.request.build_opener(urllib.request.ProxyHandler({}), urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))
            request("/api/session", {"username": "finance-test", "password": password})
            request("/api/ops/local/files", expected=403)
            client = admin_client
            # An unavailable configured agent must fail closed, not use local API permissions.
            agent.terminate()
            agent.wait(timeout=10)
            failure = json.loads(request("/api/ops/local/files", expected=503))
            assert failure["error"]["code"] == "FILE_AGENT_UNAVAILABLE"
            print(json.dumps({"status": "passed", "checks": ["non-root API reads root-only file through IPC", "peer UID rejection", "request identity rejection", "admin authentication", "origin enforcement", "path containment", "fixed operation allowlist", "root-owned directory creation", "2 MiB streamed upload/download hash", "directory size", "rename/properties/delete", "API database audit", "agent outage fails closed"], "productionServicesChanged": False}))
        except Exception:
            logs.flush()
            logs.seek(0)
            print(logs.read()[-16000:], file=sys.stderr)
            raise
        finally:
            for process in reversed(processes):
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGTERM)
                    try:
                        process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        os.killpg(process.pid, signal.SIGKILL)
                        process.wait()
            logs.close()


if __name__ == "__main__":
    main()
