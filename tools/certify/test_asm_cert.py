"""Tests of the assembly certificate's heads for a recursive function (tools/certify/asm_cert.py,
certify.rec_heads).

  python3 tools/certify/test_asm_cert.py

They need neither Lean nor solar: testdata/rec_cuts/RecSum/ holds the stages solar exports for
testdata/rec_cuts/RecSum.sol. The checker with heads (solidity-lean
SolidCore/Refinement/Solar/AssembleRec.lean) takes a recursive function's cut points' highest entry
heights from the run; these tests pin that the inference fixes them there, and that it still
refuses the program, naming the jump whose entry types no static type covers. The checker with frames
(AssembleRec.checkF) types those words by the frame relation instead: these tests pin the frames
infer_frames gives RecSum and its refusals.
"""

import pathlib
import sys
import unittest
from unittest import mock

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[1] / "solar-tv" / "milestone"))
sys.path.insert(0, str(HERE))
import asm_cert
import certify

DATA = HERE / "testdata" / "rec_cuts" / "RecSum"
# The runtime bytecode solar writes for RecSum.sol (-Ogas --evm-version osaka).
CODE = bytes.fromhex(
    "34603a575f3560e01c8063b119ebfe03603a5760806040526024361060406004351117603a57600435603e5b81"
    "15604f57600182036045602b565b5f5ffd5b5f5260205ff35b8201918210605557565b90505f90565b60116020"
    "52634e487b715f526024601cfd"
)
LAST = {"pass": "block-layout", "module": "Evm6", "def": "evm6"}


class Heads(unittest.TestCase):
    def test_heads_are_the_cut_points(self):
        # The entry (block 1: return label, n) and the resume point (block 4: the result over the
        # caller's frame), each at most its frame's words over the measure's bound (984).
        self.assertEqual(
            certify.rec_heads(DATA, LAST),
            {1: (986, 2, [True, False]), 4: (987, 3, [False, True, False])},
        )

    def test_without_heads_the_heights_do_not_settle(self):
        with self.assertRaises(SystemExit) as e:
            asm_cert.infer((DATA / "Evm6.lean").read_text(), CODE)
        self.assertIn("do not settle", str(e.exception))

    def test_with_heads_the_resume_types_refuse(self):
        with self.assertRaises(SystemExit) as e:
            asm_cert.infer(
                (DATA / "Evm6.lean").read_text(), CODE, heads=certify.rec_heads(DATA, LAST)
            )
        self.assertIn(
            "block 4 enters the recursive function's cut point at block 4 with 1 words typed "
            "[some false], where it needs 3 typed [some false, some true, some false]",
            str(e.exception),
        )

    def test_heads_leave_other_programs_alone(self):
        # No heads: the inference of a program without recursion is unchanged.
        text = "def p : EVM.Program.Program :=\n  { blocks := [\n    -- bb0\n    [.stop]] }\n"
        c = asm_cert.infer(text, bytes([0x00]))
        self.assertEqual(c["heads"], [])
        self.assertEqual(c["maxs"], [0])


class Frames(unittest.TestCase):
    """The checker with frames (AssembleRec.checkF): the caller's words below a recursive call are
    typed by the frame relation of the call site whose label the function returns to."""

    def infer(self, text=None, code=CODE):
        heads, entry = certify.rec_frame_heads(DATA, LAST)
        return asm_cert.infer_frames(text or (DATA / "Evm6.lean").read_text(), code, heads, entry)

    def refuse(self, text):
        """infer_frames on a mutated program, whose layout no bytecode matches: the flow analysis
        alone, at RecSum's layout."""
        real = asm_cert.layout(asm_cert.parse_program((DATA / "Evm6.lean").read_text()), CODE)
        with mock.patch.object(asm_cert, "layout", lambda blocks, code: real):
            with self.assertRaises(SystemExit) as e:
                self.infer(text)
        return str(e.exception)

    def test_recsum_frames(self):
        c = self.infer()
        # The function's blocks: its entry, the point it resumes at and its base case; the panic
        # block it jumps to on overflow only reverts, so it needs no frame.
        self.assertEqual(c["frames"]["blocks"], [1, 4, 5])
        # The outer call (label 3) keeps the selector word below the callee; the recursive call
        # (label 4) keeps the caller's return label and argument, the label first.
        self.assertEqual(
            c["frames"]["calls"], [(3, [False], None), (4, [True, False], 0)]
        )
        # Where the outer call resumes, the result over the selector word it kept.
        self.assertEqual(c["needs"][3], 2)
        self.assertEqual(c["types"][3], [False, False])
        self.assertEqual(c["maxs"][1], 986)
        self.assertEqual(c["maxs"][4], 987)
        self.assertEqual(
            asm_cert.lean_frames(c),
            "{ blocks := [1, 4, 5], calls := [(3, [some false], none), "
            "(4, [some true, some false], some 0)] }",
        )

    def test_jump_into_the_function_refuses(self):
        # The dispatcher jumps into the function's base case instead of calling it.
        text = (DATA / "Evm6.lean").read_text().replace(".pushLabel 3,\n     .jump 1]", ".jump 5]")
        self.assertIn("enters the recursive function's block 5 other than by a call", self.refuse(text))

    def test_frame_block_memory_refuses(self):
        # A store in the function's base case: a block with a frame runs stack operations only.
        text = (DATA / "Evm6.lean").read_text().replace(
            "[.op (.stack (.swap 0)),\n     .op (.stack .pop),",
            "[.op (.stack (.swap 0)),\n     .op (.stack (.push 0)),\n     .op .store,\n     .op (.stack .pop),",
        )
        self.assertIn("block 5 of the recursive function runs ('store',)", self.refuse(text))


if __name__ == "__main__":
    unittest.main()
