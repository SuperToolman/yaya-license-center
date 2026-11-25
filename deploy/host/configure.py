"""Render native service configuration; never execute or shell-source an env file."""
import argparse
import os
from pathlib import Path
import re
import shlex
import subprocess
from urllib.parse import urlsplit

UNITS = (
    "yaya-operation-center.target",
    "yaya-operation-center-api.service",
    "yaya-operation-center-web.service",
    "yaya-host-monitor.service",
    "yaya-file-agent.service",
)


def installation_root(value):
    path = Path(value)
    if not re.fullmatch(r"/(opt|srv)/[A-Za-z0-9][A-Za-z0-9._/-]*", value):
        raise ValueError("Installation root must be an application directory under /opt or /srv")
    if str(path.resolve()) != value or any(part in (".", "..") for part in value.split("/")):
        raise ValueError("Installation root must be canonical and must not traverse symlinks")
    for child in ("shared", "shared/data", "shared/secrets", "shared/monitor", "releases", "backups"):
        candidate = path / child
        if candidate.resolve() != candidate:
            raise ValueError(f"Managed path must not traverse symlinks: {child}")
    return path


def read_env(path):
    values = {}
    if not path.is_file():
        return values
    for number, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        key, separator, value = line.partition("=")
        if not separator or not re.fullmatch(r"[A-Z][A-Z0-9_]*", key):
            raise ValueError(f"Invalid environment entry at line {number}")
        if value.startswith(('"', "'")):
            parts = shlex.split(value, comments=False)
            if len(parts) != 1:
                raise ValueError(f"Invalid quoted environment entry at line {number}")
            value = parts[0]
        if any(char in value for char in ("\0", "\n", "\r")):
            raise ValueError(f"Invalid environment value at line {number}")
        values[key] = value
    return values


def origin(value):
    parsed = urlsplit(value)
    if (parsed.scheme not in ("http", "https") or not parsed.hostname
            or parsed.username is not None or parsed.password is not None
            or parsed.path not in ("", "/") or parsed.query or parsed.fragment
            or any(char in value for char in ("\n", "\r", "\0"))):
        raise ValueError("Expected an HTTP(S) origin without credentials or a path")
    parsed.port  # Validate a malformed port before producing service configuration.
    return value.rstrip("/")


