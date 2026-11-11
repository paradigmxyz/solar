"""Live pass validation using the compiler and checker binaries."""

import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOLAR = os.environ.get("SOLAR", str(ROOT.parent / "target/debug/solar"))
CHECKER = str(ROOT / "solar-tv")


class PassManagerTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.out = Path(self.tmp.name) / "pairs"

    def compile(self, source, pipeline, *, enabled=True):
        ir = "mir" if source.suffix == ".mir" else "evm-ir"
        args = [SOLAR, source, "-Ogas", f"-Z{ir}-pipeline={pipeline}"]
        if enabled:
            args.append(f"-Zsolar-tv={self.out}")
        return subprocess.run(
            args,
            env={**os.environ, "SOLAR_TV": CHECKER, "SOLAR": SOLAR},
            text=True,
            capture_output=True,
            timeout=120,
            check=False,
        )

    def test_passes_verify_and_keep_normal_output(self):
        for example, ext, pass_name in [
            ("dce", "mir", "dce"),
            ("switch", "mir", "cfg-simplify"),
            ("phi", "mir", "cfg-simplify"),
            ("casts", "mir", "integer-cleanup"),
            ("checked", "mir", "check-elim"),
            ("typed-branch", "mir", "cfg-simplify"),
            ("peephole", "evmir", "peephole"),
            ("symbolic-stack", "evmir", "peephole"),
        ]:
            with self.subTest(example=example):
                source = ROOT / "examples" / ext / example / f"before.{ext}"
                # The second invocation has no additional work on these inputs.
                pipeline = f"{pass_name},{pass_name}"
                plain = self.compile(source, pipeline, enabled=False)
                result = self.compile(source, pipeline)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(plain.returncode, 0, plain.stderr)
                self.assertEqual(result.stdout, plain.stdout)
                self.assertEqual(result.stderr.count(": verified"), 1, result.stderr)
                self.assertEqual(list(self.out.iterdir()), [])

    def test_inconclusive_pair_is_retained(self):
        source = Path(self.tmp.name) / "store.mir"
        text = (ROOT / "examples/mir/dce/before.mir").read_text()
        source.write_text(text.replace("ret v1", "mstore 0, v1\n    ret v1"))
        result = self.compile(source, "dce")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("inconclusive", result.stderr)
        self.assertIn("unmodeled MIR instruction", result.stderr)
        self.assertIn("Replay", result.stderr)
        (pair,) = self.out.iterdir()
        self.assertNotEqual(
            (pair / "before.mir").read_text(), (pair / "after.mir").read_text()
        )
        self.assertIn("unmodeled MIR instruction", (pair / "result.txt").read_text())

    def test_standard_json_cannot_enable_a_checker(self):
        request = {
            "language": "Solidity",
            "sources": {
                "C.sol": {
                    "content": "contract C { function f() external pure returns (uint) { return 42; } }"
                }
            },
            "settings": {
                "optimizer": {"enabled": True},
                "solar": {"unstable": [f"solar-tv={self.out}"]},
                "outputSelection": {"*": {"*": ["evm.bytecode.object"]}},
            },
        }
        result = subprocess.run(
            [SOLAR, "--standard-json"],
            env={**os.environ, "SOLAR_TV": str(self.out / "absent")},
            input=json.dumps(request),
            capture_output=True,
            text=True,
            check=False,
            timeout=30,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        output = json.loads(result.stdout)
        self.assertFalse(
            [e for e in output.get("errors", []) if e["severity"] == "error"], output
        )
        self.assertTrue(output["contracts"]["C.sol"]["C"]["evm"]["bytecode"]["object"])
        self.assertFalse(self.out.exists())
