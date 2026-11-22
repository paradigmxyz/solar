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
from validator import (
    builtin_lowering,
    evaluate,
    memory_induction,
    mir_state,
    native,
    storage_induction,
    world,
    world_check,
)

FIXTURES = ROOT.parent / "tests/ui/codegen/mir/indvar-simplify"
VERSION = "osaka"


def call_data(elements, needle):
    # Selector, dynamic array offset, needle, length, then array elements.
    return "0x828c8292" + "".join(
        f"{word:064x}" for word in (64, needle, len(elements), *elements)
    )


class InductionFixture(unittest.TestCase):
    @classmethod
    def matches_capture(cls, item):
        return (
            item["pass"] == "indvar-simplify" and item["region"]["name"] == cls.function
        )

    @classmethod
    def setUpClass(cls):
        work = tempfile.TemporaryDirectory()
        cls.addClassCleanup(work.cleanup)
        cls.directory = Path(work.name)
        compiler = [
            SOLAR,
            str(FIXTURES / cls.source),
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
            if cls.matches_capture(item)
        ]
        cls.before, cls.after = row["paths"]
        cls.replay = native.default(SOLAR)

    def check_pair(self, after, report, *, selection="--function"):
        result = subprocess.run(
            [
                CLI,
                "mir",
                self.before,
                after,
                selection,
                self.function,
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


class CalldataInductionTests(InductionFixture):
    source = "calldata_pointer_exit.sol"
    function = "stopAt"

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


class MemoryInductionTests(InductionFixture):
    source = "continued_counter.sol"
    function = "fill"

    @staticmethod
    def array(observation):
        data = bytes(observation["data"])
        return [
            int.from_bytes(data[i : i + 32], "big") for i in range(0, len(data), 32)
        ]

    def test_real_memory_fill(self):
        result, record = self.check_pair(self.after, self.directory / "valid.json")
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertEqual(record["status"], "verified")
        self.assertEqual(record["gas_policy"], "unpriced-structural")
        self.assertEqual(record["region"]["proof"], "kernel-checked-memory-fill")
        self.assertFalse(record["whole_pass"])

    def test_corrupted_counter_handoff_has_independent_witness(self):
        prefix, body = self.after.read_text().rsplit("fn @fill() {", 1)
        head, end, suffix = body.partition("\n}")
        mutated, changes = re.subn(
            r"( = shr v\d+, )5(?=\n\s+jump bb\d+\n)", r"\g<1>6", head
        )
        self.assertEqual(changes, 1)
        after = self.directory / "wrong-handoff.mir"
        after.write_text(prefix + "fn @fill() {" + mutated + end + suffix)
        result, record = self.check_pair(after, self.directory / "wrong-handoff.json")
        self.assertEqual(result.returncode, 1, result.stderr + result.stdout)
        self.assertEqual(record["status"], "counterexample")
        witness = record["witness"]
        self.assertEqual(witness["observations"], witness["native_observations"])
        self.assertEqual(
            list(map(self.array, witness["observations"])),
            [[32, 6, 8, 8, 9, 9, 9, 7], [32, 6, 8, 9, 9, 9, 9, 7]],
        )

    def test_replay_with_expanded_and_unexpanded_memory(self):
        for base, dirty in ((448, True), (65312, False)):
            with self.subTest(heap_floor=base, dirty=dirty):
                path = self.directory / f"heap-{base}.json"
                path.write_text(
                    json.dumps(
                        memory_induction.witness_world("fill", base, dirty=dirty)
                    )
                )
                fixture = world.read(path, SOLAR, None, kind="mir")
                programs = [
                    mir_state.translate(
                        source.read_text(),
                        SOLAR,
                        VERSION,
                        fixture,
                        45,
                        "fill",
                        replay=self.replay,
                    )
                    for source in (self.before, self.after)
                ]
                result = native.replay(
                    *programs, [], executable=self.replay, timeout=30
                )
                observed = list(map(json.loads, result["observations"]))
                self.assertEqual(observed[0], observed[1])
                self.assertEqual(observed[0]["outcome"], "success")
                self.assertEqual(self.array(observed[0]), [32, 6, 8, 8, 9, 9, 9, 7])


class StorageInductionTests(InductionFixture):
    source = "continued_counter.sol"
    function = "remove"

    @classmethod
    def matches_capture(cls, item):
        if not super().matches_capture(item):
            return False
        before, after = (path.read_text() for path in item["paths"])
        if not all("fn @remove() {" in text for text in (before, after)):
            return False
        shift = re.compile(r"^    v\d+ = shr v\d+, 1$", re.MULTILINE)
        return (
            shift.search(cls.remove_body(after)[1]) is not None
            and shift.search(cls.remove_body(before)[1]) is None
        )

    @staticmethod
    def remove_body(text):
        prefix, body = text.rsplit("fn @remove() {", 1)
        body, closing, suffix = body.partition("\n}")
        return prefix + "fn @remove() {", body, closing + suffix

    def test_real_search_to_shift_handoff(self):
        result, record = self.check_pair(
            self.after, self.directory / "valid.json", selection="--storage-loop"
        )
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertEqual(record["status"], "verified")
        self.assertEqual(record["gas_policy"], "unpriced-structural")
        self.assertEqual(record["region"]["kind"], "loop-region")
        self.assertEqual(record["region"]["proof"], "kernel-checked-storage-induction")
        self.assertFalse(record["whole_pass"])

    def test_broken_recovery_replays_first_different_write(self):
        prefix, body, suffix = self.remove_body(self.after.read_text())
        body, changes = re.subn(
            r"( = shr v\d+, )1$", r"\g<1>2", body, flags=re.MULTILINE
        )
        self.assertEqual(changes, 1)
        broken = self.directory / "wrong-index.mir"
        broken.write_text(prefix + body + suffix)
        result, record = self.check_pair(
            broken, self.directory / "broken.json", selection="--storage-loop"
        )
        self.assertEqual(result.returncode, 1, result.stderr + result.stdout)
        witness = record["witness"]
        self.assertEqual(witness["observations"], witness["native_observations"])
        self.assertEqual(
            [
                int.from_bytes(bytes(row["data"]), "big")
                for row in witness["observations"]
            ],
            [12450, 13450],
        )
        first = witness["first_different_write"]
        self.assertEqual(first["index"], 10)
        self.assertEqual((first["before"][0], first["after"][0]), (0x55, 0x55))
        self.assertEqual((first["before"][2], first["after"][2]), ("4", "3"))
        self.assertEqual(int(first["before"][1]) - int(first["after"][1]), 2)
        self.assertIn("First differing SSTORE (#11)", result.stdout)
        self.assertIn("--storage-loop remove", result.stdout)

    def test_changed_slot_cannot_use_certificate(self):
        prefix, body, suffix = self.remove_body(self.after.read_text())
        body, changes = re.subn(
            r"( = add v\d+, )3(?=\n +v\d+ = sload)", r"\g<1>4", body
        )
        self.assertEqual(changes, 1)
        broken = self.directory / "wrong-slot.mir"
        broken.write_text(prefix + body + suffix)
        result, record = self.check_pair(
            broken, self.directory / "wrong-slot.json", selection="--storage-loop"
        )
        self.assertEqual(result.returncode, 2, result.stderr + result.stdout)
        self.assertEqual(record["status"], "unsupported")

    def test_real_edges_in_both_interpreters(self):
        texts, models, _ = builtin_lowering.load_pair(
            (self.before, self.after), SOLAR, self.replay, VERSION, 45
        )
        cuts = [
            storage_induction.capture_region(
                builtin_lowering.functions(model)[self.function], pointer=pointer
            )
            for pointer, model in zip((False, True), models, strict=True)
        ]
        for needle, expected, clear_count in [
            (1, 23450, 10),
            (5, 12340, 2),
            (9, 12345, 0),
        ]:
            with self.subTest(needle=needle):
                fixture = storage_induction.call_world(
                    self.directory, cuts, SOLAR, needle
                )
                _, observations, _, traces = storage_induction.trace_pair(
                    self.directory, texts, fixture, SOLAR, self.replay, VERSION, 60
                )
                self.assertEqual(observations[0], observations[1])
                self.assertEqual(traces[0], traces[1])
                self.assertEqual(observations[0]["outcome"], "success")
                self.assertEqual(
                    int.from_bytes(bytes(observations[0]["data"]), "big"), expected
                )
                self.assertEqual(
                    sum(op == 0x55 for op, _, _ in traces[0]), 10 + clear_count
                )

        # Substitute a base at the end of the address space so the actual
        # loop bodies cross zero. A mapping hash cannot be chosen directly.
        base = 2**256 - 4
        wrapped = []
        for text in texts:
            prefix, body, suffix = self.remove_body(text)
            body, changes = re.subn(
                r" = keccak256 0, 64.*$",
                f" = add 0, {base:#x}",
                body,
                flags=re.MULTILINE,
            )
            self.assertEqual(changes, 1)
            wrapped.append(prefix + body + suffix)
        fixture = storage_induction.call_world(self.directory, cuts, SOLAR, 3)
        _, observations, _, traces = storage_induction.trace_pair(
            self.directory, wrapped, fixture, SOLAR, self.replay, VERSION, 60
        )
        self.assertEqual(observations[0], observations[1])
        self.assertEqual(traces[0], traces[1])
        self.assertEqual(
            traces[0][10:13],
            [[0x54, str(base), "1"], [0x54, str(base + 2), "2"], [0x54, "0", "3"]],
        )
        self.assertEqual(
            [(slot, value) for op, slot, value in traces[0][13:] if op == 0x55],
            [("0", "4"), ("1", "4"), ("2", "5"), ("3", "5"), ("4", "0"), ("5", "0")],
        )
