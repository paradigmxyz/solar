"""Tests of the fixed-fuel gas stage (tools/certify/fixed_fuel.py).

  python3 tools/certify/test_fixed_fuel.py        # SOLAR names the solar binary for the contract cases

The control-flow cases need nothing. The contract cases run the stage on
examples/fixed_fuel: a loop with a literal limit is BOUNDED (exit 0), a loop to an argument word is
REJECTED for resources (out of gas, exit 1), and nested loops are BLOCKED (exit 3).
"""

import json
import os
import pathlib
import subprocess
import sys
import unittest
from typing import ClassVar

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import fixed_fuel

SOLAR = os.environ.get("SOLAR", str(HERE.parent.parent / "target" / "debug" / "solar"))


def op(*instr):
    return list(instr)


class ControlFlow(unittest.TestCase):
    # bb0 -> bb2 (header); bb2 -> bb1 (body) or falls to bb3 (stop); bb1 -> bb2.
    LOOP: ClassVar = [[op("jump", 2)], [op("jump", 2)], [op("jumpi", 1)], [op("stop")]]

    def test_successors(self):
        self.assertEqual(fixed_fuel.successors(self.LOOP), [[2], [2], [1, 3], [], []])

    def test_fuel(self):
        f = fixed_fuel.fuel(self.LOOP, 10)
        self.assertEqual((f["header"], f["B"], f["E"], f["X"]), (2, 2, 1, 2))
        self.assertEqual(f["F"], f["E"] + 10 * f["B"] + f["X"])

    def test_fuel_counts_a_revert_exit(self):
        # The body can revert (bb3): the exit path through it is longer and counts.
        blocks = [
            [op("jump", 1)],
            [op("jumpi", 2), op("stop")],
            [op("jumpi", 3), op("jump", 1)],
            [op("revert")],
        ]
        f = fixed_fuel.fuel(blocks, 4)
        self.assertEqual((f["header"], f["B"], f["X"]), (1, 2, 3))

    def test_no_loop(self):
        with self.assertRaisesRegex(fixed_fuel.Unsupported, "0 loops"):
            fixed_fuel.fuel([[op("stop")]], 1)

    def test_two_loops(self):
        blocks = [[op("jumpi", 0)], [op("jumpi", 1)], [op("stop")]]
        with self.assertRaisesRegex(fixed_fuel.Unsupported, "2 loops"):
            fixed_fuel.fuel(blocks, 1)

    def test_two_entries(self):
        # bb1 and bb2 form the loop, entered at both from bb0.
        blocks = [
            [op("jumpi", 2), op("jump", 1)],
            [op("jump", 2)],
            [op("jumpi", 1)],
            [op("stop")],
        ]
        with self.assertRaisesRegex(fixed_fuel.Unsupported, "2 entry blocks"):
            fixed_fuel.fuel(blocks, 1)

    def test_gas_split(self):
        # gasBoundL c (E + M * B + X) = M * perIter + exit (Verify.Gas.FixedFuel.gasBoundL_eq).
        E, B, X, M, bmax, mem = 1, 2, 3, 10, 22, 12
        G = fixed_fuel.ITEM * ((E + M * B + X) * bmax) + mem
        self.assertEqual(
            G,
            M * fixed_fuel.ITEM * (B * bmax) + fixed_fuel.ITEM * ((E + X) * bmax) + mem,
        )


@unittest.skipUnless(os.access(SOLAR, os.X_OK), "no solar binary (set SOLAR)")
class Contracts(unittest.TestCase):
    def stage(self, name):
        p = subprocess.run(
            [
                sys.executable,
                str(HERE / "fixed_fuel.py"),
                str(HERE / "examples" / "fixed_fuel" / name),
                "--solar",
                SOLAR,
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        out = p.stdout.strip().splitlines()
        return p.returncode, json.loads("\n".join(out[:-1])), out[-1]

    def test_bounded(self):
        code, ob, line = self.stage("bounded.sol")
        self.assertEqual(
            (code, ob["verdict"], ob["M"], ob["loop"]["limit"]),
            (0, "bounded", 10, "literal"),
        )
        self.assertLess(ob["G"], 2**64)
        self.assertEqual(ob["G"], ob["M"] * ob["per_iteration_gas"] + ob["exit_gas"])
        self.assertTrue(line.startswith(f"BOUNDED {ob['G']}"))

    def test_word_limit_is_rejected_for_resources(self):
        code, ob, line = self.stage("word_limit.sol")
        self.assertEqual(
            (code, ob["verdict"], ob["M"], ob["loop"]["limit"]),
            (1, "out-of-gas", 2**256 - 1, "word"),
        )
        self.assertGreaterEqual(ob["G"], 2**64)
        self.assertTrue(line.startswith("REJECTED: resource: out of gas"))

    def test_nested_is_unsupported(self):
        code, ob, line = self.stage("nested.sol")
        self.assertEqual((code, ob["verdict"]), (3, "unsupported"))
        self.assertTrue(line.startswith("BLOCKED: loop: a nested loop"))


class StackBound(unittest.TestCase):
    """The loop checker's inference refuses a stack with no bound by SystemExit (asm_cert.Unbounded,
    for its command line); the stage's own infer turns it into Unsupported, which the driver
    records, on the checked-in testdata/asm_cert_returns."""

    def test_unbounded_is_unsupported(self):
        d = HERE / "testdata" / "asm_cert_returns"
        code = bytes.fromhex(json.loads((d / "manifest.json").read_text())["bytecode"])
        with self.assertRaises(fixed_fuel.Unsupported) as e:
            fixed_fuel.infer((d / "Evm5.lean").read_text(), code)
        self.assertIn("the cycle bb3 -> bb4 -> bb3 gains stack words", str(e.exception))


if __name__ == "__main__":
    unittest.main()
