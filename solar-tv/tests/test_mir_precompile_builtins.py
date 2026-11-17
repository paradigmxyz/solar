"""Precompile extcalls: crypto, exact memory, gas failure and replay."""

import hashlib
import json
import unittest

import test_mir_abi as abi
import test_mir_abi_encode_recursive as encoding
import test_mir_state as state
from test_cli import ROOT

GENERATOR_X = int(
    "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798", 16
)
KEY_ONE_ADDRESS = int("7e5f4552091a69125d5dfcb7b8c2659029395bdf", 16)


def fixture(payload=b"abc", budgets=None):
    data = encoding.words(len(payload)) + payload + bytes((-len(payload)) % 32)
    return {
        **encoding.memory(data),
        **({"message_gas": list(budgets)} if budgets is not None else {}),
    }


def hash_step(name, input, result="v2", gas="100000"):
    option = "" if gas is None else f", gas {gas}"
    address = 2 if name == "sha256" else 3
    text = f"v710 = extcall static word i160 {address}, {input}{option}\n"
    text += "icall revert_if_zero<empty>, v710\n"
    if name == "ripemd160":
        return text + f"v711 = mload 0\n{result} = shl v711, 96\n"
    return text + f"{result} = mload 0\n"


def recovery_step(v=0, r=0, s=0, gas="3000"):
    option = "" if gas is None else f", gas {gas}"
    return (
        "v700 = mload 64\nv701 = inttoptr i256 v700 to ptr<memory>\n"
        f"mstore v701, 0\nv702 = ptradd v701, 32\nmstore v702, {v}\n"
        f"v703 = ptradd v701, 64\nmstore v703, {r}\nv704 = ptradd v701, 96\nmstore v704, {s}\n"
        "v705 = make_memory_slice v701, 128\n"
        f"v706 = extcall static word i160 1, v705{option}\n"
        "icall revert_if_zero<empty>, v706\nv0 = mload 0\n"
    )


def hash_source(name="sha256", sliced=False, gas="100000"):
    source = f"v0 = inttoptr i256 {160 if sliced else 128} to ptr<memory>\n"
    if sliced:
        source += "v1 = make_memory_slice v0, 3\n"
    source += (
        "jump bb1\nbb1:\n"
        + hash_step(name, "v1" if sliced else "v0", gas=gas)
        + "ret v2"
    )
    return state.module(source)


