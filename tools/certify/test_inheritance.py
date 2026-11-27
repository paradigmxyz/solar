"""Inheritance in the generic driver's envelope: the contract is admitted as the Lean frontend
flattens it (CheckedInput.contract over the C3 order), the order must be solc's, calls resolve as
they run in the most-derived contract (final overrides, super), and what the flattening does not
cover yet (base constructor arguments, modifiers on base constructors) is refused by name."""

import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

import generic

HERE = pathlib.Path(__file__).resolve().parent

SOLC = pathlib.Path.home() / ".cache/solc" / f"solc-{generic.SOLC_VERSION}"

HEAD = "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.20;\n"

DIAMOND = (
    HEAD
    + """
contract A { function f() public virtual returns (uint256) { return 1; } }
contract B { function f() public virtual returns (uint256) { return 2; } }
contract C is A, B {
    function f() public virtual override(A, B) returns (uint256) { return super.f(); }
}
contract D is B, C {
    function f() public override(B, C) returns (uint256) { return super.f(); }
}
"""
)

SUPER = (
    HEAD
    + """
contract A { function f() public virtual returns (uint256 r) { return 1; } }
contract B is A { function f() public virtual override returns (uint256 r) { return super.f() | 2; } }
contract C is A { function f() public virtual override returns (uint256 r) { return super.f() | 4; } }
contract D is B, C {
    /// @custom:succeeds-if msg.value == 0
    /// @custom:ensures result == 15
    function f() public override(B, C) returns (uint256 r) { return super.f() | 8; }
}
"""
)

# f calls the virtual g, which the derived contract overrides to call f: a cycle only through the
# override, which a call graph over declared targets misses.
VIRTUAL_CYCLE = (
    HEAD
    + """
contract A {
    function f(uint256 x) public returns (uint256) { return g(x); }
    function g(uint256 x) internal virtual returns (uint256) { return x; }
}
contract B is A {
    function g(uint256 x) internal override returns (uint256) { return x == 0 ? 0 : f(x - 1); }
}
"""
)

BASE_ARGS = (
    HEAD
    + """
contract A { uint256 public x; constructor(uint256 v) { x = v; } }
contract B is A(5) { function get() public view returns (uint256) { return x; } }
contract C is A { constructor() A(7) {} function get() public view returns (uint256) { return x; } }
"""
)

BASE_MOD = (
    HEAD
    + """
contract A {
    uint256 public x;
    modifier m() { x = 1; _; }
    constructor() m() {}
}
contract B is A { function get() public view returns (uint256) { return x; } }
"""
)

SPEC_IN_BASE = (
    HEAD
    + """
contract Base {
    uint256 data;
    /// @custom:succeeds-if msg.value == 0
    /// @custom:ensures data == i
    function setData(uint256 i) public { data = i; }
}
contract A is Base { function setViaA(uint256 i) public { setData(i); } }
contract Derived is Base, A {}
"""
)

# Review 184's counterexample: solc's C3 order of D is D, C, X, B, A. super.f() in C names B.f in the
# AST, but in D it runs X.f, the next f in D's order with C's resolved signature, though X.f does not
# override B.f: D.f returns 31 (an override-chain dispatch would skip X and return 27).
SUPER_SKIP = (
    HEAD
    + """
contract A { function f() public virtual returns (uint256) { return 1; } }
contract B is A { function f() public virtual override returns (uint256) { return super.f() | 2; } }
contract X is A { function f() public virtual override returns (uint256) { return super.f() | 4; } }
contract C is B { function f() public virtual override returns (uint256) { return super.f() | 8; } }
contract D is B, X, C {
    function f() public override(B, X, C) returns (uint256) { return super.f() | 16; }
}
"""
)

# Review 184's modifier counterexample: in N, M's f uses the unqualified m, which runs N's override
# (no require) and returns 3; N's g uses the qualified M.m, which runs M's m and reverts.
QUALIFIED_MOD = (
    HEAD
    + """
contract M {
    modifier m() virtual { require(false); _; }
    function f() public m returns (uint256) { return 3; }
}
contract N is M {
    modifier m() override { _; }
    function g() public M.m returns (uint256) { return 3; }
}
"""
)

