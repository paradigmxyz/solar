"""The driver's decided NatSpec checks (generic.natspec_checks and friends): each refutation is
REJECTED with the reason its registered stage names, before any proof, and a clause outside the
decided fragment is left alone (recorded OPEN, never accepted). The sources under
testdata/fixplan are tools/certify-adversarial's cases, byte for byte.

  python3 tools/certify/test_natspec_checks.py
"""

import pathlib
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import generic

SOLC = pathlib.Path.home() / ".cache" / "solc" / f"solc-{generic.SOLC_VERSION}"
FIX = HERE / "testdata" / "fixplan"
# The stages tools/certify-adversarial/stages.tsv registers for these cases.
STAGE = {
    "InitBrokenR2": r"^REJECTED: .*\binvariant\b.*\b(initial|constructor|deploy)",
    "InvBrokenR2": r"^REJECTED: .*\binvariant\b.*\breset\b",
    "FlagSucceedsAlways": r"^REJECTED: spec: \S*\blower_succeeds0 does not check",
}


def ast_of(text):
    with tempfile.NamedTemporaryFile("w", suffix=".sol", delete=False) as f:
        f.write(text)
    return generic.solc_ast(str(SOLC), pathlib.Path(f.name))[0]


def checks(name, contract=None):
    ast, _ = generic.solc_ast(str(SOLC), FIX / f"{name}.sol")
    return generic.natspec_checks(ast, contract or name)


HEAD = "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.20;\n"


class Invariant(unittest.TestCase):
    def test_initial_state_refuted(self):
        why, _ = checks("InitBrokenR2")
        self.assertEqual(
            why,
            "invariant: x >= 1 fails in the initial state (the constructor leaves x = 0)",
        )
        self.assertRegex(f"REJECTED: {why}", STAGE["InitBrokenR2"])

    def test_preservation_refuted(self):
        why, _ = checks("InvBrokenR2")
        self.assertEqual(
            why, "invariant: x >= 1 is not preserved by reset (it sets x = 0)"
        )
        self.assertRegex(f"REJECTED: {why}", STAGE["InvBrokenR2"])

    def test_holding_invariant_is_open_not_accepted(self):
        ast = ast_of(
            HEAD + "/// @custom:invariant x >= 1\ncontract C {\n uint256 public x;\n"
            " constructor() { x = 1; }\n function set() external { x = 2; }\n"
            " function inc() external { x = x + 1; }\n}\n"
        )
        why, invs = generic.natspec_checks(ast, "C")
        self.assertIsNone(why)
        self.assertEqual(
            invs, [{"invariant": "x >= 1", "status": "OPEN: not proved by the driver"}]
        )

    def test_outside_fragment_is_open(self):
        for inv in ("x >= 1 || x == 0", "x >= y", "!(x == 0)"):
            with self.subTest(inv):
                ast = ast_of(
                    HEAD
                    + f"/// @custom:invariant {inv}\ncontract C {{\n uint256 public x;\n"
                    " uint256 public y;\n function reset() external { x = 0; }\n}\n"
                )
                why, invs = generic.natspec_checks(ast, "C")
                self.assertIsNone(why)
                self.assertEqual(invs[0]["status"], "OPEN: not proved by the driver")

    def test_non_literal_constructor_is_not_decided(self):
        ast = ast_of(
            HEAD + "/// @custom:invariant x >= 1\ncontract C {\n uint256 public x;\n"
            " constructor(uint256 n) { x = n; }\n}\n"
        )
        self.assertIsNone(generic.natspec_checks(ast, "C")[0])

    def test_driver_rejects_before_any_proof(self):
        src = (HERE / "generic.py").read_text()
        i = src.index("refuted, invs = natspec_checks(ast, contract)")
        self.assertLess(i, src.index('record["environment"] = ENVIRONMENT'))
        self.assertIn("raise Failed(refuted)", src[i : i + 300])


class NonpayableSucceeds(unittest.TestCase):
    def test_succeeds_if_without_value_guard_refuted(self):
        why, _ = checks("FlagSucceedsAlways")
        self.assertEqual(
            why,
            "spec: lower_succeeds0 does not check: lower is nonpayable, so a call with a value other "
            "than 0 reverts, but its @custom:succeeds-if true does not require msg.value == 0",
        )
        self.assertRegex(f"REJECTED: {why}", STAGE["FlagSucceedsAlways"])

    def test_guarded_and_payable_pass(self):
        for fn in (
            "/// @custom:succeeds-if msg.value == 0\n function f() external {}",
            "/// @custom:succeeds-if msg.value == 0 && x < 5\n function f() external {}",
            "/// @custom:succeeds-if (0 == msg.value)\n function f() external {}",
            "/// @custom:succeeds-if true\n function f() external payable {}",
        ):
            with self.subTest(fn):
                ast = ast_of(
                    HEAD + "contract C {\n uint256 public x;\n " + fn + "\n}\n"
                )
                self.assertIsNone(generic.natspec_checks(ast, "C")[0])

    def test_disjunction_left_to_the_proofs(self):
        ast = ast_of(
            HEAD
            + "contract C {\n uint256 public x;\n /// @custom:succeeds-if msg.value == 0 || x == 1\n"
            " function f() external {}\n}\n"
        )
        self.assertIsNone(generic.natspec_checks(ast, "C")[0])


class Fragment(unittest.TestCase):
    def test_atoms(self):
        self.assertEqual(
            generic.spec_atoms("y < 3 && y > 5"), [("y", "<", 3), ("y", ">", 5)]
        )
        self.assertEqual(generic.spec_atoms("(3 > y)"), [("y", "<", 3)])
        for c in ("y < z", "a || b", "!(y == 1)", "y + 1 < 3", "msg.value == 0", ""):
            self.assertIsNone(generic.spec_atoms(c), c)


if __name__ == "__main__":
    unittest.main()
