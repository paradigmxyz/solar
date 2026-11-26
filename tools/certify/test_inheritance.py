"""Inheritance in the generic driver's envelope: the contract is admitted as the Lean frontend
flattens it (CheckedInput.contract over the C3 order), the order must be solc's, calls resolve as
they run in the most-derived contract (final overrides, super), and what the flattening does not
cover yet (base constructor arguments, modifiers on base constructors) is refused by name."""

import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

import generic  # noqa: E402

SOLC = pathlib.Path.home() / ".cache/solc" / f"solc-{generic.SOLC_VERSION}"

HEAD = "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.20;\n"

DIAMOND = HEAD + """
contract A { function f() public virtual returns (uint256) { return 1; } }
contract B { function f() public virtual returns (uint256) { return 2; } }
contract C is A, B {
    function f() public virtual override(A, B) returns (uint256) { return super.f(); }
}
contract D is B, C {
    function f() public override(B, C) returns (uint256) { return super.f(); }
}
"""

SUPER = HEAD + """
contract A { function f() public virtual returns (uint256 r) { return 1; } }
contract B is A { function f() public virtual override returns (uint256 r) { return super.f() | 2; } }
contract C is A { function f() public virtual override returns (uint256 r) { return super.f() | 4; } }
contract D is B, C {
    /// @custom:succeeds-if msg.value == 0
    /// @custom:ensures result == 15
    function f() public override(B, C) returns (uint256 r) { return super.f() | 8; }
}
"""

# f calls the virtual g, which the derived contract overrides to call f: a cycle only through the
# override, which a call graph over declared targets misses.
VIRTUAL_CYCLE = HEAD + """
contract A {
    function f(uint256 x) public returns (uint256) { return g(x); }
    function g(uint256 x) internal virtual returns (uint256) { return x; }
}
contract B is A {
    function g(uint256 x) internal override returns (uint256) { return x == 0 ? 0 : f(x - 1); }
}
"""

BASE_ARGS = HEAD + """
contract A { uint256 public x; constructor(uint256 v) { x = v; } }
contract B is A(5) { function get() public view returns (uint256) { return x; } }
contract C is A { constructor() A(7) {} function get() public view returns (uint256) { return x; } }
"""

BASE_MOD = HEAD + """
contract A {
    uint256 public x;
    modifier m() { x = 1; _; }
    constructor() m() {}
}
contract B is A { function get() public view returns (uint256) { return x; } }
"""

SPEC_IN_BASE = HEAD + """
contract Base {
    uint256 data;
    /// @custom:succeeds-if msg.value == 0
    /// @custom:ensures data == i
    function setData(uint256 i) public { data = i; }
}
contract A is Base { function setViaA(uint256 i) public { setData(i); } }
contract Derived is Base, A {}
"""


def parsed(text):
    d = pathlib.Path(tempfile.mkdtemp())
    src = d / "T.sol"
    src.write_text(text)
    ast, contracts = generic.solc_ast(str(SOLC), src)
    return src, ast, contracts


def contract(ast, name):
    return next(n for n in ast["nodes"] if n.get("name") == name)


def names(lin):
    return [c["name"] for c in lin]


def reasons(text, name):
    src, ast, contracts = parsed(text)
    try:
        generic.envelope(text, ast, contracts, name, None, False, False)
    except generic.Blocked as e:
        return e.reasons
    return []


