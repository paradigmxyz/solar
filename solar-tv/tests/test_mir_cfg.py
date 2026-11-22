"""Check that MIR jump threading preserves storage and simultaneous phi inputs."""

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import native, world
from validator.common import Program


class MirCfgTests(unittest.TestCase):
    def test_real_coalescing_with_phi_and_storage(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            before, after = (root / f"{name}.mir" for name in ("before", "after"))
            before.write_text("""@module Coalesce
fn @count(arg0: i256) -> i256 {
  bb0:
    v0 = sload 1
    jump bb1
  bb1:
    jump bb2
  bb2:
    v1 = phi [bb1: v0], [bb3: v3]
    v2 = lt v1, arg0
    jumpi v2, bb3, bb4
  bb3:
    v3 = add v1, 1
    sstore 1, v3
    jump bb2
  bb4:
    ret v1
}
""")
            after.write_text(
                subprocess.check_output(
                    [
                        SOLAR,
                        before,
                        "-j1",
                        "-Ogas",
                        "-Zmir-pipeline=cfg-simplify",
                        "-Zvalidate-ir=true",
                    ],
                    text=True,
                    timeout=10,
                )
            )
            report = root / "report.json"
            args = [
                CLI,
                "mir",
                before,
                after,
                "--function=count",
                "--solar",
                SOLAR,
                "--timeout=30",
                "--report",
                report,
            ]
            result = subprocess.run(
                args, text=True, capture_output=True, timeout=90, check=False
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            data = json.loads(report.read_text())
            self.assertEqual(data["region"]["proof"], "kernel-checked-typed-jump-chain")
            self.assertEqual(data["gas_policy"], "unpriced-structural")
            self.assertEqual(data["region"]["functions"], ["count"])

            source = after.read_text()
            self.assertIn("sstore 1,", source)
            after.write_text(source.replace("sstore 1,", "sstore 2,"))
            result = subprocess.run(
                args, text=True, capture_output=True, timeout=90, check=False
            )
            self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
            self.assertEqual(json.loads(report.read_text())["status"], "unsupported")

            fixture_path = root / "world.json"
            fixture_path.write_text(
                '{"version":1,"accounts":[{"address":65536,"storage":{"1":"2"}}]}'
            )
            initial = world.read(fixture_path, SOLAR, None, kind="mir")
            programs = [
                Program(
                    ("world-entry", "mir"),
                    ["arg0"],
                    "",
                    "",
                    replay_ir=text,
                    selector="count",
                    evm_version="cancun",
                    world=initial,
                )
                for text in (before.read_text(), source, after.read_text())
            ]
            for n, compare in enumerate(programs[1:]):
                observed = native.replay(
                    programs[0],
                    compare,
                    [4],
                    executable=native.default(SOLAR),
                    timeout=10,
                )
                a, b = map(json.loads, observed["observations"])
                self.assertEqual(a["values"], ["4"])
                self.assertEqual(b["values"], ["4"])
                self.assertEqual(a == b, n == 0)