FORGE = shutil.which("forge")


def forge_results(source, probe, optimize):
    """Runs PROBE (a contract with test_ functions, appended to SOURCE) with forge's EVM, SOURCE and
    PROBE compiled by solc SOLC_VERSION (SOLC), with or without the optimizer: test name to status."""
    d = pathlib.Path(tempfile.mkdtemp())
    (d / "src").mkdir()
    (d / "src" / "T.sol").write_text(source + probe)
    (d / "foundry.toml").write_text(
        '[profile.default]\nsrc = "src"\ntest = "src"\nout = "out"\nlibs = []\n'
        f"optimizer = {'true' if optimize else 'false'}\n"
    )
    out = subprocess.run(
        [FORGE, "test", "--offline", "--use", str(SOLC), "--json"],
        cwd=d,
        check=False,
        capture_output=True,
        text=True,
        timeout=600,
    )
    line = next((l for l in out.stdout.splitlines() if l.startswith("{")), None)
    if line is None:
        raise AssertionError(f"forge test: {out.stdout[-800:]} {out.stderr[-800:]}")
    res = json.loads(line)
    return {
        t: r["status"]
        for suite in res.values()
        for t, r in suite["test_results"].items()
    }


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
    _, ast, contracts = parsed(text)
    try:
        generic.envelope(text, ast, contracts, name, None, False, False)
    except generic.Blocked as e:
        return e.reasons
    return []


