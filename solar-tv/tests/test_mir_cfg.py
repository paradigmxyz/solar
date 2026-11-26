"""Check typed MIR control flow, scalar reuse and retained effects."""

import json
import re
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from io import StringIO
from pathlib import Path
from unittest.mock import patch

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import check, mir_cfg, native, world
from validator.common import Program, Unsupported


class MirCfgTests(unittest.TestCase):
    def test_threading_uses_the_selected_phi_input(self):
        source = """@module ThreadPhi
fn @f(arg0: i1, arg1: i1) -> i256 {
  bb0:
    jumpi arg0, bb1, bb2
  bb1:
    jump bb3
  bb2:
    jump bb3
  bb3:
    v0 = phi [bb1: arg1], [bb2: false]
    jumpi v0, bb4, bb5
  bb4:
    sstore 7, 21
    ret 21
  bb5:
    sstore 7, 22
    ret 22
}
"""
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            before, after = (folder / f"{side}.mir" for side in ("before", "after"))
            before.write_text(source)
            threaded = subprocess.check_output(
                [
                    SOLAR,
                    before,
                    "-j1",
                    "-Ogas",
                    "-Zmir-pipeline=jump-threading",
                    "-Zvalidate-ir=true",
                ],
                text=True,
                timeout=30,
            )
            after.write_text(threaded)
            executable = native.default(SOLAR)
            args = ((before, after), SOLAR, executable, "cancun", 30, "f")
            result = mir_cfg.check(*args)
            self.assertEqual(result.status, "verified")
            evidence = result.program.import_evidence["correspondence"]["f"]
            self.assertEqual(evidence["threaded_phi_guards"], [1, 0])

            wrong, count = re.subn(
                r"(jumpi arg1, )(bb\d+), (bb\d+)", r"\g<1>\g<3>, \g<2>", threaded
            )
            self.assertEqual(count, 1)
            after.write_text(wrong)
            with self.assertRaises(Unsupported):
                mir_cfg.check(*args)

            path = folder / "world.json"
            path.write_text('{"version":1}')
            initial = world.read(path, SOLAR, None, kind="mir")
            programs = [
                Program(
                    ("world-entry", "mir"),
                    ["arg0", "arg1"],
                    "",
                    "",
                    world=initial,
                    selector="f",
                    canonical_ir=text,
                )
                for text in (source, threaded, wrong)
            ]
            for operands, expected in (([0, 1], "22"), ([1, 0], "22"), ([1, 1], "21")):
                replay = native.replay(
                    *programs[:2], operands, executable=executable, timeout=20
                )
                self.assertEqual(*replay["observations"])
                self.assertEqual(
                    json.loads(replay["observations"][0])["values"], [expected]
                )
            replay = native.replay(
                programs[0], programs[2], [1, 1], executable=executable, timeout=20
            )
            self.assertEqual(
                [json.loads(x)["values"] for x in replay["observations"]],
                [["21"], ["22"]],
            )

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

            with (
                patch.object(sys, "argv", list(map(str, args))),
                patch.object(
                    check, "load", side_effect=subprocess.TimeoutExpired("load", 30)
                ) as load,
                redirect_stdout(StringIO()) as stdout,
                redirect_stderr(StringIO()) as stderr,
            ):
                self.assertEqual(check.main(), 0, stdout.getvalue() + stderr.getvalue())
                load.assert_called_once()
            recovered = json.loads(report.read_text())
            self.assertEqual(recovered["evidence"], "kernel-proof")
            self.assertEqual(
                recovered["import_correspondence"], data["import_correspondence"]
            )

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

    def check_loop(self, root, status, *, fixture=None, valid_heap=False):
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
                *(("--valid-heap",) if valid_heap else ()),
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
                record["region"]["proof"],
                "kernel-checked-valid-heap-read-reuse"
                if valid_heap
                else "kernel-checked-typed-jump-chain",
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

    def test_egraph_reuses_caller_and_keeps_the_gas_request(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            before, after = (root / f"{name}.mir" for name in ("before", "after"))
            source = """@module ScalarReuse
fn @choose(arg0: i256) -> i256 {
  bb0:
    jump bb4
  bb4:
    v0 = caller
    v1 = zext i160 v0 to i256
    v2 = gt arg0, 0
    v3 = gas
    jumpi v2, bb1, bb2
  bb1:
    v4 = caller
    v5 = zext i160 v4 to i256
    v6 = and v5, 0xffffffffffffffffffffffffffffffffffffffff
    sstore 1, v6
    jump bb3
  bb2:
    sstore 2, v1
    jump bb3
  bb3:
    v7 = phi [bb1: v5], [bb2: v1]
    v8 = add v3, v7
    ret v8
}
"""
            before.write_text(source)
            lowered = subprocess.check_output(
                [SOLAR, before, "-j1", "-Zmir-pipeline=egraph", "-Zvalidate-ir=true"],
                text=True,
                timeout=20,
            )
            self.assertEqual(lowered.count(" = caller"), 1)
            self.assertEqual(lowered.count(" = gas"), 1)
            after.write_text(lowered)
            record = self.check_loop(root, 0)
            evidence = record["import_correspondence"][0]["correspondence"]["choose"]
            self.assertEqual(evidence["scalar_reuse"][0]["nonzero_tests"], 1)
            self.assertEqual(evidence["scalar_reuse"][0]["uniform_phis"], 1)
            self.assertGreaterEqual(evidence["scalar_reuse"][0]["reused_values"], 2)

            without_sample = lowered.replace(" = gas", " = add 5, 0")
            after.write_text(without_sample)
            self.assertEqual(self.check_loop(root, 2)["status"], "unsupported")

            # Fix the shared reply to run the storage mutation in both MIR executors.
            before.write_text(source.replace(" = gas", " = add 5, 0"))
            after.write_text(without_sample.replace("sstore 1,", "sstore 17,"))
            fixture = root / "world.json"
            fixture.write_text('{"version":1,"caller":131072}')
            record = self.check_loop(root, 1, fixture=fixture)
            self.assertEqual(record["evidence"], "kernel-and-rust-replay")
            self.assertEqual(
                record["witness"]["observations"],
                record["witness"]["native_observations"],
            )

    def test_egraph_preserves_effective_call_operands_and_returndata(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            before, after = (root / f"{name}.mir" for name in ("before", "after"))
            source = """@module EffectiveCall
fn @choose(arg0: i256) -> i256 {
  bb0:
    v0 = trunc i256 arg0 to i160
    v1 = zext i160 v0 to i256
    v2 = codesize
    v3 = gas
    v4 = call v3, v1, 0, v2, 0, v2, 0
    v5 = returndatasize
    v6 = add v5, 31
    v7 = and v6, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0
    v8 = add v7, 32
    v9 = lt v8, v5
    jumpi v9, bb1, bb2
  bb1:
    mstore 32, 65
    mstore 0, 0x4e487b71
    revert 28, 36
  bb2:
    ret v8
}
"""
            before.write_text(source)
            lowered = subprocess.check_output(
                [SOLAR, before, "-j1", "-Zmir-pipeline=egraph", "-Zvalidate-ir=true"],
                text=True,
                timeout=20,
            )
            after.write_text(lowered)
            record = self.check_loop(root, 0)
            evidence = record["import_correspondence"][0]["correspondence"]["choose"]
            scalar = evidence["scalar_reuse"][0]
            self.assertEqual(scalar["effective_call_targets"], 1)
            self.assertEqual(scalar["empty_call_ranges"], 2)
            self.assertEqual(scalar["returndata_extent_guards"], 1)
            nonempty = re.sub(
                r"( = call [^\n]+), 0$", r"\1, 32", lowered, flags=re.MULTILINE
            )
            self.assertNotEqual(lowered, nonempty)
            after.write_text(nonempty)
            self.check_loop(root, 2)
            changed_guard = lowered.replace("add v5, 31", "add v5, 63")
            self.assertNotEqual(lowered, changed_guard)
            after.write_text(changed_guard)
            self.check_loop(root, 2)

            # Supply a fixed sample and a concrete child to the native interpreter.
            fixture = root / "world.json"
            fixture.write_text(
                '{"version":1,"mir_layout":{"code":"0x000000"},'
                '"accounts":[{"address":65537,"nonce":1,"code":"0x602a60005260206000f3"}]}'
            )
            initial = world.read(fixture, SOLAR, None, kind="mir")
            programs = [
                Program(
                    ("world-entry", "mir"),
                    ["arg0"],
                    "",
                    "",
                    replay_ir=text.replace(" = gas", " = add 100000, 0"),
                    selector="choose",
                    evm_version="cancun",
                    world=initial,
                )
                for text in (source, lowered, nonempty)
            ]
            for target in programs[1:]:
                result = native.replay(
                    programs[0],
                    target,
                    [65537 + 2**200],
                    executable=native.default(SOLAR),
                    timeout=20,
                )
                left, right = map(json.loads, result["observations"])
                self.assertEqual(left == right, target is programs[1])

    def test_cse_requires_a_live_heap_producer(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            before, after = (root / f"{name}.mir" for name in ("before", "after"))
            source = """@module HeapReuse
fn @choose(arg0: i256) -> i256 {
  bb0:
    v0 = alloc checked 128
    mstore v0, arg0
    v1 = ptradd v0, 64
    mstore v1, 0
    v2 = fmp
    mstore v2, 17
    v3 = mload v0
    ret v3
}
"""
            reduced = source.replace(
                "    v3 = mload v0\n    ret v3\n", "    ret arg0\n"
            )
            before.write_text(source)
            after.write_text(reduced)
            self.check_loop(root, 2)
            record = self.check_loop(root, 0, valid_heap=True)
            self.assertTrue(
                any(
                    "initialized logical MIR entry" in item
                    for item in record["assumptions"]
                )
            )

            overlapping = [
                text.replace("mstore v2, 17", "mstore v0, 17")
                for text in (source, reduced)
            ]
            before.write_text(overlapping[0])
            after.write_text(overlapping[1])
            self.check_loop(root, 2, valid_heap=True)

            fixture = root / "heap.json"
            for floor, texts, values in (
                (128, (source, reduced), [["7"], ["7"]]),
                (0, (source, reduced), [["17"], ["7"]]),
                (128, overlapping, [["17"], ["7"]]),
            ):
                fixture.write_text(
                    json.dumps(
                        {
                            "version": 1,
                            "memory": {
                                "size": 96,
                                "data": "0x"
                                + (bytes(64) + floor.to_bytes(32, "big")).hex(),
                            },
                        }
                    )
                )
                config = world.read(fixture, SOLAR, None, kind="mir")
                programs = [
                    Program(
                        ("world-entry", "mir"),
                        ["arg0"],
                        "",
                        "",
                        replay_ir=text,
                        selector="choose",
                        evm_version="cancun",
                        world=config,
                    )
                    for text in texts
                ]
                replay = native.replay(
                    *programs, [7], executable=native.default(SOLAR), timeout=10
                )
                observations = [json.loads(item) for item in replay["observations"]]
                self.assertEqual([item["values"] for item in observations], values)
                self.assertEqual(
                    observations[0] == observations[1], values[0] == values[1]
                )

    def test_raw_heap_reload_requires_a_preserved_cursor(self):
        source = """@module RawHeap
fn @choose(arg0: i256, arg1: i256) -> i256 {
  bb0:
    v0 = mload 64
    v1 = add v0, 128
    mstore 64, v1
    v2 = add v0, 64
    mstore v2, arg0
    v3 = call 100000, arg1, 0, v2, 32, 0, 0
    v4 = mload 64
    ret v4
}
"""
        optimized = source.replace("    v4 = mload 64\n    ret v4", "    ret v1")
        clobbered = [
            text.replace("v2, 32, 0, 0", "v2, 32, 64, 32")
            for text in (source, optimized)
        ]
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            before, after = (root / f"{name}.mir" for name in ("before", "after"))
            before.write_text(source)
            after.write_text(optimized)
            record = self.check_loop(root, 0, valid_heap=True)
            selected = record["import_correspondence"][0]["correspondence"]["choose"]
            self.assertEqual(selected["state_read_reuse"], [{"memory": 1}, {}])
            (flow,) = selected["heap_cursor"][0].values()
            self.assertEqual(
                (flow["raw_cursor_loads"], flow["raw_cursor_stores"]), (2, 1)
            )

            before.write_text(clobbered[0])
            after.write_text(clobbered[1])
            with self.assertRaises(Unsupported):
                mir_cfg.check(
                    (before, after),
                    SOLAR,
                    native.default(SOLAR),
                    "cancun",
                    30,
                    "choose",
                    valid_heap=True,
                )

            fixture = root / "world.json"
            token = 65537
            for pointer, texts, expected in (
                (128, (source, optimized), ["256", "256"]),
                (0, (source, optimized), ["7", "128"]),
                (128, clobbered, ["42", "256"]),
            ):
                fixture.write_text(
                    json.dumps(
                        {
                            "version": 1,
                            "memory": {
                                "data": "0x"
                                + (bytes(64) + pointer.to_bytes(32, "big")).hex()
                            },
                            "accounts": [
                                {"address": token, "code": "0x602a5f5260205ff3"}
                            ],
                        }
                    )
                )
                config = world.read(fixture, SOLAR, None, kind="mir")
                programs = [
                    Program(
                        ("world-entry", "mir"),
                        ["arg0", "arg1"],
                        "",
                        "",
                        replay_ir=text,
                        selector="choose",
                        evm_version="cancun",
                        world=config,
                    )
                    for text in texts
                ]
                replay = native.replay(
                    *programs, [7, token], executable=native.default(SOLAR), timeout=20
                )
                observations = list(map(json.loads, replay["observations"]))
                self.assertEqual(
                    [item["values"] for item in observations],
                    [[value] for value in expected],
                )
                self.assertEqual(
                    observations[0] == observations[1], expected[0] == expected[1]
                )

    def test_cse_reuses_a_warm_slot_across_a_disjoint_write(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            before, after = (root / f"{name}.mir" for name in ("before", "after"))
            source = """@module WarmSlotReuse
fn @choose(arg0: i256) -> i256 {
  bb0:
    v0 = mapping_slot arg0, 3
    v1 = sload v0
    v2 = ne arg0, 0
    jumpi v2, bb1, bb2
  bb1:
    v3 = add v0, 1
    sstore v3, 19
    jump bb3
  bb2:
    jump bb3
  bb3:
    v4 = mapping_slot arg0, 3
    v5 = sload v4
    v6 = add v1, v5
    ret v6
}
"""
            reduced = source.replace(
                "    v4 = mapping_slot arg0, 3\n    v5 = sload v4\n", ""
            ).replace("add v1, v5", "add v1, v1")
            before.write_text(source)
            after.write_text(reduced)
            proof = self.check_loop(root, 0)
            reuse = proof["import_correspondence"][0]["correspondence"]["choose"][
                "state_read_reuse"
            ]
            self.assertEqual(reuse, [{"mapping": 1, "sload": 1}, {}])

            original, optimized = [
                text.replace("add v0, 1", "add v0, 0") for text in (source, reduced)
            ]
            before.write_text(original)
            after.write_text(optimized)
            self.check_loop(root, 2)
            matched = self.replay_loop(root, source, reduced, [7])
            broken = self.replay_loop(root, original, optimized, [7])
            self.assertEqual(matched, [["0"], ["0"]])
            self.assertEqual(broken, [["19"], ["0"]])

    def test_dce_requires_active_memory_for_an_unused_load(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            before, after = (root / f"{name}.mir" for name in ("before", "after"))
            source = """@module CoveredLoad
fn @choose(arg0: i256) -> i256 {
  bb0:
    v0 = call 100000, arg0, 0, 0, 0, 0, 32
    jumpi v0, bb1, bb2
  bb1:
    v1 = mload 0
    v2 = msize
    ret v2
  bb2:
    ret 0
}
"""
            reduced = source.replace("    v1 = mload 0\n", "")
            before.write_text(source)
            after.write_text(reduced)
            record = self.check_loop(root, 0)
            scalar = record["import_correspondence"][0]["correspondence"]["choose"][
                "scalar_reuse"
            ]
            self.assertEqual(scalar[0]["covered_dead_loads"], 1)

            without_output = [
                text.replace(", 0, 32\n", ", 0, 0\n") for text in (source, reduced)
            ]
            before.write_text(without_output[0])
            after.write_text(without_output[1])
            self.check_loop(root, 2)

            fixture = root / "world.json"
            fixture.write_text(
                '{"version":1,"accounts":[{"address":65537,"nonce":1,"code":"0x00"}]}'
            )
            initial = world.read(fixture, SOLAR, None, kind="mir")
            for left, right in ((source, reduced), without_output):
                programs = [
                    Program(
                        ("world-entry", "mir"),
                        ["arg0"],
                        "",
                        "",
                        replay_ir=text,
                        selector="choose",
                        evm_version="cancun",
                        world=initial,
                    )
                    for text in (left, right)
                ]
                result = native.replay(
                    *programs,
                    [65537],
                    executable=native.default(SOLAR),
                    timeout=20,
                )
                old, new = map(json.loads, result["observations"])
                self.assertEqual((old["outcome"], new["outcome"]), ("return", "return"))
                self.assertEqual(old["values"], ["32"])
                self.assertEqual(new["values"], ["32" if left == source else "0"])
                self.assertEqual(old == new, left == source)

    def test_memory_dse_reuses_a_store_across_a_returning_helper(self):
        source = """@module RedundantStore
fn @guard(arg0: i256) -> i256 {
  bb0:
    v0 = eq arg0, 0
    jumpi v0, bb1, bb2
  bb1:
    mstore 0, 37
    revert 0, 32
  bb2:
    ret arg0
}

fn @choose(arg0: i256) -> i256 {
  bb0:
    mstore 0, arg0
    mstore 32, 19
    v0 = icall @guard, arg0
    mstore 0, arg0
    v1 = mload 0
    v2 = add v0, v1
    ret v2
}
"""
        reduced = source.replace(
            "    mstore 0, arg0\n    v1 = mload 0", "    v1 = mload 0"
        )
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            before, after = (root / f"{name}.mir" for name in ("before", "after"))
            before.write_text(source)
            after.write_text(reduced)
            proof = self.check_loop(root, 0)
            reuse = proof["import_correspondence"][0]["correspondence"]["choose"][
                "state_read_reuse"
            ]
            self.assertEqual(reuse[0]["redundant_mstore"], 1)
            self.assertEqual(
                self.replay_loop(root, source, reduced, [7]), [["14"], ["14"]]
            )

            original, wrong = [
                text.replace("mstore 32, 19", "mstore 0, 19")
                for text in (source, reduced)
            ]
            before.write_text(original)
            after.write_text(wrong)
            counterexample = self.check_loop(root, 1)
            witness = counterexample["witness"]
            self.assertEqual(witness["observations"], witness["native_observations"])
            self.assertEqual(
                self.replay_loop(root, original, wrong, [7]), [["14"], ["26"]]
            )

    def test_memory_dse_discards_only_stores_after_the_last_observation(self):
        source = """@module TerminalStore
fn @choose() {
  bb0:
    v0 = gas
    mstore 0, 7
    log 0, 32
    mstore 0, 9
    stop
}
"""
        optimized = source.replace("    mstore 0, 9\n", "")
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            before, after = (root / f"{name}.mir" for name in ("before", "after"))
            before.write_text(source)
            after.write_text(optimized)
            proof = self.check_loop(root, 0)
            rewrites = proof["import_correspondence"][0]["correspondence"]["choose"][
                "state_read_reuse"
            ]
            self.assertEqual(rewrites, [{"terminal_mstore": 1}, {}])

            # Move the store ahead of the log so its bytes become observable.
            before.write_text(
                source.replace("    v0 = gas\n", "").replace(
                    "    log 0, 32\n    mstore 0, 9", "    mstore 0, 9\n    log 0, 32"
                )
            )
            after.write_text(optimized.replace("    v0 = gas\n", ""))
            fixture = root / "world.json"
            fixture.write_text('{"version":1}')
            counterexample = self.check_loop(root, 1, fixture=fixture)
            witness = counterexample["witness"]
            self.assertEqual(witness["observations"], witness["native_observations"])

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