def render(args):
    root = installation_root(args.root)
    output = Path(args.output)
    output.mkdir(parents=True, exist_ok=True)
    values = read_env(Path(__file__).parent / "operation-center.env.example")
    if args.legacy_container:
        import json
        inspected = json.loads(subprocess.check_output([args.docker, "--host", "unix:///var/run/docker.sock", "inspect", args.legacy_container], text=True))[0]
        for entry in inspected["Config"].get("Env", []):
            key, separator, value = entry.partition("=")
            if separator and (key.startswith("ALIYUN_") or key in ("YAYA_OPERATION_CENTER_WEB_ORIGIN", "YAYA_PLATFORM_LICENSE_CENTER_URL", "YAYA_OPERATION_CENTER_INITIAL_ADMIN_USERNAME")):
                values[key] = value
    values.update(read_env(root / "shared/operation-center.env"))
    values.update({
        "YAYA_OPERATION_CENTER_HOST": "0.0.0.0",
        "YAYA_OPERATION_CENTER_PORT": str(args.api_port),
        "YAYA_OPERATION_CENTER_DB_PATH": str(root / "shared/data/operation-center.sqlite3"),
        "YAYA_OPERATION_CENTER_RELEASE_DIR": str(root / "current"),
        "YAYA_OPERATION_CENTER_SECRETS_DIR": str(root / "shared/secrets"),
        "YAYA_OPERATION_CENTER_SKILLS_DIR": str(root / "shared/data/runtime/skills"),
        "YAYA_PLATFORM_PACKAGES_DIR": str(root / "shared/data/platform-packages"),
        "YAYA_OPERATION_CENTER_WEB_ORIGIN": origin(args.web_origin or values.get("YAYA_OPERATION_CENTER_WEB_ORIGIN") or args.default_origin),
        "YAYA_PLATFORM_LICENSE_CENTER_URL": origin(args.license_api or values.get("YAYA_PLATFORM_LICENSE_CENTER_URL") or args.default_license_api),
        "YAYA_PLATFORM_DOCKER_CLI": args.docker,
        "YAYA_BACKUP_HELPER_IMAGE": args.helper_image,
        "YAYA_OPS_ENABLED": "true",
        "YAYA_HOST_MONITOR_SOCKET": "/run/yaya-host-monitor/monitor.sock",
        "YAYA_HOST_MONITOR_DATA_DIR": str(root / "shared/monitor"),
        "YAYA_HOST_MONITOR_ALLOWED_UID": str(args.uid),
        "YAYA_OPS_DOCKER_SOCKET": "/var/run/docker.sock",
        "YAYA_FILE_AGENT_SOCKET": "/run/yaya-file-agent/files.sock",
        "YAYA_FILE_AGENT_ALLOWED_UID": str(args.uid),
        "NODE_ENV": "production",
        "PORT": str(args.web_port),
    })
    # Legacy container settings have no role in the host deployment.
    for key in ("CONTAINER_NAME", "WEB_PORT", "API_PORT", "WEB_ORIGIN", "YAYA_OPS_AGENT_SOCKET", "YAYA_OPS_AGENT_DATA_DIR", "YAYA_OPS_AGENT_ALLOWED_UID"):
        values.pop(key, None)
    lines = []
    for key, value in sorted(values.items()):
        quoted = value.replace("\\", "\\\\").replace('"', '\\"')
        lines.append(f'{key}="{quoted}"\n')
    env_file = output / "operation-center.env"
    env_file.write_text("".join(lines), encoding="utf-8")
    os.chmod(env_file, 0o600)
    # The privileged file process needs no SMS credentials, database location or signing keys.
    file_values = {key: values[key] for key in (
        "YAYA_FILE_AGENT_SOCKET", "YAYA_FILE_AGENT_ALLOWED_UID", "YAYA_OPERATION_CENTER_WEB_ORIGIN",
    )}
    file_values["YAYA_FILES_ROOT"] = values.get("YAYA_FILES_ROOT") or "/"
    file_env = output / "file-agent.env"
    file_lines = []
    for key, value in sorted(file_values.items()):
        quoted = value.replace("\\", "\\\\").replace('"', '\\"')
        file_lines.append(f'{key}="{quoted}"\n')
    file_env.write_text("".join(file_lines), encoding="utf-8")
    os.chmod(file_env, 0o600)
    substitutions = {"@ROOT@": str(root), "@NODE@": args.node, "@DOCKER_GROUP@": args.docker_group}
    for name in UNITS:
        text = (Path(__file__).parent / "systemd" / name).read_text(encoding="utf-8")
        for key, value in substitutions.items():
            if any(char in value for char in ("\n", "\r", "%", " ", '"', "'")):
                raise ValueError("Unsupported systemd substitution")
            text = text.replace(key, value)
        (output / name).write_text(text, encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    validate = commands.add_parser("validate-root")
    validate.add_argument("root")
    get = commands.add_parser("get")
    get.add_argument("file", type=Path)
    get.add_argument("key")
    build = commands.add_parser("render")
    for name in ("root", "output", "docker", "node", "docker-group", "helper-image", "default-origin", "default-license-api"):
        build.add_argument("--" + name, required=True)
    for name in ("web-origin", "license-api"):
        build.add_argument("--" + name, default="")
    build.add_argument("--legacy-container", default="")
    for name in ("web-port", "api-port", "uid"):
        build.add_argument("--" + name, required=True, type=int)
    args = parser.parse_args()
    if args.command == "validate-root":
        print(installation_root(args.root))
    elif args.command == "get":
        print(read_env(args.file).get(args.key, ""))
    else:
        if not (1 <= args.web_port <= 65535 and 1 <= args.api_port <= 65535) or args.web_port == args.api_port:
            raise ValueError("Invalid or conflicting service ports")
        if args.uid <= 0:
            raise ValueError("Services must use an unprivileged UID")
        render(args)


if __name__ == "__main__":
    main()
