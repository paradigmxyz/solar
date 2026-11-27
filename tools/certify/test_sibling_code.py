#!/usr/bin/env python3
"""The envelope on sources with several units: a named contract is certified beside its siblings
(contracts, interfaces, abstract contracts and libraries it never deploys or calls), the library and
free functions it runs inline are checked as its own code, and a contract that creates, embeds or
calls a sibling, or runs a public library function by DELEGATECALL, stays refused. Needs solc, no
Lean. Run: python3 tools/certify/test_sibling_code.py"""

import pathlib
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import generic

SOLC = pathlib.Path.home() / ".cache/solc" / f"solc-{generic.SOLC_VERSION}"
DATA = HERE / "testdata" / "siblings"


def envelope(src, contract, lifecycle=False):
    """The envelope's verdict on CONTRACT in SRC: (contract, fns) when admitted, else the blocked
    reasons as (feature, detail) pairs."""
    path = DATA / src
    ast, contracts = generic.solc_ast(str(SOLC), path)
    try:
        contract, fns, _ = generic.envelope(
            path.read_text(), ast, contracts, contract, None, False, lifecycle, []
        )
    except generic.Blocked as b:
        return [(r["feature"], r["detail"]) for r in b.reasons]
    return contract, [f["sig"] for f in fns]


@unittest.skipUnless(SOLC.exists(), f"no solc at {SOLC}")
class SiblingCode(unittest.TestCase):
    def test_target_beside_siblings_admitted(self):
        """Target takes an interface's enum, calls a library's internal function by name and through
        `using`, and a free function; the abstract contract, the other contract and the library's
        loop it never reaches are not its code."""
        self.assertEqual(
            envelope("Siblings.sol", "Target"),
            ("Target", ["kind(uint8)", "put(uint256)"]),
        )

    def test_reached_code_is_closed(self):
        """sibling_code returns exactly the inlined functions Target reaches."""
        path = DATA / "Siblings.sol"
        ast, _ = generic.solc_ast(str(SOLC), path)
        cdef = next(n for n in ast["nodes"] if n.get("name") == "Target")
        inline, delegate = generic.sibling_code(ast, cdef)
        self.assertEqual(
            sorted((u["name"] if u else "", n["name"]) for n, u in inline),
            [("", "inc"), ("Math", "twice")],
        )
        self.assertEqual(delegate, [])

    def test_sibling_with_loop_is_its_own_target(self):
        """The sibling Other has a loop: certifying Other meets it, certifying Target does not."""
        reasons = envelope("Siblings.sol", "Other")
        self.assertIn("loop", [f for f, _ in reasons])

    def test_unnamed_target_refused(self):
        """Without a named contract, several concrete contracts stay refused."""
        path = DATA / "Siblings.sol"
        ast, contracts = generic.solc_ast(str(SOLC), path)
        with self.assertRaises(generic.Blocked) as e:
            generic.envelope(path.read_text(), ast, contracts, None, None)
        self.assertEqual(
            [r["feature"] for r in e.exception.reasons], ["several contracts"]
        )

    def test_public_library_call_refused(self):
        """A public library function called from the contract runs by DELEGATECALL: refused as an
        external call. Called by name inside the library, it is an internal call: admitted."""
        reasons = envelope("DelegateLib.sol", "DelegateLib")
        self.assertEqual([f for f, _ in reasons], ["external call"])
        self.assertIn("Pub.f is a public library function", reasons[0][1])
        self.assertEqual(
            envelope("DelegateLib.sol", "InlineLib"), ("InlineLib", ["run(uint256)"])
        )

    def test_creating_or_calling_a_sibling_refused(self):
        """new, type(...).creationCode and a call through an interface stay refused."""
        self.assertEqual(
            [f for f, _ in envelope("CallsSibling.sol", "Creates")],
            ["contract creation"],
        )
        self.assertEqual(
            [f for f, _ in envelope("CallsSibling.sol", "Embeds")],
            ["contract creation"],
        )
        self.assertEqual(
            [f for f, _ in envelope("CallsSibling.sol", "Calls")], ["external call"]
        )

    def test_target_with_sibling_base_admitted(self):
        """A contract that inherits a sibling takes the inheritance route (linearization, Dispatch):
        the base is part of its code, walked as its own, rather than a sibling."""
        self.assertEqual(envelope("CallsSibling.sol", "Peer"), ("Peer", ["ping()"]))

    def test_inlined_code_meets_the_envelope(self):
        """What the contract reaches through inlined library code is checked as its own code: a loop
        two calls deep, inline assembly, recursion and an external call."""
        for contract, feature in (
            ("ReachesLoop", "loop"),
            ("ReachesAsm", "inline assembly"),
            ("ReachesRec", "recursion"),
            ("ReachesCall", "external call"),
        ):
            with self.subTest(contract):
                self.assertEqual(
                    [f for f, _ in envelope("LibInline.sol", contract)], [feature]
                )

    def test_inlined_loop_at_lifecycle(self):
        """At the lifecycle tier a loop in inlined code only runtime functions reach is opened, as the
        contract's own runtime loops are; one the constructor reaches through the library blocks."""
        self.assertEqual(
            envelope("LibInline.sol", "ReachesLoop", lifecycle=True),
            ("ReachesLoop", ["f(uint256)"]),
        )
        self.assertEqual(
            [f for f, _ in envelope("LibInline.sol", "CtorLoop", lifecycle=True)],
            ["loop"],
        )


class StatementBindsTarget(unittest.TestCase):
    def test_source_contract_is_the_exported_one(self):
        """final.py binds the export of --contract: a shape whose source contract is another unit of
        the source (a sibling) is refused, the same one is written."""
        from test_generic import expected_shape, final_py

        with tempfile.TemporaryDirectory() as t:
            code, err, _ = final_py(expected_shape(source_contract="C"), pathlib.Path(t))
            self.assertEqual(code, 0, err)
        with tempfile.TemporaryDirectory() as t:
            code, err, _ = final_py(
                expected_shape(source_contract="Sibling"), pathlib.Path(t)
            )
            self.assertNotEqual(code, 0)
            self.assertIn("source_contract Sibling is not --contract C", err)


if __name__ == "__main__":
    unittest.main()