class MirPrecompileBuiltinTests(unittest.TestCase):
    pair = state.MirStateTests.pair
    lower = abi.MirAbiTests.lower

    def test_hashes_against_real_cfg_pairs_and_known_digests(self):
        for name in ("sha256", "ripemd160"):
            for sliced in (False, True):
                with self.subTest(name=name, sliced=sliced):
                    source = hash_source(name, sliced)
                    self.pair(
                        source, self.lower(source, "cfg-simplify"), fixture=fixture()
                    )
                    digest = int.from_bytes(hashlib.new(name, b"abc").digest(), "big")
                    if name == "ripemd160":
                        digest <<= 96
                    self.pair(
                        source,
                        source.replace("ret v2", f"ret {digest}"),
                        fixture=fixture(),
                    )
        source = hash_source()
        self.pair(
            source, source.replace("ret v2", "ret 0"), fixture=fixture(), status=1
        )

    def test_empty_and_zero_length_slice(self):
        source = hash_source()
        expected = int.from_bytes(hashlib.sha256(b"").digest(), "big")
        self.pair(
            source, source.replace("ret v2", f"ret {expected}"), fixture=fixture(b"")
        )
        source = state.module(
            f"v0 = inttoptr i256 {2**256 - 1} to ptr<memory>\n"
            "v1 = make_memory_slice v0, 0\n" + hash_step("sha256", "v1") + "ret v2"
        )
        self.pair(
            source, source.replace("ret v2", f"ret {expected}"), fixture=fixture(b"")
        )

    def test_recovery_valid_invalid_and_raw_scratch(self):
        for v, r, s, expected in (
            (0, 0, 0, 0),
            (27, GENERATOR_X, GENERATOR_X, KEY_ONE_ADDRESS),
        ):
            with self.subTest(v=v):
                source = state.module(
                    "jump bb1\nbb1:\n" + recovery_step(v, r, s) + "ret v0"
                )
                initial = fixture(b"", (5000,))
                if v == 0:
                    data = bytearray.fromhex(
                        initial["memory"]["data"].removeprefix("0x")
                    )
                    data[:32] = bytes([255]) * 32
                    initial["memory"]["data"] = "0x" + data.hex()
                self.pair(source, self.lower(source, "cfg-simplify"), fixture=initial)
                self.pair(
                    source, source.replace("ret v0", f"ret {expected}"), fixture=initial
                )

    def test_precompile_failure_bubbles_and_missing_budget_rejects(self):
        for source in (
            hash_source(gas="0"),
            hash_source("ripemd160", gas="0"),
            state.module(recovery_step(gas="0") + "ret v0"),
        ):
            with self.subTest(source=source):
                self.pair(source, state.module("revert 0, 0"), fixture=fixture())
                result, _ = self.pair(
                    source.replace(", gas 0", ""),
                    source.replace(", gas 0", ""),
                    fixture=fixture(budgets=()),
                    status=2,
                )
                self.assertIn("missing MIR caller gas budget", result.stderr)

    def test_callers_share_ordered_message_budgets(self):
        source = (
            state.module(
                "v0 = inttoptr i256 128 to ptr<memory>\nicall @helper, v0\n"
                + hash_step("sha256", "v0", "v1", gas=None)
                + "ret v1"
            )
            + "fn @helper(arg0: ptr<memory>) {\nbb0:\n"
            + hash_step("sha256", "arg0", "v0", gas=None)
            + "ret\n}\n"
        )
        self.pair(
            source,
            self.lower(source, "cfg-simplify"),
            fixture=fixture(budgets=(100000, 100000)),
        )
        result, _ = self.pair(
            source, source, fixture=fixture(budgets=(100000,)), status=2
        )
        self.assertIn("missing MIR caller gas budget", result.stderr)

    def test_budget_limits_are_not_verified(self):
        source = hash_source()
        for budgets in ((30000001,), tuple(range(129))):
            with self.subTest(budgets=budgets):
                self.pair(source, source, fixture=fixture(budgets=budgets), status=2)
        result, _ = self.pair(
            source, source, fixture={**fixture(), "builtin_gas": [100000]}, status=2
        )
        self.assertIn("belongs to removed MIR builtins", result.stderr)

    def test_message_gas_stays_live_at_terminal_boundaries(self):
        initial = fixture(b"", (5000,))
        # Empty identity input has no memory, returndata or account effects.
        prefix = "v0 = extcall static i160 4, gas 3000\n"
        result, _ = self.pair(
            state.module(prefix + "ret 7"),
            state.module("ret 7"),
            fixture=initial,
            status=1,
        )
        self.assertIn(
            "Remaining caller gas at a message boundary changed", result.stdout
        )
        self.pair(
            state.module(prefix + "stop"),
            state.module("stop"),
            fixture=initial,
            status=1,
        )

    def test_real_example_and_broken_digest(self):
        directory = ROOT / "examples/mir/precompile-builtins"
        before = (directory / "before.mir").read_text()
        after = self.lower(before, "cfg-simplify")
        self.assertEqual(
            after.splitlines()[1:],
            (directory / "after.mir").read_text().splitlines()[1:],
        )
        initial = json.loads((directory / "world.json").read_text())
        self.pair(before, after, fixture=initial)
        self.pair(
            before, (directory / "broken.mir").read_text(), fixture=initial, status=1
        )


if __name__ == "__main__":
    unittest.main()
