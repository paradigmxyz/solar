"""Run the conditional-check module certificate through the compiler hook."""

import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

import test_mir_state as state
from test_cli import CLI, SOLAR


class LiveLowerChecksTests(unittest.TestCase):
    def test_live_helper_audit_preserves_mir(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "before.mir"
            audit = root / "live"
            string = int.from_bytes(b"bad".ljust(32, b"\0"), "big")
            source.write_text(
                state.module(
                    f"""v0 = calldataload 0
v1 = ne v0, 0
icall require<short_string>, v1, 3, {string}
ret 7"""
                )
            )
            command = [SOLAR, source, "-j1", "-Ogas", "-Zmir-pipeline=lower-checks"]
            outputs = []
            for flags in ([], [f"-Zsolar-tv={audit}"]):
                result = subprocess.run(
                    [*command, *flags],
                    env={**os.environ, "SOLAR_TV": CLI, "SOLAR": SOLAR},
                    capture_output=True,
                    text=True,
                    check=False,
                    timeout=120,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                outputs.append(result.stdout)
            self.assertEqual(*outputs)
            records = [
                json.loads(file.read_text())
                for file in (audit / "audits").glob("*.json")
            ]
            self.assertCountEqual(
                [record["region"]["kind"] for record in records], ["function", "module"]
            )
            for record in records:
                self.assertEqual(record["status"], "verified")
                self.assertEqual(record["evidence"], "kernel-proof")
                self.assertEqual(
                    record["region"]["proof"], "kernel-checked-conditional-checks"
                )
