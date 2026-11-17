"""Typed address calls and returndata objects versus real builtin conversion."""

import unittest

import test_mir_state as state
import test_mir_storage_bytes as storage
from test_mir_messages import ADDRESS, RETURN_42


def fixture(code=RETURN_42, **extra):
    return {
        **storage.fixture(),
        "accounts": [{"address": ADDRESS, "nonce": 1, "code": "0x" + code}],
        **extra,
    }


def source(
    kind="call", *, sliced=False, gas="100000", amount=None, object_result=False
):
    prefix = (
        f"v0 = trunc i256 {ADDRESS} to i160\n"
        "mstore 128, 3\n"
        f"mstore 160, {int.from_bytes(b'abc' + bytes(29), 'big')}\n"
        f"v1 = inttoptr i256 {160 if sliced else 128} to ptr<memory>\n"
    )
    if sliced:
        prefix += "v2 = make_memory_slice v1, 3\n"
    option = "" if gas is None else f", gas {gas}"
    if amount is not None:
        option += f", value {amount}"
    policy = {"call": "", "staticcall": " static", "delegatecall": " delegate"}[kind]
    body = prefix + f"v3 = extcall{policy} v0, {'v2' if sliced else 'v1'}{option}\n"
    if object_result:
        body += "v4 = icall returndata_bytes<>\nv5 = ptrtoint ptr<memory> v4 to i256\nret v5"
    else:
        body += "v4 = zext i1 v3 to i256\nv5 = returndatasize\nv6 = add v4, v5\nret v6"
    return state.module(body)


