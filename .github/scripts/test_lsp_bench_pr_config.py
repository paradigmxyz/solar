#!/usr/bin/env python3

import hashlib
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("lsp-bench-pr-config.py")
REVISION = "1" * 40
SOURCE_URL = "https://example.invalid/source.git"


class LspBenchPrConfigTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.directory = Path(self.temporary.name)
        self.config = self.directory / "benchmark.yaml"
        self.lock = self.directory / "servers.lock.yaml"
        self.binary = self.directory / "solar"
        self.output_config = self.directory / "benchmark.pr.yaml"
        self.output_lock = self.directory / "servers.pr.yaml"
        self.config.write_text(
            "version: 1\nservers_lock: servers.lock.yaml\nfixtures_lock: fixtures.lock.yaml\n"
        )
        self.lock.write_text(
            """version: 1
servers:
  - id: solar
    label: Solar workspace
    command: ../../target/solar
    args: [lsp]
    version_args: [--version]
    locked_version: "0.2.0"
    source:
      url: https://example.invalid/solar.git
      revision: 0000000000000000000000000000000000000000
    artifact:
      path: ../../target/solar
      sha256: null
    required: true

  - id: other
    label: Other server
    command: other-lsp
    version_args: []
    locked_version: "1.0.0"
"""
        )
        self.binary.write_bytes(b"candidate binary")

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def run_script(self, *extra: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                sys.executable,
                str(SCRIPT),
                "--config",
                str(self.config),
                "--servers-lock",
                str(self.lock),
                "--solar-binary",
                str(self.binary),
                "--revision",
                REVISION,
                "--source-url",
                SOURCE_URL,
                "--role",
                "candidate",
                "--output-config",
                str(self.output_config),
                "--output-servers-lock",
                str(self.output_lock),
                *extra,
            ],
            capture_output=True,
            text=True,
        )

    def test_rewrites_only_solar_runtime_identity(self) -> None:
        result = self.run_script()
        self.assertEqual(result.returncode, 0, result.stderr)

        config = self.output_config.read_text()
        lock = self.output_lock.read_text()
        digest = hashlib.sha256(self.binary.read_bytes()).hexdigest()
        self.assertIn('servers_lock: "servers.pr.yaml"', config)
        self.assertIn(f'label: "Solar PR candidate {REVISION[:12]}"', lock)
        self.assertIn(f'command: "{self.binary.resolve()}"', lock)
        self.assertIn("locked_version: null", lock)
        self.assertIn(f"revision: {REVISION}", lock)
        self.assertIn(f'url: "{SOURCE_URL}"', lock)
        self.assertIn(f'path: "{self.binary.resolve()}"', lock)
        self.assertIn(f"sha256: {digest}", lock)
        self.assertIn(
            "  - id: other\n    label: Other server\n    command: other-lsp",
            lock,
        )

    def test_rejects_invalid_revision(self) -> None:
        result = self.run_script("--revision", "short")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("40-character Git commit", result.stderr)

    def test_rejects_multiline_source_url(self) -> None:
        result = self.run_script("--source-url", "https://example.invalid/\nsource")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("single-line", result.stderr)

    def test_rejects_duplicate_solar_entries(self) -> None:
        self.lock.write_text(self.lock.read_text() + "\n  - id: solar\n")
        result = self.run_script()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("expected exactly one Solar server, found 2", result.stderr)

    def test_rejects_duplicate_manifest_fields(self) -> None:
        self.config.write_text(self.config.read_text() + "servers_lock: other.yaml\n")
        result = self.run_script()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("expected exactly one server lock reference, found 2", result.stderr)

        self.config.write_text(
            "version: 1\nservers_lock: servers.lock.yaml\nfixtures_lock: fixtures.lock.yaml\n"
        )
        self.lock.write_text(
            self.lock.read_text().replace(
                "    command: ../../target/solar\n",
                "    command: ../../target/solar\n    command: duplicate\n",
            )
        )
        result = self.run_script()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("expected exactly one Solar command, found 2", result.stderr)

    def test_quotes_non_ascii_and_backslash_paths(self) -> None:
        for name in ("solar\\1", "solar-太阳"):
            with self.subTest(name=name):
                self.binary = self.directory / name
                self.binary.write_bytes(b"candidate binary")
                result = self.run_script()
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn(json.dumps(str(self.binary.resolve())), self.output_lock.read_text())

    def test_quotes_generated_lock_filename(self) -> None:
        self.output_lock = self.directory / "servers.pr.yaml\nprofiles: {}"
        result = self.run_script()
        self.assertEqual(result.returncode, 0, result.stderr)
        config = self.output_config.read_text()
        self.assertIn(f"servers_lock: {json.dumps(self.output_lock.name)}", config)
        self.assertNotIn("\nprofiles: {}\n", config)

    def test_rejects_source_manifest_overwrite(self) -> None:
        self.output_config = self.config
        result = self.run_script()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("must not overwrite source manifests", result.stderr)

    def test_rejects_hard_link_and_binary_aliases(self) -> None:
        os.link(self.config, self.output_config)
        result = self.run_script()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("must not overwrite source manifests", result.stderr)

        self.output_config.unlink()
        self.output_lock = self.binary
        result = self.run_script()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Solar binary", result.stderr)


if __name__ == "__main__":
    unittest.main()