@unittest.skipUnless(SOLC.exists(), "solc not installed")
class Linearization(unittest.TestCase):
    def test_c3_is_solc_order(self):
        _, ast, _ = parsed(DIAMOND)
        for name, want in (
            ("D", ["D", "C", "B", "A"]),
            ("C", ["C", "B", "A"]),
            ("A", ["A"]),
        ):
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
        fs = {
            c["name"]: next(n for n in c["nodes"] if n.get("name") == "f") for c in lin
        }
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

    def test_super_runs_next_in_order_not_override_chain(self):
        """Review 184: D's order is D, C, X, B, A, and super.f() in C (B.f in the AST) runs X.f. Dispatch's
        chain from D.f ORs every contract's bit (31), and solc 0.8.35's D.f returns that."""
        _, ast, _ = parsed(SUPER_SKIP)
        lin, _ = generic.linearization(ast, contract(ast, "D"))
        self.assertEqual(names(lin), ["D", "C", "X", "B", "A"])
        disp = generic.Dispatch(lin)
        fs = {
            c["name"]: next(n for n in c["nodes"] if n.get("name") == "f") for c in lin
        }
        bit = {"A": 1, "B": 2, "X": 4, "C": 8, "D": 16}
        name_of = {fs[k]["id"]: k for k in fs}
        chain, cur = ["D"], fs["D"]
        while True:
            call = next(
                (
                    n
                    for n in generic.walk(cur)
                    if n.get("nodeType") == "FunctionCall"
                    and n["expression"].get("nodeType") == "MemberAccess"
                ),
                None,
            )
            if call is None:
                break
            got = disp.target(
                call["expression"]["referencedDeclaration"],
                call["expression"],
                cur["id"],
            )
            chain.append(name_of[got])
            cur = fs[name_of[got]]
        self.assertEqual(chain, ["D", "C", "X", "B", "A"])
        predicted = 0
        for k in chain:
            predicted |= bit[k]
        self.assertEqual(predicted, 31)

    @unittest.skipUnless(FORGE, "forge not installed")
    def test_super_counterexample_runs_like_solc(self):
        """solc 0.8.35's D.f returns 31 (optimized and not), the value Dispatch's chain predicts."""
        probe = "contract Probe { function test_d() public { require(new D().f() == 31); } }\n"
        for optimize in (True, False):
            self.assertEqual(
                forge_results(SUPER_SKIP, probe, optimize),
                {"test_d()": "Success"},
                optimize,
            )

    def test_qualified_modifier_is_static(self):
        """Review 184: in N, M's f (the unqualified m) runs N's m, and g (M.m) runs M's m."""
        _, ast, _ = parsed(QUALIFIED_MOD)
        lin, _ = generic.linearization(ast, contract(ast, "N"))
        disp = generic.Dispatch(lin)
        mods = {
            c["name"]: next(
                n for n in c["nodes"] if n.get("nodeType") == "ModifierDefinition"
            )
            for c in lin
        }
        fns = {
            n["name"]: n
            for c in lin
            for n in c["nodes"]
            if n.get("nodeType") == "FunctionDefinition"
        }
        got = {}
        for fname, want in (("f", "N"), ("g", "M")):
            inv = fns[fname]["modifiers"][0]
            got[fname] = disp.target(
                inv["modifierName"]["referencedDeclaration"],
                inv["modifierName"],
                fns[fname]["id"],
            )
            self.assertEqual(got[fname], mods[want]["id"], fname)
        # What Dispatch predicts: the modifier with require(false) reverts, the other returns 3.
        reverts = {
            k: any(
                n.get("nodeType") == "Identifier" and n.get("name") == "require"
                for n in generic.walk(disp.callables[k])
            )
            for k in got.values()
        }
        self.assertEqual(
            {f: reverts[k] for f, k in got.items()}, {"f": False, "g": True}
        )

    @unittest.skipUnless(FORGE, "forge not installed")
    def test_modifier_counterexample_runs_like_solc(self):
        """solc 0.8.35: N's f returns 3 and N's g reverts (optimized and not), as Dispatch predicts."""
        probe = (
            "contract Probe {\n"
            "    function test_f() public { require(new N().f() == 3); }\n"
            "    function test_g() public {\n"
            "        N n = new N();\n"
            '        try n.g() returns (uint256) { revert("g returned"); } catch {}\n'
            "    }\n"
            "}\n"
        )
        for optimize in (True, False):
            self.assertEqual(
                forge_results(QUALIFIED_MOD, probe, optimize),
                {"test_f()": "Success", "test_g()": "Success"},
                optimize,
            )

    def test_admits_flattened_contract(self):
        _, ast, contracts = parsed(SUPER)
        _, fns, _ = generic.envelope(SUPER, ast, contracts, "D", None, True, False)
        self.assertEqual([f["sig"] for f in fns], ["f()"])
        self.assertTrue(fns[0]["succeeds"] and fns[0]["specified"])

    def test_spec_on_inherited_function(self):
        """A selector's NatSpec is read from its most derived definition, here in the base."""
        _, ast, contracts = parsed(SPEC_IN_BASE)
        _, fns, _ = generic.envelope(
            SPEC_IN_BASE, ast, contracts, "Derived", None, True, False
        )
        by = {f["name"]: f for f in fns}
        self.assertTrue(by["setData"]["succeeds"] and by["setData"]["specified"])
        self.assertFalse(by["setViaA"]["specified"])
        self.assertEqual(generic.natspec_checks(ast, "Derived")[0], None)

    def test_recursion_through_override(self):
        rs = reasons(VIRTUAL_CYCLE, "B")
        self.assertIn("recursion", [r["feature"] for r in rs])
        # A alone has no cycle: g is its own.
        self.assertNotIn(
            "recursion", [r["feature"] for r in reasons(VIRTUAL_CYCLE, "A")]
        )

    def test_base_constructor_arguments_refused(self):
        for name in ("B", "C"):
            feats = [r["feature"] for r in reasons(BASE_ARGS, name)]
            self.assertIn("inheritance: base constructor arguments", feats, name)

    def test_modifier_on_base_constructor_refused(self):
        feats = [r["feature"] for r in reasons(BASE_MOD, "B")]
        self.assertIn("inheritance: modifier on a base constructor", feats)

    def test_creation_reach_through_bases(self):
        _, ast, _ = parsed(BASE_MOD)
        cdef = contract(ast, "B")
        lin, _ = generic.linearization(ast, cdef)
        reach, unresolved = generic.creation_reach(cdef, lin)
        self.assertFalse(unresolved)
        mod = next(
            n
            for n in generic.walk(contract(ast, "A"))
            if n.get("nodeType") == "ModifierDefinition"
        )
        self.assertIn(id(mod["body"]), reach)

    def test_linearization_module(self):
        """The module states the frontend's order over NAME.Source's unit; Source.lean is untouched."""
        _, ast, _ = parsed(DIAMOND)
        lean = generic.linearization_module("n_0123abcd", "D", ast, "ab" * 32)
        self.assertIn("\nimport n_0123abcd.Source\n", lean)
        self.assertIn("namespace Certified.n_0123abcd.Linearization", lean)
        self.assertIn('= some ["D", "C", "B", "A"] := by decide', lean)
        self.assertIn(
            'findContract? Certified.n_0123abcd.Src.importedSourceUnit "D"', lean
        )
        self.assertIn("sha256 " + "ab" * 32, lean)
        self.assertEqual(
            generic.linearization_module("n_0123abcd", "A", ast, "ab" * 32), ""
        )
        with self.assertRaises(generic.Failed):
            generic.linearization_module("n_0123abcd", "Z", ast, "ab" * 32)

    def test_linearization_module_refuses_unchecked_order(self):
        """No module without the theorem: an order the driver does not certify raises."""
        _, ast, _ = parsed(DIAMOND)
        d = contract(ast, "D")
        d["linearizedBaseContracts"] = list(reversed(d["linearizedBaseContracts"]))
        with self.assertRaises(generic.Failed):
            generic.linearization_module("n_0123abcd", "D", ast, "ab" * 32)