@unittest.skipUnless(SOLC.exists(), "solc not installed")
class Linearization(unittest.TestCase):
    def test_c3_is_solc_order(self):
        _, ast, _ = parsed(DIAMOND)
        for name, want in (("D", ["D", "C", "B", "A"]), ("C", ["C", "B", "A"]), ("A", ["A"])):
            lin, why = generic.linearization(ast, contract(ast, name))
            self.assertIsNone(why)
            self.assertEqual(names(lin), want)
        _, ast, _ = parsed(SUPER)
        lin, _ = generic.linearization(ast, contract(ast, "D"))
        self.assertEqual(names(lin), ["D", "C", "B", "A"])

    def test_order_other_than_solc_refused(self):
        _, ast, _ = parsed(DIAMOND)
        d = contract(ast, "D")
        d["linearizedBaseContracts"] = list(reversed(d["linearizedBaseContracts"]))
        lin, why = generic.linearization(ast, d)
        self.assertIsNone(lin)
        self.assertIn("not the order solc reports", why)

    def test_super_resolves_in_most_derived(self):
        """In D (D, C, B, A), super.f() in C runs B.f, though C's own order (C, A) names A.f."""
        _, ast, _ = parsed(SUPER)
        lin, _ = generic.linearization(ast, contract(ast, "D"))
        disp = generic.Dispatch(lin)
        fs = {c["name"]: next(n for n in c["nodes"] if n.get("name") == "f") for c in lin}
        for caller, callee in (("D", "C"), ("C", "B"), ("B", "A")):
            call = next(
                n
                for n in generic.walk(fs[caller])
                if n.get("nodeType") == "FunctionCall"
                and n["expression"].get("nodeType") == "MemberAccess"
            )
            ref = call["expression"]["referencedDeclaration"]
            got = disp.target(ref, call["expression"], fs[caller]["id"])
            self.assertEqual(got, fs[callee]["id"], (caller, callee))

    def test_admits_flattened_contract(self):
        src, ast, contracts = parsed(SUPER)
        c, fns, _ = generic.envelope(SUPER, ast, contracts, "D", None, True, False)
        self.assertEqual([f["sig"] for f in fns], ["f()"])
        self.assertTrue(fns[0]["succeeds"] and fns[0]["specified"])

    def test_spec_on_inherited_function(self):
        """A selector's NatSpec is read from its most derived definition, here in the base."""
        src, ast, contracts = parsed(SPEC_IN_BASE)
        _, fns, _ = generic.envelope(SPEC_IN_BASE, ast, contracts, "Derived", None, True, False)
        by = {f["name"]: f for f in fns}
        self.assertTrue(by["setData"]["succeeds"] and by["setData"]["specified"])
        self.assertFalse(by["setViaA"]["specified"])
        self.assertEqual(generic.natspec_checks(ast, "Derived")[0], None)

    def test_recursion_through_override(self):
        rs = reasons(VIRTUAL_CYCLE, "B")
        self.assertIn("recursion", [r["feature"] for r in rs])
        # A alone has no cycle: g is its own.
        self.assertNotIn("recursion", [r["feature"] for r in reasons(VIRTUAL_CYCLE, "A")])

    def test_base_constructor_arguments_refused(self):
        for name in ("B", "C"):
            feats = [r["feature"] for r in reasons(BASE_ARGS, name)]
            self.assertIn("inheritance: base constructor arguments", feats, name)

    def test_modifier_on_base_constructor_refused(self):
        feats = [r["feature"] for r in reasons(BASE_MOD, "B")]
        self.assertIn("inheritance: modifier on a base constructor", feats)

    def test_creation_reach_through_bases(self):
        src, ast, _ = parsed(BASE_MOD)
        cdef = contract(ast, "B")
        lin, _ = generic.linearization(ast, cdef)
        reach, unresolved = generic.creation_reach(cdef, lin)
        self.assertFalse(unresolved)
        mod = next(n for n in generic.walk(contract(ast, "A")) if n.get("nodeType") == "ModifierDefinition")
        self.assertIn(id(mod["body"]), reach)

    def test_linearization_theorem(self):
        src, _, _ = parsed(DIAMOND)
        lean = generic.linearization_lean("n_1", "D", str(SOLC), src)
        self.assertIn('= some ["D", "C", "B", "A"] := by decide', lean)
        self.assertIn('findContract? importedSourceUnit "D"', lean)
        self.assertEqual(generic.linearization_lean("n_1", "A", str(SOLC), src), "")


if __name__ == "__main__":
    unittest.main(verbosity=2)
