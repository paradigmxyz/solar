"""Validate a changed pointer recurrence captured from the real MIR pass."""

import json
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import evaluate, mir_state, native, world, world_check

FIXTURE = ROOT.parent / "tests/ui/codegen/mir/indvar-simplify/calldata_pointer_exit.sol"
VERSION = "osaka"


def call_data(elements, needle):
    # Selector, dynamic array offset, needle, length, then array elements.
    return "0x828c8292" + "".join(
        f"{word:064x}" for word in (64, needle, len(elements), *elements)
    )


class CalldataInductionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        work = tempfile.TemporaryDirectory()
        cls.addClassCleanup(work.cleanup)
        cls.directory = Path(work.name)
        compiler = [
            SOLAR,
            str(FIXTURE),
            "-Ogas",
            "-j1",
            f"--evm-version={VERSION}",
            "--emit=bin,bin-runtime",
            "--allow=2264",
        ]
        baseline = subprocess.check_output(compiler, text=True)
        captured = subprocess.check_output(
            [*compiler, f"-Zsolar-tv={cls.directory}", "-Zsolar-tv-capture"], text=True
        )
        if baseline != captured:
            raise AssertionError("capturing the pass changed the emitted bytecode")
        [row] = [
            item
            for item in evaluate.captures(cls.directory)
            if item["pass"] == "indvar-simplify" and item["region"]["name"] == "stopAt"
        ]
        cls.before, cls.after = row["paths"]
        cls.replay = native.default(SOLAR)

    def check_pair(self, after, report):
        result = subprocess.run(
            [
                CLI,
                "mir",
                self.before,
                after,
                "--function",
                "stopAt",
                "--evm-version",
                VERSION,
                "--solar",
                SOLAR,
                "--report",
                report,
                "--verbose",
            ],
            capture_output=True,
            text=True,
            timeout=90,
            check=False,
        )
        return result, json.loads(report.read_text())

    def test_real_induction_rewrite(self):
        result, record = self.check_pair(self.after, self.directory / "valid.json")
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertEqual(record["status"], "verified")
        self.assertEqual(record["gas_policy"], "unpriced-structural")
        self.assertEqual(record["region"]["proof"], "kernel-checked-calldata-induction")
        self.assertFalse(record["whole_pass"])

    def test_wrong_stride_has_independent_witness(self):
        prefix, body = self.after.read_text().rsplit("fn @stopAt() {", 1)
        head, end, suffix = body.partition("\n}")
        mutated, changes = re.subn(
            r"( = add v\d+, )32(?=\n\s+jump bb\d+\n)", r"\g<1>64", head
        )
        self.assertEqual(changes, 1)
        after = self.directory / "wrong-stride.mir"
        after.write_text(prefix + "fn @stopAt() {" + mutated + end + suffix)
        result, record = self.check_pair(after, self.directory / "wrong-stride.json")
        self.assertEqual(result.returncode, 1, result.stderr + result.stdout)
        self.assertEqual(record["status"], "counterexample")
        witness = record["witness"]
        self.assertEqual(witness["observations"], witness["native_observations"])
        self.assertEqual(
            [
                int.from_bytes(bytes(row["data"]), "big")
                for row in witness["observations"]
            ],
            [1, 2],
        )

    def test_changed_index_recovery_cannot_use_certificate(self):
        prefix, body = self.after.read_text().rsplit("fn @stopAt() {", 1)
        mutated, changes = re.subn(
            r"( = shr v\d+, )5(?=\n\s+mstore 0,)", r"\g<1>4", body
        )
        self.assertEqual(changes, 1)
        after = self.directory / "wrong-recovery.mir"
        after.write_text(prefix + "fn @stopAt() {" + mutated)
        result, record = self.check_pair(after, self.directory / "wrong-recovery.json")
        self.assertEqual(result.returncode, 2, result.stderr + result.stdout)
        self.assertEqual(record["status"], "unsupported")

    def test_exits_agree_in_direct_interpreters(self):
        for index, (calldata, outcome, value) in enumerate(
            [
                (call_data([], 5), "success", 0),
                (call_data([4, 5, 6], 5), "success", 1),
                (call_data([4, 5, 6], 7), "success", 3),
                (call_data([4, 5], 5)[:-64], "revert", None),
            ]
        ):
            with self.subTest(outcome=outcome, value=value):
                path = self.directory / f"case-{index}.json"
                path.write_text(
                    json.dumps({"version": 1, "calldata": calldata, "fuel": 128})
                )
                fixture = world.read(path, SOLAR, None, kind="mir")
                programs = [
                    mir_state.translate(
                        path.read_text(),
                        SOLAR,
                        VERSION,
                        fixture,
                        45,
                        "stopAt",
                        replay=self.replay,
                    )
                    for path in (self.before, self.after)
                ]
                checked = world_check.check(
                    *programs,
                    45,
                    replay_command="",
                    replay_executable=self.replay,
                    verbose=True,
                )
                self.assertEqual(checked.status, "verified")
                result = native.replay(
                    *programs, [], executable=self.replay, timeout=30
                )
                observed = list(map(json.loads, result["observations"]))
                self.assertEqual(observed[0], observed[1])
                self.assertEqual(observed[0]["outcome"], outcome)
                self.assertEqual(
                    observed[0]["data"],
                    [] if value is None else list(value.to_bytes(32, "big")),
                )
