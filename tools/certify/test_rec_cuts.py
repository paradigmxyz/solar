"""Tests of the recursive cut-point writer (tools/certify/rec_cuts.py).

  python3 tools/certify/test_rec_cuts.py

They need neither Lean nor solar. testdata/rec_cuts/RecSum/ holds the stages that
`solar RecSum.sol -Ogas --evm-version osaka --emit=bin-runtime -Zemit-lean-stages=DIR` writes for
testdata/rec_cuts/RecSum.sol (a self-recursive sum). The relations are only inputs to kernel
checks (solidity-lean Verify/Phase3/RecSum), so these tests pin what the writer recognizes,
writes and refuses, never that the relations hold.
"""

import pathlib
import re
import shutil
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[1] / "solar-tv" / "milestone"))
sys.path.insert(0, str(HERE))
import cutpoints
import rec_cuts

DATA = HERE / "testdata" / "rec_cuts" / "RecSum"
PAIRS = [("Stage0", "Mir1"), ("Mir1", "Sched")] + [
    (a, b) for a, b in zip(["Sched", "Evm1", "Evm2", "Evm3", "Evm4", "Evm5"], ["Evm1", "Evm2", "Evm3", "Evm4", "Evm5", "Evm6"])
]


def stage(module):
    text = (DATA / f"{module}.lean").read_text()
    kind, passname = re.search(r"kind=(\w+) index=\d+ pass=([\w-]+)", text).groups()
    return {"def": re.search(r"^def (\w+)", text, re.M)[1], "file": f"{module}.lean", "kind": kind, "pass": passname}


class Shape(unittest.TestCase):
    def test_recursion_is_found(self):
        self.assertTrue(rec_cuts.is_recursive(DATA))
        funcs = cutpoints.parse_mir((DATA / "Stage0.lean").read_text())
        F, sites, outer = rec_cuts.shape("t", funcs)
        self.assertEqual((F, sites, outer), (0, [(0, 2, 0)], [(1, 2, 0)]))

    def test_cutpoints_no_longer_refuses(self):
        # cutpoints.stage_body hands a recursive module to rec_cuts instead of failing closed.
        body = cutpoints.stage_body(DATA, stage("Mir1"), stage("Sched"))
        self.assertIn("inductive sched_Fr", body)

    def test_every_pair_is_written(self):
        for a, b in PAIRS:
            body = rec_cuts.stage_body(DATA, stage(a), stage(b))
            p = stage(b)["def"]
            for name in (f"inductive {p}_Fr", f"def {p}_rels", f"theorem {p}_mkC0", f"theorem {p}_mkR0", f"def {p}_sim"):
                self.assertIn(name, body, (a, b, name))
            self.assertIn("rec_seg", body)
            self.assertNotIn("sorry", body)

    def test_frames_and_bound(self):
        body = rec_cuts.stage_body(DATA, stage("Mir1"), stage("Sched"))
        # The recursive frame keeps the caller's return label and argument below the callee's words,
        # and the stack below is bounded by the measure (argument 0, two words a frame).
        self.assertIn("sched_Fr (⟨0, 2, (some 7)", body)
        self.assertIn("(BitVec.ofNat 256 10) ((L) :: (c 0) :: T)", body)
        self.assertIn("(T).length + 2 * (env 0).toNat ≤ 984", body)
        # The outer frame of total's call returns to block 6.
        self.assertIn("(BitVec.ofNat 256 6) (junk)", body)


class Refusals(unittest.TestCase):
    def mutate(self, edit):
        d = pathlib.Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, d)
        for f in DATA.iterdir():
            shutil.copy(f, d / f.name)
        p = d / "Stage0.lean"
        p.write_text(edit(p.read_text()))
        return d

    def test_mutual_recursion(self):
        # total calls sum and sum calls total: two functions on one cycle.
        d = self.mutate(lambda t: t.replace(".call (some 7) 0 [.value 6]", ".call (some 7) 1 []").replace(
            ".call (some 8) 0 [.value 18]", ".call (some 8) 0 [.value 18], .call none 0 [.value 18]"))
        funcs = cutpoints.parse_mir((d / "Stage0.lean").read_text())
        with self.assertRaisesRegex(rec_cuts.Unsupported, "mutual recursion|is itself called|calls other functions"):
            rec_cuts.shape("t", funcs)

    def test_loop_beside_recursion(self):
        # bb1's leave becomes a jump back to bb0: a loop in the recursive function.
        d = self.mutate(lambda t: t.replace(".leave [.const 0]", ".jump 0", 1))
        funcs = cutpoints.parse_mir((d / "Stage0.lean").read_text())
        with self.assertRaisesRegex(rec_cuts.Unsupported, "a loop in @sum beside recursion"):
            rec_cuts.shape("t", funcs)

    def test_no_measure(self):
        # A schedule that keeps nothing of the argument below the recursive call: no measure.
        d = self.mutate(lambda t: t)
        s = d / "Schedule.json"
        s.write_text(s.read_text().replace('"below": ["v0", "ret"]', '"below": ["ret"]'))
        with self.assertRaisesRegex(rec_cuts.Unsupported, "no measure"):
            rec_cuts.stage_body(d, stage("Mir1"), stage("Sched"))


if __name__ == "__main__":
    unittest.main()
