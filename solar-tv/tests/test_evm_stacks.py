"""Symbolic block entries, live stack observations, and EVM terminal outcomes."""

import re
import tempfile
import unittest
from pathlib import Path

from test_cli import ROOT, check


class EvmStackTests(unittest.TestCase):
    def pair(self, before, after, *, height=0, edge=False, status=0):
        with tempfile.TemporaryDirectory() as tmp:
            paths = []
            for name, body in [("before", before), ("after", after)]:
                path = Path(tmp) / f"{name}.evmir"
                if edge:
                    body += "\npush bb1\njump\nbb1:\nstop"
                path.write_text(f"@module Stack\nbb0 [stack={height}]:\n{body}\n")
                paths.append(path)
            result = check("evmir", *paths)
        self.assertEqual(result.returncode, status, result.stderr + result.stdout)
        return result

    def test_broken_live_stack_has_a_replayable_witness(self):
        folder = ROOT / "examples/evmir/symbolic-stack"
        result = check("evmir", folder / "before.evmir", folder / "broken.evmir")
        self.assertEqual(result.returncode, 1, result.stderr)
        witness = {
            name: int(value)
            for name, value in re.findall(r"(stack\d+) = (\d+)", result.stdout)
        }
        a, b = witness["stack0"], witness["stack1"]
        before, after = (a + b) % 2**256, (a - b) % 2**256
        self.assertNotEqual(before, after)
        self.assertIn(
            f"before = bb1, stack=[{before}]\n  after  = bb1, stack=[{after}]",
            result.stdout,
        )

    def test_invalid_to_success_is_a_counterexample(self):
        folder = ROOT / "examples/evmir/failure"
        result = check("evmir", folder / "before.evmir", folder / "broken.evmir")
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn(
            "before = exception(invalid-instruction)\n  after  = success(empty)",
            result.stdout,
        )

    def test_stack_order_and_word_operations(self):
        mask = 2**256 - 1
        for operation, expected in [
            ("sub", 5),
            ("add", 13),
            ("mul", 36),
            ("and", 0),
            ("or", 13),
            ("xor", 13),
            ("eq", 0),
        ]:
            with self.subTest(operation=operation):
                self.pair(f"push 4\npush 9\n{operation}", f"push {expected}", edge=True)
        self.pair("push 4\npush 9\nswap 1\nsub", f"push {mask - 4}", edge=True)
        self.pair("push 4\ndup 1\nsub", "push 0", edge=True)
        self.pair("push 4\npush 9\ndup 2\nadd", "push 4\npush 13", edge=True)
        self.pair("push 1\npush 2\npush 3\nswap 2", "push 3\npush 2\npush 1", edge=True)
        self.pair("push 4\nnot", f"push {mask - 4}", edge=True)
        self.pair("push 4\niszero", "push 0", edge=True)

    def test_halts_discard_remaining_stack(self):
        self.pair("pop\nstop", "stop", height=1)
        self.pair("push 0\npush 0\nreturn", "stop")
        self.pair("push 0\ndup 1\nreturn", "stop")
        self.pair("push 0\ncalldatasize\nreturn", "stop")
        self.pair("push 0\npush 31\nrevert", "push 0\npush 0\nrevert")
        self.pair("push 0\npush 0\nrevert", "stop", status=1)

    def test_full_declared_stack_can_halt(self):
        self.pair("pop\nstop", "stop", height=1024)

    def test_almost_full_stack_can_push_and_pop(self):
        self.pair("push 0\npop\nstop", "stop", height=1023)

    def test_compiler_rejects_invalid_stack_snapshots(self):
        for body, height, diagnostic in [
            ("pop\nstop", 0, "empty stack"),
            ("push 0\nstop", 1024, "exceeding the limit"),
        ]:
            with self.subTest(height=height):
                result = self.pair(body, "stop", height=height, status=2)
                self.assertIn(diagnostic, result.stderr)

    def test_nonempty_or_symbolic_return_data_is_inconclusive(self):
        result = self.pair("push 1\npush 0\nreturn", "stop", status=2)
        self.assertIn("only statically empty", result.stderr)
        result = self.pair("return", "stop", height=2, status=2)
        self.assertIn("only statically empty", result.stderr)

    def test_different_entry_heights_are_inconclusive(self):
        with tempfile.TemporaryDirectory() as tmp:
            paths = []
            for height in (1, 2):
                path = Path(tmp) / f"{height}.evmir"
                path.write_text(f"@module Stack\nbb0 [stack={height}]:\nstop\n")
                paths.append(path)
            result = check("evmir", *paths)
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn("input interfaces differ", result.stderr)
