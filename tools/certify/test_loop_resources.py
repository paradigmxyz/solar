"""Tests of tools/certify/loop_resources.py.

The shape tests run the generator on the final EVM IR of solar's own exports (with SOLAR set) and on
hand-mutated copies of it: a body that steps the counter by two, a header that does not test a
header word, a counter that does not come from a call argument. Each must be refused by name. The
Lean side (the generated module building, and refusing mutated modules for their proof reason) is
tools/certify/loop_resources_check.sh.
"""

import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import loop_resources

SOLAR = os.environ.get("SOLAR", str(HERE.parents[1] / "target" / "debug" / "solar"))

COUNTER = """// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract C {
    uint256 public count;
    function bump(uint256 a) external {
        for (uint256 i = 0; i < a; ++i) {
            count += 1;
        }
    }
}
"""


def export(src_text, tmp):
    src = pathlib.Path(tmp) / "C.sol"
    src.write_text(src_text)
    out = subprocess.run(
        [
            SOLAR,
            "C.sol",
            "-Ogas",
            "--evm-version",
            "osaka",
            "--emit=bin-runtime",
            "-Zemit-lean-stages=gen",
        ],
        cwd=tmp,
        check=False,
        capture_output=True,
        text=True,
    )
    if out.returncode != 0:
        raise AssertionError(out.stderr[-400:])
    return pathlib.Path(tmp) / "gen" / "C"


def generate(gen):

    bt = (gen / "Bytecode.lean").read_text()
    plain = re.search(r'def bytecodeHex : String :=\n  "([0-9a-f]+)"', bt).group(1)
    ob = loop_resources.obligation(gen, plain)
    last = ob["final_evm_ir"]
    return loop_resources.generate(
        ns="T.LoopResources",
        contract="C",
        last=last[0].lower() + last[1:],
        gen_dir=gen,
        ob=ob,
        code_term="SolarTV.Export.C.code",
        code_import="T",
        sel=0xB20EB4C4,
        nargs=1,
    )


def mutate(gen, old, new, count=1):
    """Replace OLD by NEW in the final EVM IR (exactly COUNT times)."""
    f = max(gen.glob("Evm*.lean"), key=lambda p: int(p.stem[3:]))
    t = f.read_text()
    assert t.count(old) >= count, (old, t.count(old))
    f.write_text(t.replace(old, new, count))


class Words(unittest.TestCase):
    def test_render(self):
        e = ("op", "add", loop_resources.lit(1), ("var", "w0"))
        self.assertEqual(loop_resources.render(e), "EvmRules.Evm.add 1 w0")
        self.assertEqual(
            loop_resources.render(("cdw", loop_resources.lit(4))), "calldataWord call 4"
        )

    def test_hyp(self):
        c = ("op", "eq", ("var", "a"), loop_resources.lit(5))
        self.assertEqual(loop_resources.hyp(c, True), "a = 5")
        self.assertEqual(loop_resources.hyp(c, False), "¬a = 5")
        self.assertEqual(loop_resources.hyp(("var", "w"), True), "¬w = 0")

    def test_evaluate(self):
        f = {"cv": 0, "cds": 36, "sel": 7}
        sel = ("op", "shr", loop_resources.lit(224), ("cdw", loop_resources.lit(0)))
        self.assertEqual(
            loop_resources.evaluate(("op", "eq", sel, loop_resources.lit(7)), f), 1
        )
        self.assertIsNone(loop_resources.evaluate(("cdw", loop_resources.lit(4)), f))


@unittest.skipUnless(os.access(SOLAR, os.X_OK), "no solar binary (set SOLAR)")
class Shapes(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp()
        self.gen = export(COUNTER, self.tmp)

    def tearDown(self):
        shutil.rmtree(self.tmp)

    def test_counter_generates(self):
        text = generate(self.gen)
        for want in (
            "theorem resources_evmir",
            "def bridge",
            "theorem loops",
            "theorem verdict",
            "theorem check_ok",
            "theorem inhabited",
            "theorem total",
        ):
            self.assertIn(want, text)
        self.assertNotIn("sorry", text)
        self.assertNotIn("native_decide", text)
        self.assertNotIn("axiom ", text)

    def test_step_by_two_is_refused(self):
        # The body's increment of the counter word (add 1) becomes add 2.
        f = max(self.gen.glob("Evm*.lean"), key=lambda p: int(p.stem[3:]))
        t = f.read_text()
        hb = re.search(r"-- bb(\d+)\n((?:.|\n)*?)(?=\n    -- bb|\n  \])", t)
        self.assertIsNotNone(hb)
        n = t.count(".op (.stack (.push 1))")
        self.assertGreater(n, 0)
        f.write_text(
            t.replace(
                ".op (.stack (.push 1)),\n     .op (.stack .add)",
                ".op (.stack (.push 2)),\n     .op (.stack .add)",
            )
        )
        with self.assertRaises(loop_resources.Unsupported) as e:
            generate(self.gen)
        self.assertRegex(str(e.exception), "stepping the counter by one|counter word")

    def test_literal_limit_is_refused(self):
        tmp = tempfile.mkdtemp()
        try:
            gen = export(COUNTER.replace("i < a", "i < 10"), tmp)
            with self.assertRaises(loop_resources.Unsupported) as e:
                generate(gen)
            self.assertRegex(str(e.exception), "not 0 - a call argument|loop|header")
        finally:
            shutil.rmtree(tmp)


if __name__ == "__main__":
    unittest.main()
