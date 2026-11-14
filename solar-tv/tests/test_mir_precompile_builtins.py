"""Precompile-backed MIR builtins: crypto, exact memory, gas failure and replay."""

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


def precompile(name, operand, result, label=2):
    """A precompile call as the frontend emits it: a static word call, a branch that bubbles a
    failure's returndata, then the scratch word (RIPEMD-160 left-aligned)."""
    address = {"ecrecover": 1, "sha256": 2, "ripemd160": 3}[name]
    ok, fail = f"bb{label + 1}", f"bb{label}"
    load = (
        f"v90 = mload 0\n{result} = shl 96, v90"
        if name == "ripemd160"
        else f"{result} = mload 0"
    )
    return (
        f"v91 = extcall static word i160 {address}, {operand}\n"
        f"jumpi v91, {ok}, {fail}\n{fail}:\nrevert_returndata\n{ok}:\n{load}\n"
    )


def recover(words, result, label=2):
    """ecrecover stages its four words at the free memory pointer without reserving it."""
    staged = "v80 = fmp\nv81 = ptrtoint ptr<memory> v80 to i256\n"
    for index, word in enumerate(words):
        staged += f"v8{index + 2} = add v81, {32 * index}\nmstore v8{index + 2}, {word}\n"
    staged += "v86 = make_memory_slice v80, 128\n"
    return staged + precompile("ecrecover", "v86", result, label)


def fixture(payload=b"abc", budgets=(100000,)):
    data = encoding.words(len(payload)) + payload + bytes((-len(payload)) % 32)
    return {**encoding.memory(data), "builtin_gas": list(budgets)}


def hash_source(name="sha256", sliced=False):
    source = f"v0 = inttoptr i256 {160 if sliced else 128} to ptr<memory>\n"
    if sliced:
        source += "v1 = make_memory_slice v0, 3\n"
    source += "jump bb1\nbb1:\n" + precompile(name, "v1" if sliced else "v0", "v2") + "ret v2"
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
            "v1 = make_memory_slice v0, 0\n" + precompile("sha256", "v1", "v2") + "ret v2"
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
                    "jump bb1\nbb1:\n" + recover((0, v, r, s), "v0") + "ret v0"
                )
                initial = fixture(b"", (3000,))
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
            hash_source(),
            hash_source("ripemd160"),
            state.module(recover((0, 0, 0, 0), "v0") + "ret v0"),
        ):
            with self.subTest(source=source):
                self.pair(
                    source, state.module("revert 0, 0"), fixture=fixture(budgets=(0,))
                )
                result, _ = self.pair(
                    source, source, fixture=fixture(budgets=()), status=2
                )
                self.assertIn("builtin gas budget", result.stderr)

    def test_callers_share_ordered_builtin_budgets(self):
        source = (
            state.module(
                "v0 = inttoptr i256 128 to ptr<memory>\nicall @helper, v0\n"
                + precompile("sha256", "v0", "v1")
                + "ret v1"
            )
            + "fn @helper(arg0: ptr<memory>) {\nbb0:\n"
            + precompile("sha256", "arg0", "v0", 1)
            + "ret\n}\n"
        )
        self.pair(
            source, state.module("revert 0, 0"), fixture=fixture(budgets=(100000, 0))
        )
        result, _ = self.pair(
            source, source, fixture=fixture(budgets=(100000,)), status=2
        )
        self.assertIn("builtin gas budget", result.stderr)

    def test_budget_limits_are_not_verified(self):
        source = hash_source()
        for budgets in ((30000001,), tuple(range(129))):
            with self.subTest(budgets=budgets):
                self.pair(source, source, fixture=fixture(budgets=budgets), status=2)

    def test_budget_consumption_is_live_only_at_internal_returns(self):
        initial = fixture(b"", (3000,))
        data = bytearray(512)
        data[95] = 128
        initial["memory"] = {"size": 512, "data": "0x" + data.hex()}
        prefix = recover((0, 0, 0, 0), "v0")
        result, _ = self.pair(
            state.module(prefix + "ret 7"),
            state.module("ret 7"),
            fixture=initial,
            status=1,
        )
        self.assertIn("Boundary gas budget consumption changed", result.stdout)
        self.pair(state.module(prefix + "stop"), state.module("stop"), fixture=initial)

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
