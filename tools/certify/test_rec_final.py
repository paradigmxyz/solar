"""Tests of rec_final.py and the envelope's recursion over free functions."""

import os
import pathlib
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import generic
import rec_final

SOLC = pathlib.Path.home() / ".cache/solc/solc-0.8.35"
# A solidity-lean checkout with the pinned exports, when SOLIDITY_LEAN_DIR names one.
P2 = pathlib.Path(os.environ.get("SOLIDITY_LEAN_DIR") or HERE / "missing-solidity-lean")
RECSUM = HERE / "testdata/rec_cuts/RecSum.sol"

FREE = """// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
function exp(uint b, uint e) pure returns (uint p) {
    if (e == 0) return 1;
    p = exp(b, e / 2);
}
contract C {
  function g(uint b, uint e) public pure returns (uint) { return exp(b, e); }
}
"""

OTHER = """// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract D {
    /// @custom:decreases n
    function f(uint256 n) internal pure returns (uint256) {
        if (n == 0) return 0;
        return f(n - 1);
    }
    function g(uint256 n) external pure returns (uint256) { return f(n); }
}
"""

UNREACHED = """// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
function loopy(uint n) pure returns (uint) { return n == 0 ? 0 : loopy(n - 1); }
contract C {
  function g(uint b) public pure returns (uint) { return b; }
}
"""


def ast_of(text):
    d = tempfile.mkdtemp()
    p = pathlib.Path(d) / "S.sol"
    p.write_text(text)
    return generic.solc_ast(str(SOLC), p)[0]


def cdef(ast, name):
    return next(
        n
        for n in ast["nodes"]
        if n.get("nodeType") == "ContractDefinition" and n.get("name") == name
    )


def graph_cycles(ast, name):
    """recursion_cycles on the graph the envelope reads: the contract's linearization and the
    sibling library and free functions it runs inline (sibling_code)."""
    c = cdef(ast, name)
    lin, _ = generic.linearization(ast, c)
    inline, _ = generic.sibling_code(ast, c, lin)
    return generic.recursion_cycles(c, set(), lin, [node for node, _ in inline])


@unittest.skipUnless(SOLC.is_file(), "needs solc 0.8.35")
class Route(unittest.TestCase):
    def test_free_function_recursion_is_seen(self):
        ast = ast_of(FREE)
        cycles, _ = graph_cycles(ast, "C")
        self.assertEqual([n for n, _ in cycles], [["exp"]])
        # The contract alone, without the inlined free functions, misses it.
        self.assertEqual(generic.recursion_cycles(cdef(ast, "C"), set())[0], [])
        with self.assertRaises(rec_final.Refused) as e:
            rec_final.route(ast, "C")
        self.assertEqual(e.exception.step, "recursion")

    def test_unreached_recursion_is_not_the_contracts(self):
        ast = ast_of(UNREACHED)
        self.assertEqual(graph_cycles(ast, "C")[0], [])
        self.assertIsNone(rec_final.route(ast, "C"))

    def test_recsum_takes_the_route(self):
        ast = generic.solc_ast(str(SOLC), RECSUM)[0]
        r = rec_final.route(ast, "RecSum")
        self.assertEqual(r, {"function": "sum", "measure": "n"})
        self.assertIs(
            rec_final.program("RecSum", "sum"), rec_final.PROGRAMS[("RecSum", "sum")]
        )

    def test_other_program_is_refused(self):
        ast = ast_of(OTHER)
        r = rec_final.route(ast, "D")
        self.assertEqual(r["function"], "f")
        with self.assertRaises(rec_final.Refused) as e:
            rec_final.program("D", "f")
        self.assertEqual(e.exception.step, "program")


class Shape(unittest.TestCase):
    def test_shape_is_certified_with_nonvacuity(self):
        s = rec_final.shape(rec_final.PROGRAMS[("RecSum", "sum")])
        self.assertEqual(s["kind"], "certified")
        self.assertIn("nonvacuous", s)
        self.assertIn("618800", s["gas"])
        self.assertNotEqual(s["proof_module"], s["spec_modules"][0])


@unittest.skipUnless(
    (P2 / "Verify/Phase3/RecSum/gen/RecSum").is_dir(),
    "needs solidity-lean's pinned export",
)
class Export(unittest.TestCase):
    def test_pinned_export_matches_itself_and_refuses_a_change(self):
        prog = rec_final.PROGRAMS[("RecSum", "sum")]
        pinned = P2 / prog["export_dir"]
        self.assertIsNone(rec_final.export_problem(pinned, P2, prog))
        with tempfile.TemporaryDirectory() as d:
            for f in pinned.glob("*.lean"):
                (pathlib.Path(d) / f.name).write_bytes(f.read_bytes())
            ev = pathlib.Path(d) / "Evm6.lean"
            ev.write_text(
                ev.read_text().replace(
                    ".op (.stack (.push 1)),", ".op (.stack (.push 2)),", 1
                )
            )
            self.assertIn("Evm6.lean", rec_final.export_problem(d, P2, prog))

    def test_manifest_argument_order_only(self):
        prog = rec_final.PROGRAMS[("RecSum", "sum")]
        pinned = P2 / prog["export_dir"]
        with tempfile.TemporaryDirectory() as d:
            for f in pinned.glob("*.lean"):
                (pathlib.Path(d) / f.name).write_bytes(f.read_bytes())
            m = pathlib.Path(d) / "Manifest.lean"
            t = m.read_text()
            # The driver passes the source first: the same arguments in another order are the same export.
            moved = t.replace(
                '  ["-Ogas",', '  ["RecSum.sol",\n   "-Ogas",', 1
            ).replace(
                '   "-Zemit-lean-stages=gen",\n   "RecSum.sol"]',
                '   "-Zemit-lean-stages=gen"]',
                1,
            )
            self.assertNotEqual(moved, t)
            m.write_text(moved)
            self.assertIsNone(rec_final.export_problem(d, P2, prog))
            # Another argument is another export.
            m.write_text(moved.replace('"osaka"', '"prague"', 1))
            self.assertIn("Manifest.lean", rec_final.export_problem(d, P2, prog))


if __name__ == "__main__":
    unittest.main()
