"""Internal calls and allocation checked directly, without backend layout or protocols."""

import subprocess
import unittest

import test_mir_state as state
from test_cli import ROOT, SOLAR


def program(body, helpers="", signature="fn @f() -> i256"):
    return state.module(body, signature) + helpers


class MirCallsTests(unittest.TestCase):
    pair = state.MirStateTests.pair

    def test_real_pass_pairs_and_broken_calls_and_aliases(self):
        for example, transform, expected in (
            ("internal-calls", "inline", "Returned values changed"),
            ("allocation", "lower-alloc", "Returned values changed"),
        ):
            with self.subTest(example=example):
                folder = ROOT / "examples/mir" / example
                before = (folder / "before.mir").read_text()
                after = subprocess.check_output(
                    [
                        SOLAR,
                        folder / "before.mir",
                        "-Ogas",
                        f"-Zmir-pipeline={transform}",
                        "-Zvalidate-ir=true",
                    ],
                    text=True,
                )
                self.assertEqual(
                    after.splitlines()[1:],
                    (folder / "after.mir").read_text().splitlines()[1:],
                )
                self.pair(before, after)
                result, record = self.pair(
                    before, (folder / "broken.mir").read_text(), status=1
                )
                self.assertIn(expected, result.stdout)
                self.assertEqual(
                    record["witness"]["observations"],
                    record["witness"]["native_observations"],
                )

    def test_void_call_keeps_caller_locals_and_shares_memory(self):
        helper = (
            "fn @write(arg0: i256) {\nbb0:\nv0 = add arg0, 1\nmstore 0, v0\nret\n}\n"
        )
        before = program("v0 = add 3, 4\nicall @write, 8\nret v0", helper)
        self.pair(before, program("mstore 0, 9\nret 7"))

    def test_callee_state_is_seen_by_its_caller(self):
        helper = (
            "fn @write(arg0: i256) {\nbb0:\nsstore 0, arg0\ntstore 1, arg0\nret\n}\n"
        )
        before = program(
            "icall @write, 7\nv0 = sload 0\nv1 = tload 1\nv2 = add v0, v1\nret v2",
            helper,
        )
        self.pair(before, program("sstore 0, 7\ntstore 1, 7\nret 14"))

    def test_call_evaluates_arguments_in_caller_scope(self):
        helper = "fn @swap(arg0: i256, arg1: i256) -> i256 {\nbb0:\nv0 = sub arg0, arg1\nret v0\n}\n"
        self.pair(
            program("v0 = add 7, 1\nv1 = icall @swap, v0, 3\nret v1", helper),
            program("ret 5"),
        )

    def test_narrow_and_pointer_call_results(self):
        helper = "fn @bump(arg0: i8) -> i8 {\nbb0:\nv0 = add arg0, i8 1\nret v0\n}\n"
        self.pair(
            program(
                "v0 = icall @bump, i8 255\nv1 = zext i8 v0 to i256\nret v1", helper
            ),
            program("ret 0"),
        )
        helper = "fn @reserve() -> ptr<memory> {\nbb0:\nv0 = alloc checked 32 !metadata(preserves_fmp)\nret v0\n}\n"
        self.pair(
            program(
                "mstore 64, 128\nv0 = icall @reserve\nmstore v0, 7\nv1 = mload v0\nret v1",
                helper,
            ),
            program("mstore 64, 160\nmstore 128, 7\nret 7"),
        )

    def test_recursion_and_tail_calls(self):
        helper = "fn @count(arg0: i256) -> i256 {\nbb0:\nv0: i1 = eq arg0, 0\njumpi v0, bb1, bb2\nbb1:\nret 11\nbb2:\nv1 = sub arg0, 1\nv2 = icall @count, v1\nv3 = add v2, 1\nret v3\n}\n"
        for entry in ("v0 = icall @count, 3\nret v0", "tail_call @count, 3"):
            with self.subTest(entry=entry):
                self.pair(program(entry, helper), program("ret 14"))
        tail = "fn @count(arg0: i256) -> i256 {\nbb0:\nv0: i1 = eq arg0, 0\njumpi v0, bb1, bb2\nbb1:\nret 11\nbb2:\nv1 = sub arg0, 1\ntail_call @count, v1\n}\n"
        self.pair(
            program("tail_call @count, 140", tail),
            program("ret 11"),
            fixture={"fuel": 2048},
        )

    def test_callee_terminal_outcomes_do_not_resume_caller(self):
        for ending in ("stop", "returndata 0, 1", "revert 0, 1", "invalid"):
            with self.subTest(ending=ending):
                helper = (
                    f"fn @end() {{\nbb0:\nmstore8 0, 42\nsstore 0, 7\n{ending}\n}}\n"
                )
                before = program("icall @end\nsstore 0, 9\nret 0", helper)
                after = program(f"mstore8 0, 42\nsstore 0, 7\n{ending}")
                self.pair(before, after)

    def test_call_work_and_depth_limits_are_not_equivalence(self):
        helper = "fn @spin() -> i256 {\nbb0:\nv0 = icall @spin\nret v0\n}\n"
        before = program("v0 = icall @spin\nret v0", helper)
        result, _ = self.pair(before, before, fixture={"fuel": 2048}, status=2)
        self.assertIn("call depth limit", result.stderr)
        tail = program("tail_call @f")
        result, _ = self.pair(tail, tail, fixture={"fuel": 32}, status=2)
        self.assertIn("work limit", result.stderr)
        helper = "fn @work() {\nbb0:\n" + "mstore8 0, 1\n" * 12 + "ret\n}\n"
        before = program("icall @work\nicall @work\nret 0", helper)
        result, _ = self.pair(before, before, fixture={"fuel": 31}, status=2)
        self.assertIn("work limit", result.stderr)

    def test_allocations_do_not_alias_across_calls(self):
        helper = "fn @reserve() -> ptr<memory> {\nbb0:\nv0 = alloc checked 32 !metadata(preserves_fmp)\nret v0\n}\n"
        body = "mstore 64, 128\nv0 = icall @reserve\nv1 = icall @reserve\nmstore v0, 3\nmstore v1, 7\nv2 = mload v0\nret v2"
        bad = body.replace("mstore v1, 7", "mstore v0, 7")
        _, record = self.pair(program(body, helper), program(bad, helper), status=1)
        self.assertEqual(
            [v["values"] for v in record["witness"]["observations"]], [["3"], ["7"]]
        )

    def test_initialization_is_byte_exact_and_follows_cursor_update(self):
        initial = bytes(64) + (128).to_bytes(32, "big") + bytes(32) + bytes([0xAB]) * 32
        fixture = {"memory": {"data": "0x" + initial.hex()}}
        for zeroed, write in (("zeroed ", "memory_zero 128, 3\n"), ("", "")):
            with self.subTest(zeroed=zeroed):
                before = program(
                    f"v0 = alloc {zeroed}checked 3 !metadata(preserves_fmp)\nret 0"
                )
                after = program("mstore 64, 131\n" + write + "ret 0")
                self.pair(before, after, fixture=fixture)
        before = program(
            "mstore 64, 64\nv0 = alloc zeroed checked 32 !metadata(preserves_fmp)\nv1 = mload 64\nret v1"
        )
        self.pair(before, program("mstore 64, 0\nret 0"))

    def test_checked_overflow_and_infallible_contract(self):
        before = program(
            f"mstore 64, {2**256 - 1}\nv0 = alloc checked 1 !metadata(preserves_fmp)\nret 0"
        )
        after = program(f"mstore 0, {0x4E487B71 << 224}\nmstore 4, 65\nrevert 0, 36")
        self.pair(before, after)
        bad = before.replace("alloc checked", "alloc")
        result, _ = self.pair(bad, bad, status=2)
        self.assertIn("infallible allocation validity precondition", result.stderr)
        self.pair(
            program("mstore 64, 128\nv0 = alloc 3 !metadata(preserves_fmp)\nret 0"),
            program("mstore 64, 131\nret 0"),
        )

    def test_reserving_bytes_does_not_expand_or_zero_them(self):
        self.pair(
            program(
                "mstore 64, 128\nv0 = alloc checked 4096 !metadata(preserves_fmp)\nv1 = msize\nret v1"
            ),
            program("mstore 64, 4224\nret 96"),
        )
        self.pair(
            program(
                "mstore 64, 128\nv0 = alloc zeroed checked 0 !metadata(preserves_fmp)\nv1 = msize\nret v1"
            ),
            program("mstore 64, 128\nret 96"),
        )

    def test_unused_unsupported_function_does_not_block_closure(self):
        helper = "fn @unused() -> i256 {\nbb0:\nv0 = gas\nret v0\n}\n"
        self.pair(program("ret 7", helper), program("ret 7"))

    def test_deferred_and_heap_floor_allocations_stay_unsupported(self):
        for body in (
            "v0 = alloc 32 !metadata(deferred_alloc)\nret 0",
            "v0 = heap_floor\nret v0",
        ):
            with self.subTest(body=body):
                before = program(body)
                self.pair(before, before, status=2)
