"""Check that MIR jump threading preserves storage and simultaneous phi inputs."""

import json
import re
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

    def test_real_tail_sharing_checks_path_local_values(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            before, after = (root / f"{name}.mir" for name in ("before", "after"))
            before.write_text("""@module Shared
fn @choose(arg0: i256, arg1: i1) -> i256 {
  bb0:
    jump bb1
  bb1:
    v0 = phi [bb0: arg0], [bb2: v1]
    v2 = ne v0, 0
    jumpi v2, bb2, bb3
  bb2:
    v1 = sub v0, 1
    jump bb1
  bb3:
    v3 = lt arg0, 4
    jumpi v3, bb4, bb6
  bb4:
    v10 = add arg0, 1
    v11 = add arg0, 2
    sstore 1, v10
    ret v10
  bb5:
    v20 = add arg0, 1
    v21 = add arg0, 2
    sstore 1, v20
    ret v20
  bb6:
    jumpi arg1, bb5, bb7
  bb7:
    ret 0
}
""")
            source = subprocess.check_output(
                [SOLAR, before, "-j1", "-Ogas", "-Zmir-pipeline=cfg-simplify"],
                text=True,
                timeout=10,
            )
            self.assertEqual(source.count("sstore 1,"), 1)
            after.write_text(source)
            report = root / "report.json"
            args = [
                CLI,
                "mir",
                before,
                after,
                "--function=choose",
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
            correspondence = data["import_correspondence"][1]["correspondence"][
                "choose"
            ]
            self.assertGreater(correspondence["reused_points"][1], 0)
            self.assertGreater(correspondence["live_pairs"], 0)

            wrong_value = re.search(r"(v\d+) = add arg0, 2", source)
            self.assertIsNotNone(wrong_value)
            broken = re.sub(r"sstore 1, v\d+", f"sstore 1, {wrong_value[1]}", source)
            self.assertNotEqual(broken, source)
            after.write_text(broken)
            result = subprocess.run(
                args, text=True, capture_output=True, timeout=90, check=False
            )
            self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
            self.assertEqual(json.loads(report.read_text())["status"], "unsupported")

            fixture = root / "world.json"
            fixture.write_text('{"version":1}')
            initial = world.read(fixture, SOLAR, None, kind="mir")
            programs = [
                Program(
                    ("world-entry", "mir"),
                    ["arg0", "arg1"],
                    "",
                    "",
                    replay_ir=text,
                    selector="choose",
                    evm_version="cancun",
                    world=initial,
                )
                for text in (before.read_text(), source, broken)
            ]
            for inputs in ([2, 0], [5, 1], [5, 0]):
                for after_program in programs[1:]:
                    observed = native.replay(
                        programs[0],
                        after_program,
                        inputs,
                        executable=native.default(SOLAR),
                        timeout=10,
                    )
                    a, b = map(json.loads, observed["observations"])
                    self.assertEqual(a["values"], b["values"])
                    self.assertEqual(
                        a == b, after_program is programs[1] or inputs == [5, 0]
                    )

    def test_literal_branch_keeps_the_selected_phi_input(self):
        tail = """  bb3:
    jumpi arg1, bb4, bb5
  bb4:
    jumpi false, bb6, bb7
  bb5:
    jump bb7
  bb6:
    ret 77
  bb7:
    v3: i256 = phi [bb4: 9], [bb5: 12]
    ret v3
}
"""
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            before = self.write_loop(root, "i256", tail)
            after = self.simplify(root)
            self.assertNotIn("jumpi false,", after)
            record = self.check_loop(root, 0)
            source = record["import_correspondence"][0]["correspondence"]["choose"]
            self.assertEqual(source["literal_branches"], [1, 0])
            wrong = after.replace(": 9]", ": 10]")
            self.assertNotEqual(after, wrong)
            (root / "after.mir").write_text(wrong)
            self.check_loop(root, 2)
            for args, expected in (
                ([2, 1], [["9"], ["10"]]),
                ([2, 0], [["12"], ["12"]]),
            ):
                with self.subTest(args=args):
                    self.assertEqual(
                        self.replay_loop(root, before, wrong, args), expected
                    )

    def test_product_construction_rejects_an_uninitialized_return(self):
        tail = """  bb3:
    v3 = insert_value struct0, undef struct0, 0, arg0
    v4 = insert_value struct0, v3, 1, 17
    ret v4
}
"""
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            before = self.write_loop(
                root, "struct0", tail, "@types\n  struct0: {i256, i256}\n"
            )
            after = self.simplify(root)
            self.assertIn("insert_value struct0, undef struct0", after)
            record = self.check_loop(root, 0)
            source = record["import_correspondence"][0]["correspondence"]["choose"]
            self.assertEqual(source["product_initialization"], [True, True])
            self.assertEqual(
                self.replay_loop(root, before, after, [2, 0]),
                [["2", "17"], ["2", "17"]],
            )
            # Both graphs keep the same insert sequence, but the returned
            # product has a field that was never written.
            for name, text in (("before", before), ("after", after)):
                unfinished = text.replace(", 1, 17", ", 0, 17")
                self.assertNotEqual(unfinished, text)
                (root / f"{name}.mir").write_text(unfinished)
            self.check_loop(root, 2)

    def write_loop(self, root, return_type, tail, declarations=""):
        text = (
            f"@module Loop\n{declarations}"
            f"fn @choose(arg0: i256, arg1: i1) -> {return_type} {{\n"
            """  bb0:
    jump bb1
  bb1:
    v0 = phi [bb0: arg0], [bb2: v1]
    v2 = ne v0, 0
    jumpi v2, bb2, bb3
  bb2:
    v1 = sub v0, 1
    jump bb1
""" + tail
        )
        (root / "before.mir").write_text(text)
        return text

    def simplify(self, root):
        after = subprocess.check_output(
            [SOLAR, root / "before.mir", "-j1", "-Ogas", "-Zmir-pipeline=cfg-simplify"],
            text=True,
            timeout=10,
        )
        (root / "after.mir").write_text(after)
        return after

    def check_loop(self, root, status, *, fixture=None):
        report = root / "report.json"
        result = subprocess.run(
            [
                CLI,
                "mir",
                root / "before.mir",
                root / "after.mir",
                "--function=choose",
                "--solar",
                SOLAR,
                "--timeout=30",
                "--report",
                report,
                *(("--world", fixture) if fixture else ()),
            ],
            text=True,
            capture_output=True,
            timeout=90,
            check=False,
        )
        self.assertEqual(result.returncode, status, result.stdout + result.stderr)
        record = json.loads(report.read_text())
        if status == 0:
            self.assertEqual(
                record["region"]["proof"], "kernel-checked-typed-jump-chain"
            )
        return record

    def replay_loop(self, root, before, after, args):
        fixture = root / "world.json"
        fixture.write_text('{"version":1}')
        initial = world.read(fixture, SOLAR, None, kind="mir")
        programs = [
            Program(
                ("world-entry", "mir"),
                [f"arg{i}" for i in range(len(args))],
                "",
                "",
                replay_ir=text,
                selector="choose",
                evm_version="cancun",
                world=initial,
            )
            for text in (before, after)
        ]
        replay = native.replay(
            *programs, args, executable=native.default(SOLAR), timeout=10
        )
        return [json.loads(row)["values"] for row in replay["observations"]]

    def test_egraph_loop_with_wrapping_words_and_pointer_callee(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            before, after = (root / f"{name}.mir" for name in ("before", "after"))
            before.write_text("""@module WrappingLoop
fn @roundtrip(arg0: i256) -> i256 {
  bb0:
    v0 = inttoptr i256 arg0 to ptr<memory>
    v1 = ptrtoint ptr<memory> v0 to i256
    ret v1
}

fn @step() -> i256 {
  bb0:
    v0 = sload 1
    jump bb1
  bb1:
    v1 = phi [bb0: v0], [bb2: v4]
    v2 = phi [bb0: 0], [bb2: v5]
    v3 = lt v2, 2
    jumpi v3, bb2, bb3
  bb2:
    v4 = add v1, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0
    v5 = add v2, 1
    jump bb1
  bb3:
    v6 = icall @roundtrip, v1
    ret v6
}
""")
            output = subprocess.check_output(
                [SOLAR, before, "-j1", "-Ogas", "-Zmir-pipeline=egraph"],
                text=True,
                timeout=20,
            )
            after.write_text(output)
            report = root / "report.json"
            command = [
                CLI,
                "mir",
                before,
                after,
                "--function=step",
                "--solar",
                SOLAR,
                "--timeout=30",
                "--report",
                report,
            ]
            result = subprocess.run(
                command, text=True, capture_output=True, timeout=90, check=False
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            record = json.loads(report.read_text())
            self.assertEqual(
                record["region"]["proof"], "kernel-checked-typed-jump-chain"
            )
            proofs = record["import_correspondence"][0]["correspondence"]
            self.assertEqual(proofs["step"]["word_rewrites"], 1)
            self.assertEqual(proofs["roundtrip"]["proof"], "carrier-copies")

            broken = root / "broken.mir"
            wrong, count = re.subn(r"(\bsub v\d+, )32\b", r"\g<1>33", output)
            self.assertEqual(count, 1)
            broken.write_text(wrong)
            command[3] = broken
            result = subprocess.run(
                command, text=True, capture_output=True, timeout=90, check=False
            )
            self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
            self.assertEqual(json.loads(report.read_text())["status"], "unsupported")
            fixture = root / "world.json"
            fixture.write_text(
                '{"version":1,"accounts":[{"address":65536,"storage":{"1":"7"}}]}'
            )
            result = subprocess.run(
                [*command, "--world", fixture],
                text=True,
                capture_output=True,
                timeout=90,
                check=False,
            )
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            record = json.loads(report.read_text())
            self.assertEqual(record["evidence"], "kernel-and-rust-replay")
            self.assertEqual(
                record["witness"]["observations"],
                record["witness"]["native_observations"],
            )

    def test_egraph_preserves_a_loop_increment_guard(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            before, after = (root / f"{name}.mir" for name in ("before", "after"))
            before.write_text("""@module OverflowLoop
fn @choose() -> i256 {
  bb0:
    v0 = sload 1
    jump bb1
  bb1:
    v1 = phi [bb0: v0], [bb3: v3]
    v2 = phi [bb0: 0], [bb3: 1]
    v3 = add v1, 1
    v4 = lt v3, v1
    jumpi v4, bb2, bb3
  bb2:
    ret v2
  bb3:
    v5 = eq v2, 0
    jumpi v5, bb1, bb4
  bb4:
    ret v3
}
""")
            output = subprocess.check_output(
                [SOLAR, before, "-j1", "-Ogas", "-Zmir-pipeline=egraph"],
                text=True,
                timeout=20,
            )
            after.write_text(output)
            record = self.check_loop(root, 0)
            self.assertEqual(
                record["import_correspondence"][0]["correspondence"]["choose"][
                    "increment_guard_rewrites"
                ],
                1,
            )

            wrong, count = re.subn(
                r"(v\d+ = eq (v\d+), )0(\n    jumpi v\d+, bb2, bb3)",
                r"\g<1>1\g<3>",
                output,
            )
            self.assertEqual(count, 1)
            after.write_text(wrong)
            self.check_loop(root, 2)
            fixture = root / "world.json"
            fixture.write_text(
                json.dumps(
                    {
                        "version": 1,
                        "accounts": [
                            {"address": 65536, "storage": {"1": str(2**256 - 1)}}
                        ],
                    }
                )
            )
            record = self.check_loop(root, 1, fixture=fixture)
            self.assertEqual(record["evidence"], "kernel-and-rust-replay")
            witness = record["witness"]
            self.assertEqual(witness["observations"], witness["native_observations"])

    def test_egraph_storage_loop_replaces_word_scales_and_masks(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            before, after = (root / f"{name}.mir" for name in ("before", "after"))
            before.write_text("""@module ScalingLoop
fn @choose() -> i256 {
  bb0:
    v0 = sload 1
    v1 = sload 2
    jump bb1
  bb1:
    v2 = phi [bb0: false], [bb1: true]
    v3 = div v0, 32
    sstore 3, v3
    v4 = mul v0, 32
    sstore 4, v4
    v5 = shl 1, v1
    v6 = sub v5, 1
    v7 = not v6
    sstore 5, v7
    jumpi v2, bb2, bb1
  bb2:
    ret v3
}
""")
            output = subprocess.check_output(
                [SOLAR, before, "-j1", "-Ogas", "-Zmir-pipeline=egraph"],
                text=True,
                timeout=20,
            )
            after.write_text(output)
            record = self.check_loop(root, 0)
            self.assertEqual(
                record["import_correspondence"][0]["correspondence"]["choose"][
                    "word_rewrites"
                ],
                3,
            )
            wrong, count = re.subn(r"(shr v\d+, )5\b", r"\g<1>4", output)
            self.assertEqual(count, 1)
            after.write_text(wrong)
            self.check_loop(root, 2)
            fixture = root / "world.json"
            fixture.write_text(
                '{"version":1,"accounts":[{"address":65536,"storage":{"1":"64","2":"8"}}]}'
            )
            record = self.check_loop(root, 1, fixture=fixture)
            self.assertEqual(record["evidence"], "kernel-and-rust-replay")
            self.assertEqual(
                record["witness"]["observations"],
                record["witness"]["native_observations"],
            )

    def test_cse_erases_pointer_copies_in_a_slice_loop(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            before, after = (root / f"{name}.mir" for name in ("before", "after"))
            source = """@module SliceLoop
fn @choose(arg0: slice<memory>, arg1: i1) -> i256 {
  bb0:
    v0 = extract_value slice<memory>, arg0, 0
    v1 = ptrtoint ptr<memory> v0 to i256
    mstore v1, 7
    jump bb1
  bb1:
    v2 = phi [bb0: arg1], [bb1: false]
    v3 = ptrtoint ptr<memory> v0 to i256
    v4 = mload v3
    v5 = add v4, 1
    mstore v3, v5
    jumpi v2, bb1, bb2
  bb2:
    ret v5
}
"""
            before.write_text(source)
            output = subprocess.check_output(
                [SOLAR, before, "-j1", "-Ogas", "-Zmir-pipeline=cse"],
                text=True,
                timeout=20,
            )
            self.assertEqual(output.count("ptrtoint"), 1)
            after.write_text(output)
            record = self.check_loop(root, 0)
            self.assertEqual(
                record["import_correspondence"][0]["correspondence"]["choose"]["proof"],
                "carrier-copies",
            )
            self.assertEqual(
                self.replay_loop(root, source, output, [128, 32, 1]),
                [["9"], ["9"]],
            )

            wrong, count = re.subn(r"(v\d+ = mload) v\d+", r"\1 160", output)
            self.assertEqual(count, 1)
            after.write_text(wrong)
            self.check_loop(root, 2)
            self.assertEqual(
                self.replay_loop(root, source, wrong, [128, 32, 1]),
                [["9"], ["1"]],
            )

    def test_egraph_composes_memory_offsets_through_word_copies(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            before, after = (root / f"{name}.mir" for name in ("before", "after"))
            source = """@module MemoryOffsets
fn @choose(arg0: ptr<memory>, arg1: i1) -> i256 {
  bb0:
    v0 = ptrtoint ptr<memory> arg0 to i256
    jump bb1
  bb1:
    v1 = phi [bb0: arg1], [bb1: false]
    v2 = add v0, 64
    v3 = inttoptr i256 v2 to ptr<memory>
    v4 = ptrtoint ptr<memory> v3 to i256
    v5 = add v4, 32
    mstore v5, 7
    v6 = add v0, 96
    v7 = mload v6
    jumpi v1, bb1, bb2
  bb2:
    ret v7
}
"""
            before.write_text(source)
            output = subprocess.check_output(
                [SOLAR, before, "-j1", "-Ogas", "-Zmir-pipeline=egraph"],
                text=True,
                timeout=20,
            )
            after.write_text(output)
            record = self.check_loop(root, 0)
            relation = record["import_correspondence"][0]["correspondence"]["choose"]
            self.assertEqual(relation["proof"], "carrier-copies")
            self.assertGreater(relation["offsets"], 0)
            self.assertEqual(
                self.replay_loop(root, source, output, [128, 1]), [["7"], ["7"]]
            )
            wrong, count = re.subn(r"\bmstore v\d+, 7\b", "mstore 256, 7", output)
            self.assertEqual(count, 1)
            after.write_text(wrong)
            self.check_loop(root, 2)
            self.assertEqual(
                self.replay_loop(root, source, wrong, [128, 1]), [["7"], ["0"]]
            )

    def test_cse_retains_the_calldata_size_across_effects(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            before, after = (root / f"{name}.mir" for name in ("before", "after"))
            before.write_text("""@module InputSize
fn @choose() -> i256 {
  bb0:
    v0 = calldatasize
    jump bb1
  bb1:
    v1 = phi [bb0: false], [bb1: true]
    mstore 128, v0
    v2 = calldatasize
    mstore 160, v2
    jumpi v1, bb2, bb1
  bb2:
    ret v2
}
""")
            output = subprocess.check_output(
                [SOLAR, before, "-j1", "-Ogas", "-Zmir-pipeline=cse"],
                text=True,
                timeout=20,
            )
            self.assertEqual(output.count("calldatasize"), 1)
            after.write_text(output)
            record = self.check_loop(root, 0)
            relation = record["import_correspondence"][0]["correspondence"]["choose"]
            self.assertEqual(relation["proof"], "carrier-copies")
            after.write_text(output.replace("= calldatasize", "= returndatasize"))
            self.check_loop(root, 2)
            fixture = root / "world.json"
            fixture.write_text('{"version":1,"calldata":"0x112233"}')
            record = self.check_loop(root, 1, fixture=fixture)
            self.assertEqual(record["evidence"], "kernel-and-rust-replay")
            witness = record["witness"]
            self.assertEqual(witness["observations"], witness["native_observations"])

    def test_egraph_discharges_rounded_extent_overflow_from_length(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            before, after = (root / f"{name}.mir" for name in ("before", "after"))
            source = """@module Extent
fn @choose() -> i256 {
  bb0:
    v0 = sload 1
    v1 = shr v0, 1
    v2 = sload 2
    v3 = and v1, v2
    v4 = add v3, 31
    v5 = div v4, 32
    v6 = add v3, 31
    v7 = and v6, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0
    v8 = add v7, 32
    v9 = lt v8, v3
    jumpi v9, bb1, bb2
  bb1:
    ret 77
  bb2:
    v10 = phi [bb0: false], [bb2: true]
    sstore 3, v5
    jumpi v10, bb3, bb2
  bb3:
    ret v8
}
"""
            before.write_text(source)
            output = subprocess.check_output(
                [SOLAR, before, "-j1", "-Ogas", "-Zmir-pipeline=egraph"],
                text=True,
                timeout=20,
            )
            self.assertIn("jumpi false, bb1, bb2", output)
            after.write_text(output)
            record = self.check_loop(root, 0)
            self.assertEqual(
                record["import_correspondence"][0]["correspondence"]["choose"][
                    "rounded_extent_guards"
                ],
                [1, 0],
            )

            # Drop the size bound on both sides. Removing the check is now invalid.
            before.write_text(source.replace("shr v0, 1", "shr v0, 0"))
            after.write_text(output.replace("shr v0, 1", "shr v0, 0"))
            self.check_loop(root, 2)
            fixture = root / "world.json"
            full_word = str(2**256 - 1)
            fixture.write_text(
                json.dumps(
                    {
                        "version": 1,
                        "accounts": [
                            {
                                "address": 65536,
                                "storage": {"1": full_word, "2": full_word},
                            }
                        ],
                    }
                )
            )
            record = self.check_loop(root, 1, fixture=fixture)
            self.assertEqual(record["evidence"], "kernel-and-rust-replay")
            witness = record["witness"]
            self.assertEqual(witness["observations"], witness["native_observations"])

    def test_egraph_composes_slice_reuse_and_checked_guards(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            before, after = (root / f"{name}.mir" for name in ("before", "after"))
            before.write_text("""@module SliceChecks
fn @choose(arg0: slice<calldata>) -> i256 {
  bb0:
    v0 = extract_value slice<calldata>, arg0, 1
    v1 = mul v0, 1
    v2 = div v1, 1
    v3 = eq v2, v0
    v4 = or v3, false
    v5 = zext i1 v4 to i256
    v6 = eq v5, 0
    icall panic_if<0x41>, v6
    jump bb1
  bb1:
    v7 = phi [bb0: 0], [bb2: v9]
    v8 = lt v7, v1
    jumpi v8, bb2, bb3
  bb2:
    v9 = checked_add uint256, v7, 1
    v10 = extract_value slice<calldata>, arg0, 1
    v11 = lt v7, v10
    v12 = eq v11, false
    icall panic_if<0x32>, v12
    jump bb1
  bb3:
    ret v7
}
""")
            output = subprocess.check_output(
                [SOLAR, before, "-j1", "-Ogas", "-Zmir-pipeline=egraph"],
                text=True,
                timeout=20,
            )
            after.write_text(output)
            record = self.check_loop(root, 0)
            old, _ = record["import_correspondence"][0]["correspondence"]["choose"][
                "scalar_checks"
            ]
            self.assertEqual(
                old,
                {
                    "argument_projections": 1,
                    "check_polarities": 1,
                    "inactive_checks": 1,
                    "reused_comparisons": 1,
                    "scalar_identities": 6,
                },
            )
            wrong = output.replace("panic_if_zero<0x32>", "panic_if<0x32>")
            self.assertNotEqual(wrong, output)
            after.write_text(wrong)
            self.check_loop(root, 2)
            fixture = root / "world.json"
            fixture.write_text('{"version":1}')
            record = self.check_loop(root, 1, fixture=fixture)
            self.assertEqual(record["evidence"], "kernel-and-rust-replay")
            witness = record["witness"]
            self.assertEqual(witness["observations"], witness["native_observations"])

    def test_cfg_simplify_checks_mutually_recursive_callees(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            before, after = (root / f"{name}.mir" for name in ("before", "after"))
            before.write_text("""@module RecursiveCalls
fn @entry() -> i256 {
  bb0:
    v0 = sload 1
    v1 = icall @count, v0
    ret v1
}

fn @count(arg0: i256) -> i256 {
  bb0:
    v0 = eq arg0, 0
    jumpi v0, bb1, bb2
  bb1:
    jump bb3
  bb2:
    v1 = sub arg0, 1
    v2 = icall @next, v1
    v3 = add v2, arg0
    ret v3
  bb3:
    ret 0
}

fn @next(arg0: i256) -> i256 {
  bb0:
    v0 = icall @count, arg0
    ret v0
}
""")
            output = subprocess.check_output(
                [SOLAR, before, "-j1", "-Ogas", "-Zmir-pipeline=cfg-simplify"],
                text=True,
                timeout=20,
            )
            after.write_text(output)
            report = root / "report.json"
            command = [
                CLI,
                "mir",
                before,
                after,
                "--function=entry",
                "--solar",
                SOLAR,
                "--timeout=30",
                "--report",
                report,
            ]
            result = subprocess.run(
                command, text=True, capture_output=True, timeout=90, check=False
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            record = json.loads(report.read_text())
            self.assertEqual(
                record["region"]["proof"], "kernel-checked-typed-jump-chain"
            )
            self.assertEqual(record["region"]["functions"], ["count", "entry", "next"])
            evidence = record["import_correspondence"][0]
            self.assertTrue(evidence["nested_calls"]["recursive"])
            self.assertEqual(len(evidence["nested_calls"]["functions"]), 3)
            self.assertEqual(
                evidence["correspondence"]["count"]["proof"], "path-correspondence"
            )

            broken = root / "broken.mir"
            wrong, count = re.subn(r"\bret 0\b", "ret 1", output)
            self.assertEqual(count, 1)
            broken.write_text(wrong)
            command[3] = broken
            result = subprocess.run(
                command, text=True, capture_output=True, timeout=90, check=False
            )
            self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
            self.assertEqual(json.loads(report.read_text())["status"], "unsupported")

            fixture = root / "world.json"
            fixture.write_text(
                '{"version":1,"accounts":[{"address":65536,"storage":{"1":"4"}}]}'
            )
            result = subprocess.run(
                [*command, "--world", fixture],
                text=True,
                capture_output=True,
                timeout=90,
                check=False,
            )
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            record = json.loads(report.read_text())
            self.assertEqual(record["evidence"], "kernel-and-rust-replay")
            self.assertEqual(
                record["witness"]["observations"],
                record["witness"]["native_observations"],
            )
