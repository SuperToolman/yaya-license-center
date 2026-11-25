"""Configuration compatibility checks for native initialization and upgrades."""
import argparse
import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

HOST = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("native_configure", HOST / "configure.py")
configure = importlib.util.module_from_spec(spec)
spec.loader.exec_module(configure)


class EnvironmentTests(unittest.TestCase):
    def test_environment_values_are_parsed_without_shell_execution(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "config.env"
            path.write_text('TOKEN="$(do-not-execute) & `literal` $HOME"\nEMPTY=""\nREGION=cn-hangzhou\n', encoding="utf-8")
            values = configure.read_env(path)
            self.assertEqual(values["TOKEN"], "$(do-not-execute) & `literal` $HOME")
            self.assertEqual(values["EMPTY"], "")
            self.assertEqual(values["REGION"], "cn-hangzhou")

    def test_malformed_environment_is_rejected(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "config.env"
            path.write_text('INVALID KEY=secret\n', encoding="utf-8")
            with self.assertRaises(ValueError):
                configure.read_env(path)

    def test_origins_cannot_contain_credentials_or_paths(self):
        for value in ("https://admin:password@example.com", "https://example.com/api", "file:///tmp/key", "https://example.com:invalid"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                configure.origin(value)
        self.assertEqual(configure.origin("https://[::1]:8779/"), "https://[::1]:8779")


@unittest.skipUnless(os.name == "posix" and os.geteuid() == 0, "Linux root required for isolated temporary /opt directories")
class NativeConfigurationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="yaya-native-config-test-", dir="/opt")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        (self.root / "shared").mkdir()
        self.output = self.root / "rendered"
        self.args = argparse.Namespace(
            root=str(self.root), output=str(self.output), web_port=8878, api_port=8879,
            uid=10001, web_origin="", license_api="", default_origin="http://203.0.113.10:8878",
            default_license_api="http://203.0.113.10:8879", node=shutil.which("node") or "/usr/bin/true", docker="/usr/bin/docker",
            docker_group="docker", helper_image="sha256:example", legacy_container="",
        )

    def test_initialization_connects_host_monitor_and_configures_selected_ports(self):
        configure.render(self.args)
        values = configure.read_env(self.output / "operation-center.env")
        self.assertEqual(values["YAYA_OPS_ENABLED"], "true")
        self.assertEqual(values["YAYA_HOST_MONITOR_ALLOWED_UID"], "10001")
        self.assertEqual(values["YAYA_OPERATION_CENTER_PORT"], "8879")
        self.assertEqual(values["PORT"], "8878")
        self.assertEqual(values["YAYA_PLATFORM_LICENSE_CENTER_URL"], "http://203.0.113.10:8879")
        self.assertEqual(values["YAYA_FILE_AGENT_ALLOWED_UID"], "10001")
        self.assertEqual(values["YAYA_FILE_AGENT_SOCKET"], "/run/yaya-file-agent/files.sock")
        file_values = configure.read_env(self.output / "file-agent.env")
        self.assertEqual(file_values["YAYA_FILES_ROOT"], "/")
        self.assertNotIn("ALIYUN_ACCESS_KEY_SECRET", file_values)
        self.assertNotIn("YAYA_OPERATION_CENTER_DB_PATH", file_values)
        self.assertEqual((self.output / "file-agent.env").stat().st_mode & 0o777, 0o600)
        for name in configure.UNITS:
            unit = (self.output / name).read_text()
            self.assertNotIn("@ROOT@", unit)
            self.assertIn(f"# YAYA_INSTALL_ROOT={self.root}", unit)
        self.assertEqual((self.output / "operation-center.env").stat().st_mode & 0o777, 0o600)

    def test_upgrade_preserves_secrets_public_addresses_and_custom_settings(self):
        token = 'literal $value `text` & quote" and slash\\'
        escaped = token.replace("\\", "\\\\").replace('"', '\\"')
        (self.root / "shared/operation-center.env").write_text(
            f'ALIYUN_ACCESS_KEY_SECRET="{escaped}"\n'
            'YAYA_OPERATION_CENTER_WEB_ORIGIN="https://ops.example.com"\n'
            'YAYA_PLATFORM_LICENSE_CENTER_URL="https://api.example.com"\n'
            'CUSTOM_SETTING="keep-me"\nYAYA_OPS_AGENT_SOCKET="/old/agent.sock"\nYAYA_FILES_ROOT="/srv/customer-files"\n', encoding="utf-8")
        configure.render(self.args)
        values = configure.read_env(self.output / "operation-center.env")
        self.assertEqual(values["ALIYUN_ACCESS_KEY_SECRET"], token)
        self.assertEqual(values["CUSTOM_SETTING"], "keep-me")
        self.assertEqual(configure.read_env(self.output / "file-agent.env")["YAYA_FILES_ROOT"], "/srv/customer-files")
        self.assertEqual(values["YAYA_OPERATION_CENTER_WEB_ORIGIN"], "https://ops.example.com")
        self.assertEqual(values["YAYA_PLATFORM_LICENSE_CENTER_URL"], "https://api.example.com")
        self.assertNotIn("YAYA_OPS_AGENT_SOCKET", values)
        self.assertEqual(values["YAYA_OPERATION_CENTER_DB_PATH"], str(self.root / "shared/data/operation-center.sqlite3"))

    def test_explicit_public_address_overrides_old_configuration(self):
        (self.root / "shared/operation-center.env").write_text('YAYA_OPERATION_CENTER_WEB_ORIGIN="https://old.example.com"\n')
        self.args.web_origin = "https://new.example.com"
        configure.render(self.args)
        self.assertEqual(configure.read_env(self.output / "operation-center.env")["YAYA_OPERATION_CENTER_WEB_ORIGIN"], "https://new.example.com")

    def test_symlinked_data_directory_and_path_traversal_are_rejected(self):
        with tempfile.TemporaryDirectory() as outside:
            (self.root / "shared/data").symlink_to(outside, target_is_directory=True)
            with self.assertRaises(ValueError):
                configure.installation_root(str(self.root))
        for value in ("/", "/opt", "/opt/../etc", "/opt/application/../other"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                configure.installation_root(value)

    def test_invalid_systemd_substitutions_cannot_inject_unit_directives(self):
        self.args.docker_group = "docker\nUser=root"
        with self.assertRaises(ValueError):
            configure.render(self.args)

    @unittest.skipUnless(shutil.which("systemd-analyze"), "systemd analyzer unavailable")
    def test_systemd_service_definitions_and_dependency_graph_validate(self):
        # Isolated executable fixtures only; no units are installed or started.
        release = self.root / "releases/test"
        (release / "bin").mkdir(parents=True)
        (release / "web/web").mkdir(parents=True)
        (self.root / "shared/data").mkdir()
        (self.root / "shared/monitor").mkdir()
        (self.root / "current").symlink_to(release, target_is_directory=True)
        for name in ("run-api.sh", "yaya-host-monitor", "yaya-operation-center-api"):
            binary = release / "bin" / name
            shutil.copyfile("/usr/bin/true", binary)
            binary.chmod(0o755)
        configure.render(self.args)
        result = subprocess.run(
            ["systemd-analyze", "verify", *(str(self.output / name) for name in configure.UNITS)],
            capture_output=True, text=True, timeout=30,
        )
        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
