"""Direct MIR messages use real child execution, not compiled caller bytecode."""

import json
import subprocess
import unittest

import test_mir_state as state
from test_cli import ROOT, SOLAR

ADDRESS = 262144
OTHER = 327680
RETURN_42 = "602a5f5260205ff3"


def module(body, helper=""):
    return state.module(body) + helper


def call(op="call", destination=ADDRESS, gas=100000, value=0, out_size=32):
    words = [gas, destination]
    if op in {"call", "callcode"}:
        words.append(value)
    words.extend([0, 0, 0, out_size])
    return op + " " + ", ".join(map(str, words))


class MirMessagesTests(unittest.TestCase):
    pair = state.MirStateTests.pair

    def fixture(self, code=RETURN_42, **extra):
        return {
            "accounts": [{"address": ADDRESS, "nonce": 1, "code": "0x" + code}],
            **extra,
        }

    def test_real_sccp_pair_and_broken_call(self):
        folder = ROOT / "examples/mir/external-calls"
        before = (folder / "before.mir").read_text()
        after = subprocess.check_output(
            [
                SOLAR,
                folder / "before.mir",
                "-Ogas",
                "-Zmir-pipeline=sccp",
                "-Zvalidate-ir=true",
            ],
            text=True,
        )
        self.assertEqual(
            after.splitlines()[1:], (folder / "after.mir").read_text().splitlines()[1:]
        )
        fixture = json.loads((folder / "world.json").read_text())
        self.pair(before, after, fixture=fixture)
        _, record = self.pair(
            before, (folder / "broken.mir").read_text(), fixture=fixture, status=1
        )
        self.assertEqual(
            record["witness"]["observations"], record["witness"]["native_observations"]
        )

    def test_real_creation_pair_and_broken_salt(self):
        folder = ROOT / "examples/mir/creation"
        before = (folder / "before.mir").read_text()
        after = subprocess.check_output(
            [
                SOLAR,
                folder / "before.mir",
                "-Ogas",
                "-Zmir-pipeline=sccp",
                "-Zvalidate-ir=true",
            ],
            text=True,
        )
        self.assertEqual(
            after.splitlines()[1:], (folder / "after.mir").read_text().splitlines()[1:]
        )
        fixture = json.loads((folder / "world.json").read_text())
        self.pair(before, after, fixture=fixture)
        _, record = self.pair(
            before, (folder / "broken.mir").read_text(), fixture=fixture, status=1
        )
        self.assertEqual(
            record["witness"]["observations"], record["witness"]["native_observations"]
        )

    def test_call_partial_output_keeps_tail_and_full_return_data(self):
        body = f"mstore 0, 255\nv0 = {call(out_size=1)}\nv1 = returndatasize\nv2 = mload 0\nv3 = add v1, v2\nret v3"
        # The first returned byte is zero; the untouched low byte stays 255.
        after = f"mstore 0, 255\nv0 = {call(out_size=0)}\nret 287"
        self.pair(module(body), module(after), fixture=self.fixture())

    def test_internal_calls_share_return_data(self):
        helper = f"fn @read() {{\nbb0:\nv0 = {call()}\nret\n}}\n"
        before = module("icall @read\nv0 = returndatasize\nret v0", helper)
        after = module(f"v0 = {call()}\nret 32")
        self.pair(before, after, fixture=self.fixture())

    def test_all_call_contexts(self):
        # Return ADDRESS, CALLER, CALLVALUE in three words.
        code = "305f52336020523460405260605ff3"
        fixture = self.fixture(
            code,
            accounts=[
                {"address": ADDRESS, "nonce": 1, "code": "0x" + code},
                {"address": 65536, "balance": 100},
            ],
        )
        for op, owner, caller, value in (
            ("call", ADDRESS, 65536, 7),
            ("callcode", 65536, 65536, 7),
            ("delegatecall", 65536, 131072, 0),
            ("staticcall", ADDRESS, 65536, 0),
        ):
            with self.subTest(op=op):
                prefix = f"v0 = {call(op, value=7, out_size=96)}\n"
                before = module(
                    prefix
                    + "v1 = mload 0\nv2 = mload 32\nv3 = mload 64\nv4 = add v1, v2\nv5 = add v4, v3\nret v5"
                )
                self.pair(
                    before,
                    module(prefix + f"ret {owner + caller + value}"),
                    fixture=fixture,
                )

    def test_child_revert_and_exception_roll_back(self):
        for code, size in (("60075f55602a5f5360015ffd", 1), ("60075f55fe", 0)):
            with self.subTest(code=code):
                fixture = self.fixture(code)
                prefix = f"v0 = {call()}\n"
                self.pair(
                    module(prefix + "v1 = zext i1 v0 to i256\nret v1"),
                    module(prefix + "ret 0"),
                    fixture=fixture,
                )
                self.pair(
                    module(prefix + "v1 = returndatasize\nret v1"),
                    module(prefix + f"ret {size}"),
                    fixture=fixture,
                )

    def test_call_out_of_gas_and_precompile_gas(self):
        for gas, expected in ((14, 0), (15, 1)):
            with self.subTest(gas=gas):
                prefix = f"v0 = {call(destination=4, gas=gas, out_size=0)}\n"
                self.pair(
                    module(prefix + "v1 = zext i1 v0 to i256\nret v1"),
                    module(prefix + f"ret {expected}"),
                )
        prefix = f"v0 = {call(gas=100000)}\n"
        self.pair(
            module(prefix + "v1 = mload 0\nret v1"),
            module(prefix + "ret 99998"),
            fixture=self.fixture("5a5f5260205ff3"),
        )

    def test_empty_call_resets_return_data(self):
        prefix = f"v0 = {call()}\nv1 = {call(destination=OTHER, out_size=0)}\n"
        self.pair(
            module(prefix + "v2 = returndatasize\nret v2"),
            module(prefix + "ret 0"),
            fixture=self.fixture(),
        )

    def test_return_data_copy_bounds_include_empty_copies(self):
        for offset, count, ending in (
            (0, 0, "ret 0"),
            (1, 0, "invalid"),
            (0, 1, "invalid"),
        ):
            with self.subTest(offset=offset, count=count):
                self.pair(
                    module(f"returndatacopy 0, {offset}, {count}\nret 0"),
                    module(ending),
                )
        prefix = f"v0 = {call(out_size=0)}\n"
        self.pair(
            module(prefix + "returndatacopy 64, 31, 1\nv1 = mload 64\nret v1"),
            module(prefix + f"mstore8 64, 42\nret {42 << 248}"),
            fixture=self.fixture(),
        )

    def test_caller_changes_are_visible_to_delegate(self):
        fixture = self.fixture("5f545f5260205ff3")
        prefix = f"sstore 0, 42\nv0 = {call('delegatecall')}\n"
        self.pair(
            module(prefix + "v1 = mload 0\nret v1"),
            module(prefix + "ret 42"),
            fixture=fixture,
        )

    def test_caller_revert_restores_child_world(self):
        fixture = self.fixture("60075f5500")
        self.pair(
            module(f"v0 = {call()}\nrevert 0, 0"),
            module("revert 0, 0"),
            fixture=fixture,
        )

    def test_nested_children_and_unknown_root_code(self):
        nested = "60205f5f5f5f62050000620186a0f15060205ff3"
        fixture = self.fixture(
            nested,
            accounts=[
                {"address": ADDRESS, "nonce": 1, "code": "0x" + nested},
                {"address": OTHER, "nonce": 1, "code": "0x" + RETURN_42},
            ],
        )
        prefix = f"v0 = {call(gas=250000)}\n"
        self.pair(
            module(prefix + "v1 = mload 0\nret v1"),
            module(prefix + "ret 42"),
            fixture=fixture,
        )
        for source, fixture in (
            (module(f"v0 = {call(destination=65536)}\nret 0"), {}),
            (module(f"v0 = {call()}\nret 0"), self.fixture("620100003b00")),
        ):
            with self.subTest(source=source, fixture=fixture):
                result, _ = self.pair(source, source, fixture=fixture, status=2)
                self.assertIn("root runtime code is unknown", result.stderr)

    def test_creation_and_collision(self):
        initcode = "5f5f5360015ff3"
        fixture = {
            "memory": {"data": "0x" + initcode},
            "creation_gas": [100000, 100000],
        }
        prefix = f"v0 = create2 0, 0, {len(bytes.fromhex(initcode))}, 0\nv1 = create2 0, 0, {len(bytes.fromhex(initcode))}, 0\n"
        self.pair(
            module(prefix + "v2 = zext i160 v1 to i256\nret v2"),
            module(prefix + "ret 0"),
            fixture=fixture,
        )

    def test_creation_revert_and_explicit_budgets(self):
        initcode = "602a5f5360015ffd"
        prefix = f"v0 = create 0, 0, {len(bytes.fromhex(initcode))}\n"
        fixture = {"memory": {"data": "0x" + initcode}, "creation_gas": [100000]}
        self.pair(
            module(prefix + "v1 = returndatasize\nret v1"),
            module(prefix + "ret 1"),
            fixture=fixture,
        )
        source = module(prefix + "ret 0")
        result, _ = self.pair(source, source, status=2)
        self.assertIn("missing MIR creation gas budget", result.stderr)
        self.pair(module("ret 0"), module("ret 0"), fixture={"creation_gas": [100000]})
        # A deleted creation remains a detectable state change, not an unused-budget error.
        self.pair(source, module("ret 0"), fixture=fixture, status=1)

    def test_call_input_alias_and_value_transfer(self):
        echo = "365f5f37365ff3"
        fixture = self.fixture(echo)
        before = module(
            f"mstore 0, 42\nv0 = call 100000, {ADDRESS}, 0, 0, 32, 0, 32\nv1 = mload 0\nret v1"
        )
        after = module(
            f"mstore 0, 42\nv0 = call 100000, {ADDRESS}, 0, 0, 32, 0, 32\nret 42"
        )
        self.pair(before, after, fixture=fixture)
        fixture = self.fixture(
            "00",
            accounts=[
                {"address": 65536, "balance": 100},
                {"address": ADDRESS, "nonce": 1, "code": "0x00"},
            ],
        )
        prefix = f"v0 = {call(value=7)}\n"
        self.pair(
            module(prefix + "v1 = selfbalance\nret v1"),
            module(prefix + "ret 93"),
            fixture=fixture,
        )
        prefix = f"v0 = {call(value=101)}\n"
        self.pair(
            module(prefix + "v1 = zext i1 v0 to i256\nret v1"),
            module(prefix + "ret 0"),
            fixture=fixture,
        )

    def test_static_messages_and_parent_logs(self):
        source = module(f"v0 = {call(value=1)}\nret 0")
        self.pair(source, module("invalid"), fixture=self.fixture(static=True))
        prefix = f"v0 = {call('staticcall')}\n"
        self.pair(
            module(prefix + "v1 = zext i1 v0 to i256\nret v1"),
            module(prefix + "ret 0"),
            fixture=self.fixture("60075f5500"),
        )
        prefix = (
            f"mstore8 0, 7\nlog 0, 1\nv0 = {call(out_size=0)}\nmstore8 0, 9\nlog 0, 1\n"
        )
        self.pair(
            module(prefix + "v1 = add 1, 1\nret v1"),
            module(prefix + "ret 2"),
            fixture=self.fixture("602a5f5360015fa000"),
        )

    def test_creation_out_of_gas_and_lifetime(self):
        initcode = "5f5f5360015ff3"
        fixture = {"memory": {"data": "0x" + initcode}, "creation_gas": [1]}
        prefix = f"v0 = create 0, 0, {len(bytes.fromhex(initcode))}\n"
        self.pair(
            module(prefix + "v1 = zext i160 v0 to i256\nret v1"),
            module(prefix + "ret 0"),
            fixture=fixture,
        )
        # A constructor that self-destructs still returns a successful created address.
        # Deletion is deferred until transaction termination, including under Cancun.
        initcode = "62010000ff"
        fixture = {"memory": {"data": "0x" + initcode}, "creation_gas": [100000]}
        prefix = f"v0 = create 0, 0, {len(bytes.fromhex(initcode))}\n"
        self.pair(module(prefix + "stop"), module(prefix + "stop"), fixture=fixture)
        # An ordinary internal return must retain both lifetime flags for its caller.
        self.pair(module(prefix + "ret 0"), module(prefix + "ret 0"), fixture=fixture)

    def test_access_warmth_and_return_data_are_live(self):
        # Even a discarded word result must retain its effects on later child gas.
        result, _ = self.pair(module("v0 = sload 0\nret 0"), module("ret 0"), status=1)
        self.assertIn("Live storage access warmth changed", result.stdout)
        prefix = f"v0 = {call(out_size=0)}\n"
        result, _ = self.pair(
            module(prefix + "ret 0"),
            module(prefix + f"v1 = {call(destination=OTHER, out_size=0)}\nret 0"),
            fixture=self.fixture(),
            status=1,
        )
        self.assertIn("Live return data changed", result.stdout)

    def test_nested_message_work_uses_one_budget(self):
        # The inner child and the caller's continuation each fit the limit alone;
        # their combined instruction count does not. Fuel must not restart on return.
        nested = "60205f5f5f5f62050000620186a0f15060205ff3"
        fixture = self.fixture(
            nested,
            accounts=[
                {"address": ADDRESS, "nonce": 1, "code": "0x" + nested},
                {"address": OTHER, "nonce": 1, "code": "0x" + "5f50" * 10 + "00"},
            ],
            fuel=32,
        )
        source = module(f"v0 = {call(gas=250000)}\nret 0")
        result, _ = self.pair(source, source, fixture=fixture, status=2)
        self.assertIn("evaluation limit", result.stderr)

    def test_discarded_load_changes_child_gas_outcome(self):
        fixture = self.fixture(
            "5f545f5260205ff3",
            accounts=[
                {"address": 65536, "storage": {"0": 42}},
                {"address": ADDRESS, "nonce": 1, "code": "0x5f545f5260205ff3"},
            ],
        )
        suffix = f"v1 = {call('delegatecall', gas=300)}\nv2 = mload 0\nret v2"
        _, record = self.pair(
            module("v0 = sload 0\n" + suffix),
            module(suffix),
            fixture=fixture,
            status=1,
        )
        self.assertEqual(
            [o["values"] for o in record["witness"]["observations"]], [["42"], ["0"]]
        )
        self.assertEqual(
            record["witness"]["observations"], record["witness"]["native_observations"]
        )
