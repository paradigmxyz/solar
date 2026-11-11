"""Defined failures, first-failure ordering, and checked arithmetic boundaries."""

import tempfile
import unittest
from pathlib import Path

from test_cli import check


class OutcomeTests(unittest.TestCase):
    def pair(self, before, after, *, width=256, params="", status=0):
        with tempfile.TemporaryDirectory() as tmp:
            paths = []
            for name, body in [("before", before), ("after", after)]:
                path = Path(tmp) / f"{name}.mir"
                path.write_text(
                    f"@module Outcomes\nfn @f({params}) -> i{width} {{\nbb0:\n{body}\n}}\n"
                )
                paths.append(path)
            result = check("mir", *paths)
        self.assertEqual(result.returncode, status, result.stderr + result.stdout)
        return result

    def test_checked_boundaries_against_integer_oracle(self):
        for width in (160, 256):
            mask, sign = 2**width - 1, 2 ** (width - 1)
            cases = [
                (0, 1),
                (mask, 1),
                (sign, mask),
                (sign - 1, 1),
                (sign, 1),
                (sign - 1, mask),
                (3, 2),
            ]
            for signed in (False, True):
                for op in ("add", "sub", "mul"):
                    for a, b in cases:
                        with self.subTest(width=width, signed=signed, op=op, a=a, b=b):
                            left = a - 2**width if signed and a >= sign else a
                            right = b - 2**width if signed and b >= sign else b
                            value = {
                                "add": left + right,
                                "sub": left - right,
                                "mul": left * right,
                            }[op]
                            lower, upper = (-sign, sign) if signed else (0, 2**width)
                            if lower <= value < upper:
                                expected = f"ret i{width} {value % 2**width}"
                            else:
                                expected = "icall panic_if<0x11>, true\ninvalid"
                            kind = "int" if signed else "uint"
                            self.pair(
                                f"v0 = checked_{op} {kind}{width}, i{width} {a}, i{width} {b}\nret v0",
                                expected,
                                width=width,
                            )

    def test_dropping_an_unused_check_fails_refinement(self):
        result = self.pair("v0 = checked_sub uint256, 0, 1\nret 7", "ret 7", status=1)
        self.assertEqual(
            result.stdout.split("\n\n", 1)[0] + "\n",
            "counterexample\n  MIR function @f\n  before = revert Panic(17)\n  after  = 7\n",
        )

    def test_first_failure_wins(self):
        self.pair(
            "icall panic_if<0x1>, true\nv0 = checked_sub uint256, 0, 1\nret v0",
            "icall panic_if<0x1>, true\ninvalid",
        )
        self.pair(
            "v0 = checked_sub uint256, 0, 1\nicall panic_if<0x1>, true\nret v0",
            "icall panic_if<0x11>, true\ninvalid",
        )

    def test_panic_codes_are_observable(self):
        result = self.pair(
            "icall panic_if<0x1>, true\ninvalid",
            "icall panic_if<0x11>, true\ninvalid",
            status=1,
        )
        self.assertIn(
            "before = revert Panic(1)\n  after  = revert Panic(17)", result.stdout
        )

    def test_invalid_empty_revert_and_success_are_distinct(self):
        for before, after in [
            ("invalid", "revert 0, 0"),
            ("revert 0, 0", "ret 0"),
            ("ret 0", "invalid"),
        ]:
            with self.subTest(before=before, after=after):
                result = self.pair(before, after, status=1)
                self.assertEqual(result.stdout.splitlines()[0], "counterexample")

    def test_conditional_checks_preserve_the_passing_path(self):
        self.pair(
            "icall panic_if_zero<0x1>, arg0\nret 7",
            "jumpi arg0, bb1, bb2\nbb1:\nret 7\nbb2:\nicall panic_if<0x1>, true\ninvalid",
            params="arg0: i1",
        )
        self.pair(
            "icall require<empty_string>, arg0\nret 7",
            "jumpi arg0, bb1, bb2\nbb1:\nret 7\nbb2:\nrevert 0, 0",
            params="arg0: i1",
        )
        self.pair(
            "icall revert_if<empty>, arg0\nret 7",
            "jumpi arg0, bb1, bb2\nbb1:\nrevert 0, 0\nbb2:\nret 7",
            params="arg0: i1",
        )

    def test_unknown_revert_bytes_are_inconclusive(self):
        result = self.pair("revert 0, 32", "revert 0, 32", status=2)
        self.assertIn("unmodeled MIR terminator", result.stderr)
        result = self.pair("v0 = checked_div uint256, 7, 0\nret v0", "ret 0", status=1)
        self.assertIn("before = revert Panic(18)", result.stdout)
