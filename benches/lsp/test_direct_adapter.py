#!/usr/bin/env python3

from __future__ import annotations

import hashlib
import json
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
LSP_DIR = ROOT / "benches" / "lsp"
UPSTREAM_PATH = LSP_DIR / "upstream.json"


class DirectAdapterTests(unittest.TestCase):
    def setUp(self) -> None:
        self.metadata = json.loads(UPSTREAM_PATH.read_text())
        self.patch_path = ROOT / self.metadata["adapter"]["path"]
        self.patch = self.patch_path.read_text()

    def test_pins_official_source_and_adapter_bytes(self) -> None:
        commit = self.metadata["commit"]

        self.assertEqual(
            set(self.metadata),
            {
                "schema_version",
                "repository",
                "version",
                "tag",
                "commit",
                "source",
                "adapter",
            },
        )
        self.assertEqual(set(self.metadata["source"]), {"name", "url", "sha256"})
        self.assertEqual(set(self.metadata["adapter"]), {"path", "sha256"})
        self.assertEqual(self.metadata["schema_version"], 2)
        self.assertEqual(self.metadata["repository"], "asyncswap/lsp-bench")
        self.assertEqual(self.metadata["version"], "0.3.3")
        self.assertEqual(self.metadata["tag"], "v0.3.3")
        self.assertRegex(commit, r"^[0-9a-f]{40}$")
        self.assertEqual(
            self.metadata["source"]["url"],
            f"https://codeload.github.com/asyncswap/lsp-bench/tar.gz/{commit}",
        )
        self.assertEqual(self.metadata["source"]["name"], f"lsp-bench-{commit}.tar.gz")
        self.assertEqual(
            self.metadata["source"]["sha256"],
            "145dc03c5606d6b5ec66647d233486bab9f4e65022275763bf445bc26414470e",
        )
        self.assertEqual(
            hashlib.sha256(self.patch_path.read_bytes()).hexdigest(),
            self.metadata["adapter"]["sha256"],
        )
        self.assertEqual(
            self.metadata["adapter"]["path"], "benches/lsp/lsp-bench-direct.patch"
        )
        self.assertNotIn("asset", self.metadata)

    def test_patch_is_narrow_and_enforces_the_direct_contract(self) -> None:
        short_commit = self.metadata["commit"][:7]
        old_paths = re.findall(r"^--- (\S+)$", self.patch, re.MULTILINE)
        new_paths = re.findall(r"^\+\+\+ (\S+)$", self.patch, re.MULTILINE)

        self.assertEqual(old_paths, ["a/build.rs", "a/src/main.rs"])
        self.assertEqual(new_paths, ["b/build.rs", "b/src/main.rs"])
        self.assertIn(f'.filter(|commit| commit == "{short_commit}")', self.patch)
        self.assertIn(f'.unwrap_or_else(|| "{short_commit}".to_string())', self.patch)
        self.assertIn("is_valid_initialize_response", self.patch)
        self.assertIn("is_fixture_ready_diagnostics", self.patch)
        self.assertIn('Some("textDocument/publishDiagnostics")', self.patch)
        self.assertIn('code.as_str() == Some("2018")', self.patch)
        self.assertIn("self.diagnostics_ready = true", self.patch)
        self.assertIn('              "ms": ms,', self.patch)

    def test_server_side_python_proxy_is_removed(self) -> None:
        self.assertFalse((LSP_DIR / "lsp_filter.py").exists())


if __name__ == "__main__":
    unittest.main()
