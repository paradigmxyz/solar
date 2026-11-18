"""Pins the record and bind wording the driver writes (tools/certify/generic.py's shape and record
fields, scripts/certify's statuses and caveats), so a change to it is a deliberate one.

  python3 tools/certify/test_record_wording.py
"""

import pathlib
import sys
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import generic

CERTIFY = (HERE.parents[1] / "scripts" / "certify").read_text()
FNS = [{"sig": "get()", "sel": 0x6D4CE63C, "params": []}]

CALLS_ANY = (
    "calls: one frame; the equivalence, the non-payable revert and the specification hold from "
    "every caller below 2^160 other than the contract (ctx.Ok), with the contract address and the "
    "block and transaction fixed; no external calls, GAS reads or return data in the code (the "
    "loop checker's fragment)"
)
CALLS_FIXED = (
    "calls: one frame, caller 0x2000, contract at 0x1000, the block and transaction of "
    "Target.message; no external calls, GAS reads or return data in the code (the loop checker's "
    "fragment)"
)
CALLERS = (
    "callers: from every caller below 2^160 other than the contract, at the gas bound of the "
    "checker with contexts (3845403): the exact bytecode runs as the source, a call with a value "
    "other than 0 reverts with empty output, no logs and unchanged storage, and every result meets "
    "the exported specification's allowed statuses and postcondition read in that caller's "
    "context (partial correctness); the Expected conjunct keeps the fixed frame"
)
ENV_ANY = (
    "caller quantified (every caller below 2^160 other than the contract) for the any-caller "
    "conjuncts; the contract address, block and transaction fixed; the Expected conjunct uses the "
    "fixed frame caller 0x2000, contract 0x1000"
)
VALID = (
    "valid calls: get() ABI-encoded with clean argument words, any value below 2^256 (no function "
    "is payable: the value reverts on both sides), any storage covering the constructor's slots; "
    "other call data is outside the statement; payable functions not exercised"
)
CAVEAT = (
    "not end to end: no solidity-lean headline theorem for {m[1]}; the source and "
    '"\n                "specification facts are those of Final.Statement (Expected assumed, OPEN)'
)
ASSUMED = "Expected (Generic.Expected src spec runtimeBytes) is driver-fixed and assumed: OPEN"


def shape(any_gas=None):
    return generic.shape_json(
        "n_0000abcd", "C", FNS, 3580203, "spec", "get", any_gas=any_gas
    )


class Shape(unittest.TestCase):
    def test_any_caller_calls_and_callers(self):
        ex = shape(3845403)["excluded"]
        self.assertIn(CALLS_ANY, ex)
        self.assertNotIn(CALLS_FIXED, ex)
        self.assertEqual(ex[-1], CALLERS)

    def test_fixed_route_keeps_the_fixed_frame(self):
        ex = shape()["excluded"]
        self.assertIn(CALLS_FIXED, ex)
        self.assertFalse(any(e.startswith("callers:") for e in ex))

    def test_valid_calls(self):
        self.assertIn(VALID, shape()["excluded"])
        self.assertIn(VALID, shape(3845403)["excluded"])

    def test_record_environment(self):
        self.assertEqual(generic.ENVIRONMENT_ANY, ENV_ANY)
        self.assertEqual(
            generic.ENVIRONMENT,
            "one frame, caller 0x2000, contract at 0x1000, the block and transaction of "
            "Target.message (fixed, not quantified)",
        )
        src = (HERE / "generic.py").read_text()
        i = src.index('            record["caller"] = caller\n')
        self.assertIn('record["environment"] = ENVIRONMENT_ANY', src[i : i + 120])


class Certify(unittest.TestCase):
    """scripts/certify's statuses and caveats, pinned in its source."""

    def test_assumptions_open(self):
        self.assertIn(ASSUMED, CERTIFY)
        self.assertIn('if expected else "none beyond the statement\'s scope"', CERTIFY)

    def test_caveat(self):
        self.assertIn(CAVEAT, CERTIFY)
        self.assertIn("Expected assumed, OPEN", CERTIFY)

    def test_status_environment(self):
        for part in (
            '"caller quantified (every caller below 2^160 other than the contract) for the any-caller "',
            '"conjuncts; the contract address, block and transaction fixed; the Expected conjunct uses "',
            '"the fixed frame caller 0x2000, contract 0x1000"',
        ):
            self.assertIn(part, CERTIFY)
        self.assertEqual(
            "".join(
                p.strip('"')
                for p in (
                    "caller quantified (every caller below 2^160 other than the contract) for the any-caller ",
                    "conjuncts; the contract address, block and transaction fixed; the Expected conjunct uses ",
                    "the fixed frame caller 0x2000, contract 0x1000",
                )
            ),
            ENV_ANY,
        )
        self.assertIn(
            'any(e.startswith("callers: ") for e in final["excluded"])', CERTIFY
        )


if __name__ == "__main__":
    unittest.main()