class MirAddressCallsTests(unittest.TestCase):
    pair = state.MirStateTests.pair
    lowered_pair = storage.MirStorageBytesTests.lowered_pair

    def test_real_all_call_kinds_object_and_slice_inputs(self):
        # The identity precompile consumes the complete input synchronously.
        for kind in ("call", "staticcall", "delegatecall"):
            for sliced in (False, True):
                with self.subTest(kind=kind, sliced=sliced):
                    self.lowered_pair(
                        source(kind, sliced=sliced).replace(str(ADDRESS), "4"),
                        fixture(),
                    )

    def test_failed_call_preserves_revert_data_and_returns_false(self):
        self.lowered_pair(source(object_result=True), fixture("60ab5f5360015ffd"))
        self.pair(
            source(),
            source().replace("gas 100000", "gas 0"),
            fixture=fixture(),
            status=1,
        )

    def test_call_value_and_static_violations(self):
        world = fixture(
            accounts=[
                {"address": ADDRESS, "nonce": 1, "code": "0x" + RETURN_42},
                {"address": 65536, "balance": 20},
            ]
        )
        self.lowered_pair(source(amount=7), world)
        self.lowered_pair(source(amount=7), {**world, "static": True})
        self.pair(source(amount=7), source(amount=8), fixture=world, status=1)

    def test_returndata_object_keeps_dirty_tail_and_does_not_consume_buffer(self):
        text = source(object_result=True).replace(
            "v4 = icall returndata_bytes<>",
            f"mstore 160, {2**256 - 1}\nv4 = icall returndata_bytes<>",
        )
        self.lowered_pair(text, fixture("602a5f5360015ff3"))
        self.pair(
            text,
            text.replace("ret v5", "ret 160"),
            fixture=fixture("602a5f5360015ff3"),
            status=1,
        )
        empty = state.module(
            "v0 = icall returndata_bytes<>\nv1 = ptrtoint ptr<memory> v0 to i256\nret v1"
        )
        self.lowered_pair(empty, fixture())
        self.pair(
            empty,
            state.module("mstore 64, 160\nmstore 128, 0\nret 128"),
            fixture=fixture(),
        )

    def test_explicit_zero_and_failed_message_replace_old_return_data(self):
        prefix = source().replace(
            "v4 = zext i1 v3 to i256\nv5 = returndatasize\nv6 = add v4, v5\nret v6", ""
        )
        body = (
            prefix.replace("}\n", "")
            + "v7 = extcall v0, v1, gas 0\nv8 = icall returndata_bytes<>\nv9 = ptrtoint ptr<memory> v8 to i256\nret v9\n}\n"
        )
        self.lowered_pair(body, fixture())
        self.lowered_pair(source(gas="0"), fixture())

    def test_missing_gas_and_unsupported_fork_are_explicit(self):
        text = source(gas=None)
        result, _ = self.pair(text, text, fixture=fixture(), status=2)
        self.assertIn("requires caller pricing", result.stderr)
        text = source()
        result, _ = self.pair(
            text, text, fixture=fixture(), status=2, version="amsterdam"
        )
        self.assertIn("Amsterdam MIR message gas", result.stderr)

    def test_absent_input_does_not_read_an_object_header(self):
        text = state.module(
            "v0 = extcall i160 4, gas 100000\n"
            "v1 = zext i1 v0 to i256\nv2 = msize\nv3 = add v1, v2\nret v3"
        )
        initial = {"memory": {"data": "0x", "size": 0}}
        self.lowered_pair(text, initial)
        self.pair(text, text.replace("ret v3", "ret 1"), fixture=initial)

    def test_word_output_clears_bytes_beyond_short_returndata(self):
        text = state.module(
            f"v0 = extcall word i160 {ADDRESS}, gas 100000\nv1 = mload 0\nret v1"
        )
        for code in ("602a5f5360015ff3", "602a5f5360015ffd", "00"):
            with self.subTest(code=code):
                initial = fixture(code, memory={"data": "0x" + "ff" * 32, "size": 32})
                self.lowered_pair(text, initial)
        initial = fixture(
            "602a5f5360015ff3", memory={"data": "0x" + "ff" * 32, "size": 32}
        )
        self.pair(
            text, text.replace("extcall word", "extcall"), fixture=initial, status=1
        )

    def test_zero_length_slice_ignores_its_out_of_range_pointer(self):
        text = state.module(
            f"v0 = trunc i256 {ADDRESS} to i160\n"
            f"v1 = inttoptr i256 {2**256 - 1} to ptr<memory>\n"
            "v2 = make_memory_slice v1, 0\n"
            "v3 = extcall static v0, v2, gas 100000\n"
            "v4 = returndatasize\nret v4"
        )
        self.lowered_pair(text, fixture())

    def test_pre_byzantium_world_is_not_claimed_as_verified(self):
        before = state.module(
            "v0 = extcall static word i160 4, gas 100000\nv1 = mload 0\nret v1"
        )
        after = state.module(
            "v0 = call 100000, 4, 0, 0, 0, 0, 32\nv1 = mload 0\nret v1"
        )
        result, _ = self.pair(
            before,
            after,
            fixture={"memory": {"data": "0x" + "ff" * 32, "size": 32}},
            version="homestead",
            status=2,
        )
        self.assertIn("explicit-world execution supports Istanbul", result.stderr)

    def test_word_output_reads_header_before_clearing_input_aliases(self):
        initial = {
            "memory": {
                "data": "0x" + (1).to_bytes(32, "big").hex() + "aa" + "ff" * 31,
                "size": 64,
            }
        }
        for sliced in (False, True):
            with self.subTest(sliced=sliced):
                text = state.module(
                    f"v0 = inttoptr i256 {31 if sliced else 0} to ptr<memory>\n"
                    + ("v1 = make_memory_slice v0, 1\n" if sliced else "")
                    + f"v2 = extcall word i160 4, {'v1' if sliced else 'v0'}, gas 100000\n"
                    + "v3 = mload 0\nret v3"
                )
                self.lowered_pair(text, initial)
                self.pair(
                    text,
                    text.replace("ret v3", f"ret {0 if sliced else 0xAA << 248}"),
                    fixture=initial,
                )

    def test_constant_hash_callees_skip_clear_even_when_the_call_fails(self):
        initial = {"memory": {"data": "0x" + "ff" * 32, "size": 32}}
        for address in (2, 3):
            with self.subTest(address=address):
                text = state.module(
                    f"v0 = extcall static word i160 {address}, gas 0\nv1 = mload 0\nret v1"
                )
                self.lowered_pair(text, initial)
                self.pair(
                    text, text.replace("ret v1", f"ret {2**256 - 1}"), fixture=initial
                )
