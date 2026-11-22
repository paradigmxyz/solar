"""Check real lower-evm-shaped captures and independently replay broken edits."""

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import evaluate, native, world
from validator.common import Program


class PhaseAdvanceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        temporary = tempfile.TemporaryDirectory()
        cls.addClassCleanup(temporary.cleanup)
        cls.directory = Path(temporary.name)
        cls.source = {
            "language": "Solidity",
            "sources": {"C.sol": {"content": "contract C {}"}},
            "settings": {
                "optimizer": {"enabled": True},
                "outputSelection": {
                    "*": {"*": ["abi", "evm.bytecode", "evm.deployedBytecode"]}
                },
            },
        }
        cls.baseline, _ = evaluate.compile_input(SOLAR, cls.source, cls.directory)
        cls.captured, _ = evaluate.compile_input(
            SOLAR, cls.source, cls.directory, capture=True
        )
        (cls.phase_change,) = (
            row
            for row in evaluate.captures(cls.directory)
            if row["pass"] == "lower-evm-shaped"
        )

    def check_pair(self, paths, status):
        report = self.directory / "phase-report.json"
        result = subprocess.run(
            [
                CLI,
                "mir",
                *paths,
                "--prune",
                "--solar",
                SOLAR,
                "--report",
                report,
                "--timeout=30",
                "--verbose",
            ],
            capture_output=True,
            text=True,
            check=False,
            timeout=60,
        )
        self.assertEqual(result.returncode, status, result.stdout + result.stderr)
        return json.loads(report.read_text())

    def test_real_capture_proves_all_resolved_bodies(self):
        self.assertEqual(self.baseline, self.captured)
        self.assertIsNone(self.phase_change["region"])
        record = self.check_pair(self.phase_change["paths"], 0)
        self.assertEqual(record["evidence"], "kernel-proof")
        self.assertEqual(record["gas_policy"], "unpriced-structural")
        self.assertFalse(record["whole_pass"])
        self.assertEqual(
            record["region"],
            {
                "kind": "module",
                "proof": "resolved-phase-advance",
                "phases": ["semantic", "lowered"],
                "functions": ["entry"],
            },
        )
        before, after = record["import_correspondence"]
        self.assertNotEqual(before["source_sha256"], after["source_sha256"])
        self.assertEqual(before["resolved_sha256"], after["resolved_sha256"])

    def test_live_pass_runs_certificate_without_changing_output(self):
        audit = self.directory / "audit"
        result = subprocess.run(
            [SOLAR, "--standard-json", "-j1", f"-Zsolar-tv={audit}"],
            input=json.dumps(self.source),
            env={**os.environ, "SOLAR_TV": CLI, "SOLAR": SOLAR},
            text=True,
            capture_output=True,
            check=False,
            timeout=90,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        contracts = json.loads(result.stdout)["contracts"]
        self.assertEqual(evaluate.digest(evaluate.stable(contracts)), self.baseline)
        records = [
            json.loads(path.read_text()) for path in (audit / "audits").glob("*.json")
        ]
        (record,) = (
            row
            for row in records
            if row["context"].startswith("solar-tv: mir pass `lower-evm-shaped`")
        )
        self.assertEqual(record["status"], "verified")
        self.assertEqual(record["region"]["proof"], "resolved-phase-advance")

    def test_phase_label_does_not_allow_other_edits(self):
        before, after = self.phase_change["paths"]
        text = after.read_text()
        broken = self.directory / "changed.mir"
        for old, new in (("revert 0, 0", "stop"), ("@module C", "@module Different")):
            with self.subTest(edit=new):
                self.assertIn(old, text)
                broken.write_text(text.replace(old, new))
                record = self.check_pair([before, broken], 2)
                self.assertEqual(record["status"], "unsupported")
                self.assertIn("phase advance changed executable", record["diagnostic"])
        self.assertEqual(self.check_pair([after, before], 2)["status"], "unsupported")

    def test_lowered_phase_contract_is_validated(self):
        before, after = (
            self.directory / name for name in ("narrow.mir", "bad-phase.mir")
        )
        text = """@module P
fn @f(arg0: i8) -> i8 {
  bb0:
    ret arg0
}
"""
        before.write_text(text)
        after.write_text(text.replace("@module P\n", "@module P\n@phase lowered\n"))
        record = self.check_pair([before, after], 2)
        self.assertEqual(record["status"], "unsupported")
        self.assertIn("non-word type", record["diagnostic"])

    def test_solidity_nonreturning_call_and_bytecode_neutrality(self):
        folder = self.directory / "solidity-shape"
        folder.mkdir()
        source = {
            **self.source,
            "sources": {
                "C.sol": {
                    "content": """contract C {
    function fail(uint256 x) internal pure {
        assembly {
            mstore(0, x)
            revert(0, 32)
        }
    }

    function go(uint256 x) external pure {
        fail(x);
    }
}"""
                }
            },
            "settings": {
                **self.source["settings"],
                "optimizer": {"enabled": False},
                "evmVersion": "cancun",
            },
        }
        baseline, _ = evaluate.compile_input(SOLAR, source, folder)
        captured, _ = evaluate.compile_input(SOLAR, source, folder, capture=True)
        self.assertEqual(baseline, captured)
        (phase,) = (
            row
            for row in evaluate.captures(folder)
            if row["pass"] == "lower-evm-shaped"
        )
        before, after = phase["paths"]
        self.assertIn("icall @fail", before.read_text())
        self.assertIn("tail_call @fail", after.read_text())
        report = self.check_pair([before, after], 0)
        self.assertEqual(report["region"]["proof"], "resolved-phase-rewrites")
        self.assertEqual(report["region"]["certificates"]["go"]["abort_tails"], 1)

    def test_real_pointer_call_and_phi_rewrites(self):
        folder = self.directory / "shape-audit"
        example = ROOT / "examples/mir/phase-shaping/input.mir"
        args = [
            SOLAR,
            example,
            "-j1",
            "--evm-version=cancun",
            "-Zmir-pipeline=lower-evm-shaped",
            "-Zvalidate-ir=true",
        ]
        baseline = subprocess.check_output(args, text=True, timeout=10)
        captured = subprocess.check_output(
            [*args, f"-Zsolar-tv={folder}", "-Zsolar-tv-capture"], text=True, timeout=10
        )
        self.assertEqual(baseline, captured)
        (capture,) = evaluate.captures(folder)
        before, after = capture["paths"]
        record = self.check_pair([before, after], 0)
        self.assertEqual(record["evidence"], "kernel-proof")
        self.assertEqual(record["gas_policy"], "unpriced-structural")
        self.assertEqual(record["region"]["proof"], "resolved-phase-rewrites")
        certificates = record["region"]["certificates"]
        self.assertEqual(
            certificates,
            {
                "entry": {"kind": "direct-execution"},
                "caller": {"kind": "direct-execution"},
                "alias": {"kind": "direct-execution"},
                "widen": {"kind": "direct-execution"},
                "count": {"kind": "phi-edges", "edges": [[2, 1]]},
                "choose": {"kind": "phi-edges", "edges": [[2, 1]]},
            },
        )

        fixture = folder / "world.json"
        fixture.write_text('{"version": 1}')
        initial = world.read(fixture, SOLAR, None, kind="mir")
        lowered = after.read_text()

        for function, args, old, new, outcome, field, original, broken in (
            (
                "entry",
                [],
                "tail_call @terminate, 37",
                "tail_call @terminate, 38",
                "revert",
                "data",
                [0] * 31 + [37],
                [0] * 31 + [38],
            ),
            (
                "alias",
                [41],
                "mstore 32, arg0",
                "mstore 64, arg0",
                "return",
                "values",
                ["41"],
                ["0"],
            ),
            ("count", [2], "[bb4: v1]", "[bb4: 9]", "return", "values", ["2"], ["9"]),
        ):
            with self.subTest(rewrite=function):
                self.assertIn(old, lowered)
                good, candidate = self.replay(before, lowered, initial, function, args)
                self.assertEqual(good, candidate)
                self.assertEqual((good["outcome"], good[field]), (outcome, original))
                mutated = lowered.replace(old, new, 1)
                path = folder / f"broken-{function}.mir"
                path.write_text(mutated)
                self.assertEqual(
                    self.check_pair([before, path], 2)["status"], "unsupported"
                )
                good, wrong = self.replay(before, mutated, initial, function, args)
                self.assertEqual((good["outcome"], good[field]), (outcome, original))
                self.assertEqual((wrong["outcome"], wrong[field]), (outcome, broken))

    def test_contextual_graph_preserves_loop_gas_and_abort(self):
        folder = self.directory / "carrier-audit"
        subprocess.run(
            [
                SOLAR,
                ROOT / "examples/mir/phase-carriers/input.mir",
                "-j1",
                "--evm-version=cancun",
                "-Zmir-pipeline=lower-evm-shaped",
                "-Zvalidate-ir=true",
                f"-Zsolar-tv={folder}",
                "-Zsolar-tv-capture",
            ],
            check=True,
            capture_output=True,
            timeout=10,
        )
        (capture,) = evaluate.captures(folder)
        before, after = capture["paths"]
        record = self.check_pair([before, after], 0)
        self.assertEqual(record["evidence"], "kernel-proof")
        self.assertEqual(record["gas_policy"], "unpriced-structural")
        self.assertEqual(
            record["region"]["certificates"],
            {
                "go": {
                    "kind": "carrier-graph",
                    "copies": 0,
                    "blocks": 1,
                    "abort_tails": 1,
                },
                "count": {
                    "kind": "carrier-graph",
                    "copies": 2,
                    "blocks": 4,
                    "abort_tails": 0,
                },
                "observe_gas": {
                    "kind": "carrier-graph",
                    "copies": 2,
                    "blocks": 1,
                    "abort_tails": 0,
                },
            },
        )
        fixture = folder / "world.json"
        fixture.write_text('{"version": 1, "mir_layout": {"gas_before": 100}}')
        initial = world.read(fixture, SOLAR, None, kind="mir")
        lowered = after.read_text()
        for function, args, old, new, field, original, broken in (
            ("count", [3], "add v0, 1", "add v0, 2", "values", ["3"], ["4"]),
            (
                "entry",
                [],
                "tail_call @abort, 37",
                "tail_call @abort, 38",
                "data",
                [0] * 31 + [37],
                [0] * 31 + [38],
            ),
            (
                "entry",
                [],
                "revert 0, 32",
                "returndata 0, 32",
                "outcome",
                "revert",
                "success",
            ),
            (
                "observe_gas",
                [],
                "v0 = gas",
                "v0 = calldatasize",
                "values",
                ["98"],
                ["0"],
            ),
        ):
            with self.subTest(rewrite=function):
                self.assertIn(old, lowered)
                # Only the lowered observer is an isolated GAS window in replay.
                original_ir = after if function == "observe_gas" else before
                if original_ir == before:
                    good, candidate = self.replay(
                        before, lowered, initial, function, args
                    )
                    self.assertEqual(good, candidate)
                mutated = lowered.replace(old, new, 1)
                path = folder / f"broken-{function}.mir"
                path.write_text(mutated)
                self.assertEqual(
                    self.check_pair([before, path], 2)["status"], "unsupported"
                )
                good, wrong = self.replay(original_ir, mutated, initial, function, args)
                self.assertEqual((good[field], wrong[field]), (original, broken))

    def replay(self, before, after, initial, function, values):
        pair = [
            Program(
                ("world-entry", "mir"),
                [f"arg{i}" for i in range(len(values))],
                "",
                "",
                replay_ir=body,
                selector=function,
                evm_version="cancun",
                world=initial,
            )
            for body in (before.read_text(), after)
        ]
        data = native.replay(
            *pair, values, executable=native.default(SOLAR), timeout=10
        )
        return tuple(map(json.loads, data["observations"]))