@unittest.skipUnless(SOLC.exists(), "solc not installed")
class LinearizationBind(unittest.TestCase):
    """tools/certify/linearization_bind.sh against a lean directory that pins this solc."""

    def setUp(self):
        self.lean = pathlib.Path(tempfile.mkdtemp())
        (self.lean / "scripts/refinement").mkdir(parents=True)
        sha = hashlib.sha256(SOLC.read_bytes()).hexdigest()
        (self.lean / "scripts/refinement/gen.sh").write_text(
            f"solc_version={generic.SOLC_VERSION}\nsolc_sha256={sha}\n"
        )
        self.dir = self.lean / "Certified/n_0123abcd"
        self.dir.mkdir(parents=True)
        self.src = self.dir / "n_0123abcd.sol"
        self.src.write_text(DIAMOND)

    def bind(self, contract):
        return subprocess.run(
            [
                "bash",
                str(HERE / "linearization_bind.sh"),
                str(self.lean),
                str(self.src),
                "Certified/n_0123abcd",
                "n_0123abcd",
                contract,
            ],
            capture_output=True,
            check=False,
            text=True,
            env=dict(os.environ, SOLC=str(SOLC)),
        )

    def module(self, contract):
        ast, _ = generic.solc_ast(str(SOLC), self.src)
        sha = hashlib.sha256(self.src.read_bytes()).hexdigest()
        return generic.linearization_module("n_0123abcd", contract, ast, sha)

    def test_binds_the_driver_module(self):
        (self.dir / "Linearization.lean").write_text(self.module("D"))
        r = self.bind("D")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertTrue(
            r.stdout.strip().endswith(": n_0123abcd.Linearization"), r.stdout
        )

    def test_missing_module_fails(self):
        r = self.bind("D")
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("is missing", r.stderr)

    def test_tampered_order_fails(self):
        (self.dir / "Linearization.lean").write_text(
            self.module("D").replace('"C", "B"', '"B", "C"')
        )
        r = self.bind("D")
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("is not the C3 theorem", r.stderr)

    def test_edited_source_fails(self):
        (self.dir / "Linearization.lean").write_text(self.module("D"))
        self.src.write_text(DIAMOND + "\n// edited\n")
        r = self.bind("D")
        self.assertNotEqual(r.returncode, 0)

    def test_no_bases(self):
        r = self.bind("A")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("linearization: none:", r.stdout)
        # A module for a contract without bases is stale data, not ignored.
        (self.dir / "Linearization.lean").write_text(self.module("D"))
        r = self.bind("A")
        self.assertNotEqual(r.returncode, 0)


if __name__ == "__main__":
    unittest.main(verbosity=2)
