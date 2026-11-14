"""Direct builtin effects versus real conversion, including failing calls."""

import unittest

import test_mir_abi as abi
import test_mir_address_calls as calls
import test_mir_state as state
import test_mir_storage_bytes as storage

MASK = 2**256 - 1
PIPELINE = "lower-builtins,lower-checks,lower-slices,lower-memory-objects"


def send(kind, amount=0):
    """send and transfer as the frontend emits them: an extcall with the stipend for zero value,
    and for transfer a branch that bubbles a failure's returndata."""
    body = (
        f"v0 = trunc i256 {calls.ADDRESS} to i160\n"
        f"v2 = eq {amount}, 0\nv3 = select v2, 2300, 0\n"
        f"v1 = extcall v0, gas v3, value {amount}\n"
    )
    if kind == "send":
        body += "ret v1"
    else:
        body += "jumpi v1, bb2, bb1\nbb1:\nrevert_returndata\nbb2:\nret 7"
    return state.module(body)


class MirBuiltinTests(unittest.TestCase):
    pair = state.MirStateTests.pair
    lower = abi.MirAbiTests.lower
    lowered_pair = abi.MirAbiTests.lowered_pair

    def test_checked_modular_arithmetic(self):
        for op in ("addmod", "mulmod"):
            for a, b, modulus in ((7, 9, 0), (MASK, MASK, MASK - 1), (7, 9, 1)):
                with self.subTest(op=op, a=a, b=b, modulus=modulus):
                    source = state.module(
                        f"v0 = icall checked_{op}<>, {a}, {b}, {modulus}\nret v0"
                    )
                    self.lowered_pair(source, PIPELINE)
            source = state.module(f"v0 = icall checked_{op}<>, 7, 9, 0\nret v0")
            self.pair(source, state.module(f"v0 = {op} 7, 9, 0\nret v0"), status=1)

    def test_namespace_hash_retains_scratch_and_source_reads(self):
        for payload in (b"", b"example.main"):
            with self.subTest(payload=payload):
                initial = storage.fixture()
                data = bytearray.fromhex(initial["memory"]["data"].removeprefix("0x"))
                data.extend(bytes(224 - len(data)))
                data[128:160] = len(payload).to_bytes(32, "big")
                data[160 : 160 + len(payload)] = payload
                initial["memory"] = {"size": 224, "data": "0x" + data.hex()}
                source = state.module(
                    "v0 = inttoptr i256 128 to ptr<memory>\nv1 = icall erc7201<>, v0\nret v1"
                )
                self.lowered_pair(source, PIPELINE, initial)
                self.pair(
                    source, source.replace("ret v1", "ret 0"), fixture=initial, status=1
                )

    def test_stipend_calls_success_failure_and_revert_bytes(self):
        for kind in ("send", "transfer"):
            for code in ("00", "60ab5f5360015ffd", "600160005500", "5b5f56"):
                with self.subTest(kind=kind, code=code):
                    self.lowered_pair(
                        send(kind), PIPELINE, calls.fixture(code, fuel=2048)
                    )
        source = send("send")
        self.pair(
            source,
            source.replace("ret v1", "ret 1"),
            fixture=calls.fixture("fe"),
            status=1,
        )

    def test_value_static_and_insufficient_balance(self):
        accounts = [
            {"address": calls.ADDRESS, "nonce": 1, "code": "0x00"},
            {"address": 65536, "balance": 20},
        ]
        for kind in ("send", "transfer"):
            for amount, static in ((7, False), (21, False), (7, True), (0, True)):
                with self.subTest(kind=kind, amount=amount, static=static):
                    self.lowered_pair(
                        send(kind, amount),
                        PIPELINE,
                        calls.fixture(accounts=accounts, static=static),
                    )

    def test_selfdestruct_forks_static_and_prior_writes(self):
        initial = calls.fixture(
            accounts=[{"address": 65536, "balance": 17, "storage": {"0": 7}}]
        )
        for version in ("istanbul", "cancun", "osaka"):
            for beneficiary in (65536, calls.ADDRESS):
                with self.subTest(version=version, beneficiary=beneficiary):
                    source = state.module(
                        f"jump bb1\nbb1:\nsstore 0, 9\nselfdestruct {beneficiary}"
                    )
                    after = self.lower(source, "cfg-simplify")
                    self.pair(source, after, fixture=initial, version=version)
                    self.pair(
                        source,
                        state.module("sstore 0, 9\nstop"),
                        fixture=initial,
                        version=version,
                        status=0
                        if version != "istanbul" and beneficiary == 65536
                        else 1,
                    )
        self.pair(
            state.module("selfdestruct 1"),
            state.module("invalid"),
            fixture={**initial, "static": True},
        )


if __name__ == "__main__":
    unittest.main()
