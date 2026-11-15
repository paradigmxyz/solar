"""Implicit typed-call gas at explicit caller boundaries, without backend lowering."""

import json
import unittest

import test_mir_abi as abi
import test_mir_address_calls as calls
import test_mir_state as state
from test_cli import ROOT


def read_child_gas(source):
    return source.replace(
        "v4 = zext i1 v3 to i256\nv5 = returndatasize\nv6 = add v4, v5\nret v6",
        "returndatacopy 224, 0, 32\nv4 = mload 224\nret v4",
    )


class MirImplicitGasTests(unittest.TestCase):
    pair = state.MirStateTests.pair
    lower = abi.MirAbiTests.lower

    def test_all_call_kinds_and_input_representations(self):
        for kind in ("call", "staticcall", "delegatecall"):
            for sliced in (False, True):
                with self.subTest(kind=kind, sliced=sliced):
                    before = calls.source(kind, sliced=sliced, gas=None)
                    explicit = calls.source(kind, sliced=sliced)
                    self.pair(
                        before,
                        explicit,
                        fixture=calls.fixture(message_gas=[6400]),
                    )

    def test_child_gas_and_real_sccp_example(self):
        folder = ROOT / "examples/mir/implicit-gas"
        before = (folder / "before.mir").read_text()
        after = (folder / "after.mir").read_text()
        actual = self.lower(before, "sccp")
        self.assertEqual(
            actual[actual.index("@module") :], after[after.index("@module") :]
        )
        world = json.loads((folder / "world.json").read_text())
        self.pair(before, after, fixture=world)
        _, record = self.pair(
            before, (folder / "broken.mir").read_text(), fixture=world, status=1
        )
        obs = record["witness"]["observations"]
        self.assertEqual(obs, record["witness"]["native_observations"])
        self.assertEqual(obs[0]["values"], ["3739"])
        self.assertEqual(obs[1]["values"], ["98"])

    def test_upfront_out_of_gas_rolls_back_caller_writes(self):
        before = calls.source(gas=None).replace(
            "v3 = address_call", "sstore 0, 7\nv3 = address_call"
        )
        _, record = self.pair(
            before,
            state.module("ret 0"),
            fixture=calls.fixture(message_gas=[2599]),
            status=1,
        )
        obs = record["witness"]["observations"]
        self.assertEqual(obs, record["witness"]["native_observations"])
        self.assertEqual(obs[0]["outcome"], "out-of-gas")
        self.assertTrue(all(not account[4] for account in obs[0]["accounts"]))
        _, record = self.pair(
            before,
            before.replace("ret v6", "ret 0"),
            fixture=calls.fixture(message_gas=[2600]),
        )
        self.assertIn("EIP-150", " ".join(record["assumptions"]))

    def test_stipend_and_static_restrictions(self):
        before = read_child_gas(calls.source(gas=None, amount=1))
        after = read_child_gas(calls.source(amount=1))
        world = calls.fixture(
            "5a5f5260205ff3",
            message_gas=[40000],
            accounts=[
                {"address": calls.ADDRESS, "nonce": 1, "code": "0x5a5f5260205ff3"},
                {"address": 65536, "balance": 20},
            ],
        )
        self.pair(before, after, fixture=world)
        self.pair(before, after, fixture={**world, "static": True})
        _, record = self.pair(
            before, before.replace("ret v4", "ret 0"), fixture=world, status=1
        )
        obs = record["witness"]["observations"]
        self.assertEqual(obs, record["witness"]["native_observations"])
        self.assertEqual(obs[0]["values"], ["30255"])

    def test_warmth_and_budget_order_across_calls(self):
        before = calls.source(gas=None).replace(
            "v4 = zext i1 v3 to i256",
            "v7 = address_call v0, v1\nv4 = zext i1 v7 to i256",
        )
        _, record = self.pair(
            before,
            before.replace("ret v6", "ret 0"),
            fixture=calls.fixture("00", message_gas=[6400, 6400]),
            status=1,
        )
        obs = record["witness"]["observations"]
        self.assertEqual(obs, record["witness"]["native_observations"])
        self.assertEqual(obs[0]["message_gas"], [3800, 6300])

    def test_missing_budget_and_explicit_zero_are_distinct(self):
        before = calls.source(gas=None)
        for world in (calls.fixture(), calls.fixture(message_gas=[])):
            with self.subTest(world=world):
                self.pair(before, before, fixture=world, status=2)
        _, record = self.pair(
            before,
            calls.source(gas="0"),
            fixture=calls.fixture(message_gas=[6400]),
            status=1,
        )
        obs = record["witness"]["observations"]
        self.assertEqual(obs, record["witness"]["native_observations"])
        self.assertEqual(obs[0]["values"], ["33"])
        self.assertEqual(obs[1]["values"], ["0"])

    def test_precompile_and_fork_pricing(self):
        for version in ("istanbul", "berlin", "cancun", "prague", "osaka"):
            with self.subTest(version=version):
                before = calls.source(gas=None).replace(str(calls.ADDRESS), "4")
                after = calls.source().replace(str(calls.ADDRESS), "4")
                self.pair(
                    before,
                    after,
                    fixture=calls.fixture(message_gas=[6400]),
                    version=version,
                )
