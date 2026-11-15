"""Caller-boundary gas, independent message replay and real MIR pass pairs."""

import json
import unittest

import test_mir_abi as abi
import test_mir_messages as messages
import test_mir_state as state
from test_cli import ROOT


class MirMessageGasTests(unittest.TestCase):
    pair = state.MirStateTests.pair
    fixture = messages.MirMessagesTests.fixture
    lower_pass = abi.MirAbiTests.lower

    def test_capped_call_real_sccp_and_broken_budget(self):
        before = state.module(
            f"v0 = add 50000, 50000\nv1 = {messages.call(gas='v0')}\nv2 = zext i1 v1 to i256\nret v2"
        )
        after = self.lower(before)
        fixture = self.fixture(code="00", message_gas=[6400])
        self.pair(before, after, fixture=fixture)
        broken = state.module(
            f"v1 = {messages.call(gas=0)}\nv2 = zext i1 v1 to i256\nret v2"
        )
        _, record = self.pair(
            before, broken, fixture=self.fixture(message_gas=[6400]), status=1
        )
        self.assertEqual(
            record["witness"]["observations"], record["witness"]["native_observations"]
        )

    def lower(self, before):
        return self.lower_pass(before, "sccp")

    def test_cold_and_warm_call_remaining_gas(self):
        before = state.module(
            f"v0 = {messages.call(out_size=0)}\nv1 = {messages.call(out_size=0)}\nv2 = zext i1 v1 to i256\nret v2"
        )
        broken = before.replace("ret v2", "ret 0")
        _, record = self.pair(
            before,
            broken,
            fixture=self.fixture(code="00", message_gas=[6400, 6400]),
            status=1,
        )
        observations = record["witness"]["observations"]
        self.assertEqual(observations, record["witness"]["native_observations"])
        self.assertEqual(observations[0]["message_gas"], [3800, 6300])

    def test_upfront_out_of_gas_and_roll_back(self):
        before = state.module(
            f"sstore 0, 7\nv0 = {messages.call()}\nv2 = zext i1 v0 to i256\nret v2"
        )
        _, record = self.pair(
            before,
            state.module("ret 0"),
            fixture=self.fixture(message_gas=[2599]),
            status=1,
        )
        obs = record["witness"]["observations"]
        self.assertEqual(obs, record["witness"]["native_observations"])
        self.assertEqual(obs[0]["outcome"], "out-of-gas")
        self.assertEqual(obs[0]["accounts"][0][4], [])

    def test_value_stipend_and_new_account_charge(self):
        fixture = {
            "accounts": [{"address": 65536, "balance": 10}],
            "message_gas": [40000],
        }
        before = state.module(
            f"v0 = {messages.call(gas=0, value=1, out_size=0)}\nv2 = zext i1 v0 to i256\nret v2"
        )
        _, record = self.pair(
            before, before.replace("ret v2", "ret 0"), fixture=fixture, status=1
        )
        obs = record["witness"]["observations"]
        self.assertEqual(obs, record["witness"]["native_observations"])
        self.assertEqual(obs[0]["message_gas"], [5700])

    def test_create_and_create2_derive_child_budget(self):
        for op, extra in (("create", ""), ("create2", ", 17")):
            with self.subTest(op=op):
                before = state.module(
                    f"v0 = {op} 0, 0, 0{extra}\nv2 = zext i160 v0 to i256\nret v2"
                )
                self.pair(before, self.lower(before), fixture={"message_gas": [40000]})
                _, record = self.pair(
                    before,
                    state.module("ret 0"),
                    fixture={"message_gas": [31999]},
                    status=1,
                )
                obs = record["witness"]["observations"]
                self.assertEqual(obs, record["witness"]["native_observations"])
                self.assertEqual(obs[0]["outcome"], "out-of-gas")

    def test_missing_and_competing_budgets_are_unsupported(self):
        before = state.module(
            f"v0 = {messages.call()}\nv2 = zext i1 v0 to i256\nret v2"
        )
        for fixture in (
            {"message_gas": []},
            {"message_gas": [6400], "creation_gas": [100]},
        ):
            with self.subTest(fixture=fixture):
                self.pair(before, before, fixture=fixture, status=2)

    def test_gas_example_and_independent_broken_forwarding(self):
        folder = ROOT / "examples/mir/message-gas"
        before = (folder / "before.mir").read_text()
        after = (folder / "after.mir").read_text()
        actual = self.lower(before)
        self.assertEqual(
            actual[actual.index("@module") :], after[after.index("@module") :]
        )
        fixture = json.loads((folder / "world.json").read_text())
        self.pair(before, after, fixture=fixture)
        _, record = self.pair(
            before, (folder / "broken.mir").read_text(), fixture=fixture, status=1
        )
        obs = record["witness"]["observations"]
        self.assertEqual(obs, record["witness"]["native_observations"])
        self.assertEqual(obs[0]["values"], ["3736"])
        self.assertEqual(obs[1]["values"], ["0"])

    def test_fork_access_price_and_child_failure_gas(self):
        for version, code, remaining in (
            ("istanbul", "00", 5700),
            ("berlin", "00", 3800),
            ("cancun", "5f5ffd", 3796),
            ("cancun", "fe", 59),
        ):
            with self.subTest(version=version, code=code):
                before = state.module(f"v0 = {messages.call(out_size=0)}\nret 17")
                _, record = self.pair(
                    before,
                    before.replace("ret 17", "ret 0"),
                    fixture=self.fixture(code=code, message_gas=[6400]),
                    version=version,
                    status=1,
                )
                obs = record["witness"]["observations"]
                self.assertEqual(obs, record["witness"]["native_observations"])
                self.assertEqual(obs[0]["message_gas"], [remaining])

    def test_delegation_resolution_is_charged_before_forwarding(self):
        authority = "ef0100" + messages.OTHER.to_bytes(20, "big").hex()
        fixture = {
            "message_gas": [6400],
            "accounts": [
                {"address": messages.ADDRESS, "nonce": 1, "code": "0x" + authority},
                {"address": messages.OTHER, "nonce": 1, "code": "0x00"},
            ],
        }
        before = state.module(f"v0 = {messages.call(out_size=0)}\nret 17")
        _, record = self.pair(
            before,
            before.replace("ret 17", "ret 0"),
            fixture=fixture,
            version="prague",
            status=1,
        )
        obs = record["witness"]["observations"]
        self.assertEqual(obs, record["witness"]["native_observations"])
        self.assertEqual(obs[0]["message_gas"], [1200])

    def test_memory_initcode_and_hashing_charges(self):
        for version, op, expected in (
            ("paris", "create", 8000),
            ("cancun", "create", 7998),
            ("cancun", "create2", 7992),
        ):
            with self.subTest(version=version, op=op):
                salt = ", 17" if op == "create2" else ""
                before = state.module(f"mstore 0, 0\nv0 = {op} 0, 0, 1{salt}\nret 17")
                _, record = self.pair(
                    before,
                    before.replace("ret 17", "ret 0"),
                    fixture={"message_gas": [40000]},
                    version=version,
                    status=1,
                )
                obs = record["witness"]["observations"]
                self.assertEqual(obs, record["witness"]["native_observations"])
                self.assertEqual(obs[0]["message_gas"], [expected])

    def test_initcode_limit_is_out_of_gas(self):
        before = state.module("v0 = create 0, 0, 49153\nret 0")
        _, record = self.pair(
            before, state.module("ret 0"), fixture={"message_gas": [40000]}, status=1
        )
        obs = record["witness"]["observations"]
        self.assertEqual(obs, record["witness"]["native_observations"])
        self.assertEqual(obs[0]["outcome"], "out-of-gas")

    def test_remaining_gas_is_live_even_with_equal_call_results(self):
        before = state.module(f"v0 = {messages.call(gas=1000, out_size=0)}\nret 17")
        after = before.replace("call 1000,", "call 500,")
        _, record = self.pair(
            before, after, fixture=self.fixture(code="fe", message_gas=[6400]), status=1
        )
        obs = record["witness"]["observations"]
        self.assertEqual(obs, record["witness"]["native_observations"])
        self.assertEqual(obs[0]["values"], obs[1]["values"])
        self.assertEqual([o["message_gas"] for o in obs], [[2800], [3300]])

    def test_upfront_failure_does_not_execute_unknown_root(self):
        before = state.module(f"v0 = {messages.call(destination=65536)}\nret 0")
        _, record = self.pair(
            before, state.module("ret 0"), fixture={"message_gas": [0]}, status=1
        )
        obs = record["witness"]["observations"]
        self.assertEqual(obs, record["witness"]["native_observations"])
        self.assertEqual(obs[0]["outcome"], "out-of-gas")

    def test_constructor_records_the_forwarded_gas_cap(self):
        # GAS; PUSH0; MSTORE; PUSH1 32; PUSH0; RETURN publishes the gas word as code.
        initcode = int.from_bytes(
            bytes.fromhex("5a5f5260205ff3").ljust(32, b"\0"), "big"
        )
        for op, salt, expected_word, remaining in (
            ("create", "", 7872, 1583),
            ("create2", ", 17", 7866, 1577),
        ):
            with self.subTest(op=op):
                before = state.module(
                    f"mstore 0, {initcode}\nv0 = add 0, 0\nv1 = {op} v0, 0, 7{salt}\nret 17"
                )
                fixture = {"message_gas": [40000]}
                self.pair(before, self.lower(before), fixture=fixture)
                _, record = self.pair(
                    before, before.replace("ret 17", "ret 0"), fixture=fixture, status=1
                )
                obs = record["witness"]["observations"]
                self.assertEqual(obs, record["witness"]["native_observations"])
                created = [account for account in obs[0]["accounts"] if account[6]]
                self.assertEqual(len(created), 1)
                self.assertEqual(
                    int.from_bytes(bytes(created[0][3]), "big"), expected_word
                )
                self.assertEqual(obs[0]["message_gas"], [remaining])
