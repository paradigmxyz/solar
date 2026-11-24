"""Check the implicit-to-explicit runtime entry through CLI and pass audits."""

import gzip
import json
import os
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import evaluate, native, world
from validator.common import Program


class LowerDispatchTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        temporary = tempfile.TemporaryDirectory()
        cls.addClassCleanup(temporary.cleanup)
        cls.folder = Path(temporary.name)
        archive = cls.folder / "tiny.json.gz"
        archive.write_bytes(
            gzip.compress(
                evaluate.stable(
                    {"sources": {"C.sol": {"content": "contract C {}"}}, "settings": {}}
                )
            )
        )
        cls.case = {
            "name": "tiny",
            "source": "C.sol",
            "contract": "C",
            "archive": str(archive),
            "sha256": evaluate.digest(archive.read_bytes()),
        }
        cls.before = cls.folder / "before.mir"
        cls.before.write_text(
            "@module C\nfn @keep() -> i256 {\nbb0:\nv0 = icall @leaf\nret v0\n}\n"
            "fn @leaf() -> i256 {\nbb0:\nret 7\n}\n"
        )
        cls.after = cls.folder / "after.mir"
        cls.after.write_text(
            subprocess.check_output(
                [SOLAR, cls.before, "-j1", "-Zmir-pipeline=lower-dispatch"],
                text=True,
                timeout=30,
            )
        )

    def check_pair(self, before, after, status, *flags):
        report = self.folder / "dispatch-report.json"
        run = subprocess.run(
            [
                CLI,
                "mir",
                before,
                after,
                "--prune",
                "--solar",
                SOLAR,
                "--report",
                report,
                *flags,
            ],
            capture_output=True,
            text=True,
            check=False,
            timeout=60,
        )
        self.assertEqual(run.returncode, status, run.stdout + run.stderr)
        return json.loads(report.read_text())

    def test_strict_smoke_checks_every_pass(self):
        manifest = self.folder / "manifest.json"
        manifest.write_text(json.dumps({"version": 1, "cases": [self.case]}))
        output = self.folder / "smoke"
        run = subprocess.run(
            [
                ROOT / "evaluate",
                "--manifest",
                manifest,
                "--solar",
                SOLAR,
                "--all",
                "--require-verified",
                "--timeout",
                "30",
                "--output",
                output,
            ],
            capture_output=True,
            text=True,
            check=False,
            timeout=120,
        )
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        report = json.loads((output / "results.json").read_text())
        (case,) = report["cases"]
        self.assertTrue(case["output_identical"])
        self.assertEqual(case["captured"], 3)
        self.assertEqual(
            {row["pass"]: row["status"] for row in case["checks"]},
            {
                "cfg-simplify": "verified",
                "lower-dispatch": "verified",
                "lower-evm-shaped": "verified",
            },
        )
        (dispatch,) = (row for row in case["checks"] if row["pass"] == "lower-dispatch")
        self.assertEqual(dispatch["proof_mode"], "empty-runtime-dispatch")
        proof = json.loads((output / dispatch["report"]).read_text())
        self.assertEqual(proof["evidence"], "kernel-proof")
        self.assertFalse(proof["whole_pass"])

    def test_live_dispatch_audit_keeps_bytecode(self):
        source = evaluate.inputs(self.case)
        baseline, _ = evaluate.compile_input(SOLAR, source, self.folder)
        self.check_audit(
            source, baseline, self.folder / "audit", "empty-runtime-dispatch"
        )

    def check_audit(self, source, baseline, audit, proof):
        run = subprocess.run(
            [SOLAR, "--standard-json", "-j1", f"-Zsolar-tv={audit}"],
            input=json.dumps(source),
            env={**os.environ, "SOLAR_TV": CLI, "SOLAR": SOLAR},
            capture_output=True,
            text=True,
            check=False,
            timeout=90,
        )
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        contracts = json.loads(run.stdout)["contracts"]
        self.assertEqual(evaluate.digest(evaluate.stable(contracts)), baseline)
        records = [
            json.loads(path.read_text()) for path in (audit / "audits").glob("*.json")
        ]
        (dispatch,) = (
            row
            for row in records
            if row["context"].startswith("solar-tv: mir pass `lower-dispatch`")
        )
        self.assertEqual(dispatch["status"], "verified")
        self.assertEqual(dispatch["region"]["proof"], proof)

    def test_retained_function_and_bad_dispatchers(self):
        record = self.check_pair(self.before, self.after, 0)
        self.assertEqual(record["region"]["retained"], ["keep", "leaf"])
        self.assertEqual(record["region"]["work_steps"], 6)
        text = self.after.read_text()
        broken = self.folder / "broken.mir"
        for old, new, diagnostic in (
            ("revert 0, 0", "stop", "must reach an empty revert"),
            ("revert 0, 0", "revert 0, 32", "must reach an empty revert"),
            ("revert 0, 0", "jump bb1", "must reach an empty revert"),
            ("ret 7", "ret 8", "changed retained functions"),
        ):
            with self.subTest(edit=new):
                self.assertIn(old, text)
                broken.write_text(text.replace(old, new))
                report = self.check_pair(self.before, broken, 2)
                self.assertEqual(report["status"], "unsupported")
                self.assertIn(diagnostic, report["diagnostic"])

    def test_absent_routes_are_checked_in_the_source(self):
        routed = self.folder / "routed.mir"
        routed.write_text(
            self.before.read_text().replace("-> i256", "-> i256 [selector=0x01020304]")
        )
        report = self.check_pair(routed, self.after, 2)
        self.assertEqual(report["status"], "unsupported")
        self.assertIn("explicit ABI wrappers", report["diagnostic"])

    def test_selector_receive_and_nonpayable_fallback(self):
        folder = self.folder / "routed"
        folder.mkdir()
        source = evaluate.inputs(self.case)
        source["sources"]["C.sol"]["content"] = """
contract C {
    function nonces(address) external pure returns (uint256) { return 7; }
    receive() external payable {}
    fallback() external {}
}
"""
        baseline, _ = evaluate.compile_input(SOLAR, source, folder)
        captured, _ = evaluate.compile_input(SOLAR, source, folder, capture=True)
        self.assertEqual(baseline, captured)
        row = next(
            r for r in evaluate.captures(folder) if r["pass"] == "lower-dispatch"
        )
        before, after = row["paths"]
        proof = self.check_pair(before, after, 0)
        self.assertEqual(proof["region"]["proof"], "routed-runtime-dispatch")
        self.assertEqual(proof["region"]["selectors"], 1)
        self.assertTrue(proof["region"]["receive"])
        self.assertTrue(proof["region"]["fallback"])
        self.check_audit(source, baseline, folder / "audit", "routed-runtime-dispatch")
        # A constructor keeps the printed module prefix unchanged as dispatch is
        # materialized, so capture records several changed function regions.
        unit = source["sources"]["C.sol"]
        unit["content"] = unit["content"].replace(
            "contract C {",
            "contract C { constructor() { require(msg.sender != address(0)); }",
        )
        with patch.object(evaluate, "inputs", return_value=source):
            evaluated = evaluate.evaluate_case(
                self.case,
                SOLAR,
                folder / "evaluation",
                None,
                "0",
                30,
                passes=("lower-dispatch",),
            )
        checks = evaluated["checks"]
        self.assertGreater(len(checks), 1)
        self.assertEqual(evaluated["eligible"], len(checks))
        self.assertGreater(evaluated["captured"], evaluated["eligible"])
        self.assertEqual({check["status"] for check in checks}, {"verified"})
        self.assertEqual(
            {check["proof_mode"] for check in checks}, {"routed-runtime-dispatch"}
        )
        self.assertEqual(len({check["report"] for check in checks}), 1)
        text = after.read_text()
        self.assertIn("0x7ecebe00 =>", text)
        broken = folder / "wrong-selector.mir"
        broken.write_text(text.replace("0x7ecebe00 =>", "0x7ecebe01 =>"))
        report = self.check_pair(before, broken, 2)
        self.assertEqual(report["status"], "unsupported")
        self.assertIn("dispatch proof did not close", report["diagnostic"])
        without_short_guard, count = re.subn(
            r"((v\d+) = lt v\d+, 4\n)    jumpi \2, bb\d+, (bb\d+)",
            r"\1    jump \3",
            text,
        )
        self.assertEqual(count, 1)
        broken.write_text(without_short_guard)
        report = self.check_pair(before, broken, 2)
        self.assertEqual(report["status"], "unsupported")
        self.assertIn("dispatch proof did not close", report["diagnostic"])

    def test_hoisted_calldata_guard_and_independent_boundary_replay(self):
        folder = self.folder / "hoisted"
        folder.mkdir()
        source = evaluate.inputs(self.case)
        source["sources"]["C.sol"]["content"] = (
            (ROOT / "examples/solidity/functions.sol")
            .read_text()
            .replace("contract FunctionRegions", "contract C")
        )
        baseline, _ = evaluate.compile_input(SOLAR, source, folder)
        captured, _ = evaluate.compile_input(SOLAR, source, folder, capture=True)
        self.assertEqual(baseline, captured)
        row = next(
            r for r in evaluate.captures(folder) if r["pass"] == "lower-dispatch"
        )
        before, after = row["paths"]
        proof = self.check_pair(before, after, 0)
        self.assertEqual(proof["evidence"], "kernel-proof")
        self.assertFalse(proof["whole_pass"])
        self.assertEqual(proof["region"]["proof"], "calldata-guard-hoist")
        self.assertEqual(proof["region"]["minimum_calldata"], 36)
        self.assertEqual(proof["region"]["selectors"], 2)
        self.check_audit(source, baseline, folder / "audit", "calldata-guard-hoist")

        entry, wrappers = after.read_text().split("fn @first(", 1)
        self.assertEqual(entry.count(", 36\n"), 1)
        broken = folder / "broken-minimum.mir"
        broken.write_text(entry.replace(", 36\n", ", 35\n") + "fn @first(" + wrappers)
        report = self.check_pair(before, broken, 2)
        self.assertIn("dispatch proof did not close", report["diagnostic"])

        def program(path, selector, initial):
            return Program(
                ("world-entry", "mir"),
                [],
                "",
                "",
                replay_ir=path.read_text(),
                selector=selector,
                evm_version="cancun",
                world=initial,
            )

        fixture = folder / "world.json"
        request = {"executable": native.default(SOLAR), "timeout": 30}
        for size in (35, 36):
            with self.subTest(size=size):
                fixture.write_text(
                    json.dumps(
                        {"version": 1, "calldata": "0x0b3f62f5" + "00" * (size - 4)}
                    )
                )
                initial = world.read(fixture, SOLAR, None, kind="mir")
                original = program(before, "first", initial)
                source_results = native.replay(original, original, [], **request)
                target_results = native.replay(
                    program(after, "entry", initial),
                    program(broken, "entry", initial),
                    [],
                    **request,
                )
                old = json.loads(source_results["observations"][0])
                good, bad = map(json.loads, target_results["observations"])
                self.assertEqual(old, good)
                if size == 35:
                    self.assertEqual((old["outcome"], old["data"]), ("revert", []))
                    self.assertEqual(
                        (bad["outcome"], bad["data"]), ("success", [0] * 31 + [1])
                    )
                else:
                    self.assertEqual(old, bad)
                    self.assertEqual(
                        (old["outcome"], old["data"]), ("success", [0] * 31 + [1])
                    )

    def test_library_delegatecall_guard_with_hoisted_calldata(self):
        folder = self.folder / "library"
        folder.mkdir()
        source = evaluate.inputs(self.case)
        source["sources"]["C.sol"]["content"] = """
library C {
    function touch(uint256 value) external returns (uint256) { return value + 1; }
    function probe(uint256 value) external pure returns (uint256) { return value; }
}
"""
        baseline, _ = evaluate.compile_input(SOLAR, source, folder)
        captured, _ = evaluate.compile_input(SOLAR, source, folder, capture=True)
        self.assertEqual(baseline, captured)
        before, after = next(
            row["paths"]
            for row in evaluate.captures(folder)
            if row["pass"] == "lower-dispatch"
        )
        proof = self.check_pair(before, after, 0)
        self.assertEqual(proof["region"]["library_guarded_routes"], 1)
        self.assertEqual(proof["region"]["minimum_calldata"], 36)

        text = after.read_text()
        broken_text, changes = re.subn(
            r"(    v\d+ = )eq (v\d+, v\d+\n    jumpi)",
            r"\1ne \2",
            text,
            count=1,
        )
        self.assertEqual(changes, 1)
        broken = folder / "inverted-library-guard.mir"
        broken.write_text(broken_text)
        report = self.check_pair(before, broken, 2)
        self.assertIn("dispatch proof did not close", report["diagnostic"])

        fixture = folder / "world.json"
        for selector, address, outcome, inverted in (
            ("edaa0c1d", 65536, "revert", "success"),
            ("edaa0c1d", 65537, "success", "revert"),
            ("db082440", 65536, "success", "success"),
        ):
            with self.subTest(selector=selector, address=address):
                fixture.write_text(
                    json.dumps(
                        {
                            "version": 1,
                            "address": address,
                            "calldata": "0x" + selector + "00" * 32,
                            "mir_layout": {"immutable_values": {"0": 65536}},
                        }
                    )
                )
                initial = world.read(fixture, SOLAR, None, kind="mir")
                original, candidate = [
                    Program(
                        ("world-entry", "mir"),
                        [],
                        "",
                        "",
                        replay_ir=body,
                        selector="entry",
                        evm_version="cancun",
                        world=initial,
                    )
                    for body in (text, broken_text)
                ]
                result = native.replay(
                    original,
                    candidate,
                    [],
                    executable=native.default(SOLAR),
                    timeout=30,
                )
                good, bad = map(json.loads, result["observations"])
                self.assertEqual(good["outcome"], outcome)
                self.assertEqual(bad["outcome"], inverted)

    def test_debug_dispatch_reverts_match_configured_payload(self):
        variants = {
            "empty": "contract C {}",
            "value": "contract C { function f(uint256 x) external pure returns (uint256) { return x; } }",
            "receive": "contract C { receive() external payable {} }",
            "fallback": "contract C { function f() external payable {} receive() external payable {} fallback() external {} }",
            "library": """
library C {
    function touch(uint256 value) external returns (uint256) { return value + 1; }
    function probe(uint256 value) external pure returns (uint256) { return value; }
}
""",
        }
        for name, content in variants.items():
            with self.subTest(name=name):
                folder = self.folder / f"debug-{name}"
                folder.mkdir()
                source = evaluate.inputs(self.case)
                source["sources"]["C.sol"]["content"] = content
                source["settings"]["debug"] = {"revertStrings": "debug"}
                baseline, _ = evaluate.compile_input(SOLAR, source, folder)
                captured, _ = evaluate.compile_input(
                    SOLAR, source, folder, capture=True
                )
                self.assertEqual(baseline, captured)
                dispatch = [
                    row
                    for row in evaluate.captures(folder)
                    if row["pass"] == "lower-dispatch"
                ]
                self.assertTrue(dispatch)
                self.assertTrue(all(row["debug_reverts"] for row in dispatch))
                before, after = dispatch[0]["paths"]
                proof = self.check_pair(before, after, 0, "--debug-reverts")
                self.assertEqual(proof["region"]["proof"], "debug-runtime-dispatch")
                self.assertTrue(proof["region"]["debug_reverts"])
                if name == "library":
                    self.check_debug_audit(before, folder / "audit")
                    self.check_pair(before, after, 2)
                    self.check_broken_debug_library(before, after, folder)
                if name == "empty":
                    with (
                        patch.object(evaluate, "inputs", return_value=source),
                        patch.object(
                            evaluate,
                            "select",
                            side_effect=lambda rows, *_: [
                                r for r in rows if r["pass"] == "lower-dispatch"
                            ],
                        ),
                    ):
                        report = evaluate.evaluate_case(
                            self.case, SOLAR, folder / "evaluation", None, "0", 30
                        )
                    (check,) = report["checks"]
                    self.assertEqual(check["status"], "verified")
                    self.assertEqual(check["proof_mode"], "debug-runtime-dispatch")

    def check_debug_audit(self, before, audit):
        command = [
            SOLAR,
            before,
            "-j1",
            "--evm-version=cancun",
            "--revert-strings",
            "debug",
            "-Zmir-pipeline=lower-dispatch",
        ]
        baseline = subprocess.check_output(command, text=True, timeout=30)
        checked = subprocess.run(
            [*command, f"-Zsolar-tv={audit}"],
            env={**os.environ, "SOLAR_TV": CLI, "SOLAR": SOLAR},
            text=True,
            capture_output=True,
            check=False,
            timeout=30,
        )
        self.assertEqual(checked.returncode, 0, checked.stderr)
        self.assertEqual(baseline, checked.stdout)
        (record,) = [
            json.loads(path.read_text()) for path in (audit / "audits").glob("*.json")
        ]
        self.assertEqual(record["status"], "verified")
        self.assertEqual(record["region"]["proof"], "debug-runtime-dispatch")

    def check_broken_debug_library(self, before, after, folder):
        text = after.read_text()
        self.assertEqual(text.count("0x4e6f6e2d"), 1)
        altered = text.replace("0x4e6f6e2d", "0x4d6f6e2d")
        broken = folder / "wrong-library-error.mir"
        broken.write_text(altered)
        proof = self.check_pair(before, broken, 2, "--debug-reverts")
        self.assertIn("dispatch proof did not close", proof["diagnostic"])

        fixture = folder / "world.json"
        fixture.write_text(
            json.dumps(
                {
                    "version": 1,
                    "address": 65536,
                    "calldata": "0xedaa0c1d" + "00" * 32,
                    "mir_layout": {"immutable_values": {"0": 65536}},
                    "memory": {"data": "0x" + "ef" * 192},
                }
            )
        )
        initial = world.read(fixture, SOLAR, None, kind="mir")
        original, candidate = [
            Program(
                ("world-entry", "mir"),
                [],
                "",
                "",
                replay_ir=body,
                selector="entry",
                evm_version="cancun",
                world=initial,
            )
            for body in (text, altered)
        ]
        result = native.replay(
            original, candidate, [], executable=native.default(SOLAR), timeout=30
        )
        original, candidate = map(json.loads, result["observations"])
        reason = b"Non-view function of library called without DELEGATECALL"
        expected = (
            bytes.fromhex("08c379a0")
            + (32).to_bytes(32)
            + len(reason).to_bytes(32)
            + reason.ljust(64, b"\0")
        )
        self.assertEqual(original["outcome"], "revert")
        self.assertEqual(candidate["outcome"], "revert")
        self.assertEqual(bytes(original["data"]), expected)
        self.assertEqual(bytes(candidate["data"]), expected[:68] + b"M" + expected[69:])

    def test_legacy_selector_shift_is_proved_before_arithmetic_lowering(self):
        folder = self.folder / "legacy-selector"
        folder.mkdir()
        source = evaluate.inputs(self.case)
        source["sources"]["C.sol"]["content"] = (
            "contract C { function f(uint256 x) external pure returns (uint256) { return x; } }"
        )
        source["settings"]["evmVersion"] = "byzantium"
        baseline, _ = evaluate.compile_input(SOLAR, source, folder)
        captured, _ = evaluate.compile_input(SOLAR, source, folder, capture=True)
        self.assertEqual(baseline, captured)
        captures = evaluate.captures(folder)
        before, after = next(
            row["paths"] for row in captures if row["pass"] == "lower-dispatch"
        )
        target = ("--evm-version", "byzantium")
        proof = self.check_pair(before, after, 0, *target)
        self.assertEqual(proof["evidence"], "kernel-proof")
        self.assertTrue(proof["region"]["legacy_selector_shift"])

        text = after.read_text()
        mutated, count = re.subn(r"( = shr v\d+, )224\b", r"\g<1>225", text)
        self.assertEqual(count, 1)
        broken = folder / "wrong-selector-shift.mir"
        broken.write_text(mutated)
        report = self.check_pair(before, broken, 2, *target)
        self.assertIn("unavailable opcode", report["diagnostic"])

        _, lowered = next(
            row["paths"] for row in captures if row["pass"] == "lower-shifts"
        )
        self.assertIn(f", {2**224:#x}", lowered.read_text())
        self.assertRegex(lowered.read_text(), r"= div v\d+, 0x1[0-9a-f]+")
