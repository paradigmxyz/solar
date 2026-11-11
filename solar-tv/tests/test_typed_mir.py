"""Typed inputs and boundary values checked through the real compiler and Lean."""

import re
import tempfile
import unittest
from pathlib import Path

from test_cli import ROOT, check


class TypedMirTests(unittest.TestCase):
    def pair(self, before, after, *, params="", width=256, status=0):
        with tempfile.TemporaryDirectory() as tmp:
            paths = []
            for name, body in [("before", before), ("after", after)]:
                path = Path(tmp) / f"{name}.mir"
                path.write_text(
                    f"@module Typed\nfn @f({params}) -> i{width} {{\nbb0:\n{body}\n}}\n"
                )
                paths.append(path)
            result = check("mir", *paths)
        self.assertEqual(result.returncode, status, result.stderr + result.stdout)
        return result

    def test_comparisons_at_width_boundaries(self):
        for width in (1, 160, 256):
            with self.subTest(width=width):
                mask = 2**width - 1
                sign = 2 ** (width - 1)
                pairs = [(0, 0), (0, mask), (sign, sign - 1), (mask, sign)]
                code = []
                expected = 0
                bit = 0
                accumulator = "0"
                for op in ("eq", "ne", "lt", "gt", "slt", "sgt"):
                    for a, b in pairs:
                        signed_a = a - 2**width if a >= sign else a
                        signed_b = b - 2**width if b >= sign else b
                        outcomes = {
                            "eq": a == b,
                            "ne": a != b,
                            "lt": a < b,
                            "gt": a > b,
                            "slt": signed_a < signed_b,
                            "sgt": signed_a > signed_b,
                        }
                        expected |= int(outcomes[op]) << bit
                        base = 4 * bit
                        code.extend(
                            [
                                f"v{base} = {op} i{width} {a}, i{width} {b}",
                                f"v{base + 1} = zext i1 v{base} to i256",
                                f"v{base + 2} = mul v{base + 1}, {2**bit}",
                                f"v{base + 3} = add v{base + 2}, {accumulator}",
                            ]
                        )
                        accumulator = f"v{base + 3}"
                        bit += 1
                self.pair("\n".join([*code, f"ret {accumulator}"]), f"ret {expected}")

    def test_casts_at_sign_and_truncation_boundaries(self):
        cases = [
            ("trunc", 256, 1),
            ("trunc", 256, 160),
            ("trunc", 160, 1),
            ("zext", 1, 160),
            ("zext", 1, 256),
            ("zext", 160, 256),
            ("sext", 1, 160),
            ("sext", 1, 256),
            ("sext", 160, 256),
        ]
        for kind, source, target in cases:
            for value in {0, 2 ** (source - 1), 2**source - 1}:
                with self.subTest(kind=kind, source=source, target=target, value=value):
                    signed = value - 2**source if value >= 2 ** (source - 1) else value
                    expected = (signed if kind == "sext" else value) % 2**target
                    self.pair(
                        f"v0 = {kind} i{source} {value} to i{target}\nret v0",
                        f"ret i{target} {expected}",
                        width=target,
                    )

    def test_narrow_arithmetic_wraps(self):
        for width in (1, 160, 256):
            with self.subTest(width=width):
                mask = 2**width - 1
                self.pair(
                    f"v0 = add i{width} {mask}, i{width} 1\nret v0",
                    f"ret i{width} 0",
                    width=width,
                )

    def test_i1_counterexample_is_a_boolean(self):
        result = self.pair(
            "ret arg0", "ret false", params="arg0: i1", width=1, status=1
        )
        self.assertEqual(
            result.stdout.split("\n\n", 1)[0] + "\n",
            "counterexample\n  MIR function @f\n  arg0 = 1\n  before = 1\n  after  = 0\n",
        )

    def test_truncation_is_not_a_nonzero_test(self):
        result = self.pair(
            "v0 = trunc i256 arg0 to i1\nret v0",
            "v0 = ne arg0, 0\nret v0",
            params="arg0: i256",
            width=1,
            status=1,
        )
        match = re.search(r"arg0 = (\d+)", result.stdout)
        assert match is not None, result.stdout
        value = int(match[1])
        self.assertGreater(value, 0)
        self.assertEqual(value % 2, 0)
        self.assertIn("before = 0\n  after  = 1", result.stdout)

    def test_inverted_typed_branch_has_a_replayable_witness(self):
        folder = ROOT / "examples/mir/typed-branch"
        result = check("mir", folder / "before.mir", folder / "broken.mir")
        self.assertEqual(result.returncode, 1, result.stderr)
        witness = {
            key: int(value)
            for key, value in re.findall(r"(\w+) = (\d+)", result.stdout)
        }
        self.assertIn(witness["arg0"], (0, 1))
        self.assertLess(witness["arg1"], 2**160)
        before = witness["arg1"] if witness["arg0"] else (witness["arg1"] + 1) % 2**160
        after = (witness["arg1"] + 1) % 2**160 if witness["arg0"] else witness["arg1"]
        self.assertIn(f"before = {before}\n  after  = {after}", result.stdout)

    def test_input_width_changes_are_inconclusive(self):
        with tempfile.TemporaryDirectory() as tmp:
            paths = []
            for width in (160, 256):
                path = Path(tmp) / f"{width}.mir"
                path.write_text(
                    f"@module Width\nfn @f(arg0: i{width}) -> i256 {{\nbb0:\nret 0\n}}\n"
                )
                paths.append(path)
            result = check("mir", *paths)
            self.assertEqual(result.returncode, 2, result.stderr)
            self.assertIn("input interfaces differ", result.stderr)

    def test_unmodeled_width_and_undefined_values_are_inconclusive(self):
        result = self.pair("ret arg0", "ret arg0", params="arg0: i8", width=8, status=2)
        self.assertIn("unmodeled MIR integer width i8", result.stderr)
        result = self.pair("ret undef i256", "ret undef i256", status=2)
        self.assertIn("undefined MIR operands", result.stderr)
