"""Physical aggregate/slice return buffers and independent type-lowering replay."""

import json
import sys
import tempfile
import unittest
from pathlib import Path

import test_mir_abi as abi
import test_mir_return_buffers as buffers
import test_mir_state as state
from test_cli import ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import mir_state, native, world
from validator.common import Program, Unsupported

PIPELINE = "lower-structs,lower-frame-slots,lower-slices,lower-memory-objects"


def module(body, helper, *, location="memory", signature="fn @f() -> i256"):
    return (
        "@module AggregateReturns\n@types\nstruct0: {i8, i1}\n"
        f"struct1: {{struct0, slice<{location}>, i256}}\n"
        + state.module(body, signature).split("\n", 1)[1]
        + helper
    )


def make(location="memory", last="19"):
    return f"""fn @make() -> struct1 {{
bb0:
 v0 = insert_value struct0, undef struct0, 0, i8 255
 v1 = insert_value struct0, v0, 1, true
 v2 = inttoptr i256 128 to ptr<{location}>
 v3 = make_{location}_slice v2, 7
 v4 = insert_value struct1, undef struct1, 0, v1
 v5 = insert_value struct1, v4, 1, v3
 v6 = insert_value struct1, v5, 2, {last}
 ret v6
}}
"""


def consume(location="memory", callee="make"):
    return f"""v0 = icall @{callee}
v1 = extract_value struct1, v0, 0
v2 = extract_value struct0, v1, 0
v3 = zext i8 v2 to i256
v4 = extract_value struct0, v1, 1
v5 = zext i1 v4 to i256
v6 = extract_value struct1, v0, 1
v7 = extract_value slice<{location}>, v6, 0
v8 = ptrtoint ptr<{location}> v7 to i256
v9 = mul v8, 2
v10 = extract_value slice<{location}>, v6, 1
v11 = extract_value struct1, v0, 2
v12 = add v3, v5
v13 = add v12, v9
v14 = add v13, v10
v15 = add v14, v11
ret v15"""


def publication(address, last=19):
    return (
        f"mstore {address + 32}, 1\nmstore {address + 64}, 128\n"
        f"mstore {address + 96}, 7\nmstore {address + 128}, {last}\n"
        f"mstore 32, {address}\n"
    )


class MirAggregateReturnTests(unittest.TestCase):
    pair = state.MirStateTests.pair
    lower = abi.MirAbiTests.lower

    def test_real_nested_aggregate_return_lowering(self):
        for location in ("memory", "calldata", "returndata"):
            with self.subTest(location=location):
                before = module(consume(location), make(location), location=location)
                initial = buffers.fixture(make={"return_buffer": 256})
                after = self.lower(before, PIPELINE)
                self.assertNotEqual(before.splitlines()[1:], after.splitlines()[1:])
                self.pair(before, after, fixture=initial)
                self.pair(
                    before, state.module(publication(256) + "ret 538"), fixture=initial
                )
                self.pair(
                    before,
                    before.replace("i8 255", "i8 254"),
                    fixture=initial,
                    status=1,
                )

    def test_real_slice_return_lowering(self):
        for location in ("memory", "calldata", "returndata"):
            with self.subTest(location=location):
                helper = f"fn @slice() -> slice<{location}> {{\nbb0:\nv0 = inttoptr i256 128 to ptr<{location}>\nv1 = make_{location}_slice v0, 7\nret v1\n}}\n"
                body = f"v0 = icall @slice\nv1 = extract_value slice<{location}>, v0, 0\nv2 = ptrtoint ptr<{location}> v1 to i256\nv3 = extract_value slice<{location}>, v0, 1\nv4 = sub v2, v3\nret v4"
                before = state.module(body) + helper
                initial = buffers.fixture(slice={"return_buffer": 256})
                self.pair(before, self.lower(before, "lower-slices"), fixture=initial)
                self.pair(
                    before,
                    state.module("mstore 288, 7\nmstore 32, 256\nret 121"),
                    fixture=initial,
                )

    def test_mixed_expanded_components_use_leaf_offsets(self):
        helper = "fn @mixed() [return_abi=[i8,slice<memory>,i1]] {\nbb0:\nv0 = inttoptr i256 128 to ptr<memory>\nv1 = make_memory_slice v0, 7\nret i8 255, v1, true\n}\n"
        before = (
            state.module("v0 = icall @mixed\nv1 = zext i8 v0 to i256\nret v1") + helper
        )
        initial = buffers.fixture(mixed={"return_buffer": 256})
        self.pair(before, self.lower(before, "lower-slices"), fixture=initial)
        self.pair(
            before,
            state.module(
                "mstore 288, 128\nmstore 320, 7\nmstore 352, 1\nmstore 32, 256\nret 255"
            ),
            fixture=initial,
        )

    def test_tail_calls_keep_the_original_aggregate_return_owner(self):
        relay = "fn @relay() -> struct1 {\nbb0:\ntail_call @make\n}\n"
        before = module(consume(callee="relay"), relay + make())
        initial = buffers.fixture(
            relay={"return_buffer": 256}, make={"return_buffer": 512}
        )
        self.pair(before, self.lower(before, PIPELINE), fixture=initial)
        self.pair(before, state.module(publication(256) + "ret 538"), fixture=initial)
        self.pair(
            before,
            state.module(publication(512) + "ret 538"),
            fixture=initial,
            status=1,
        )

    def test_aliased_return_values_are_read_before_publication(self):
        helper = make(last="v7").replace(" v6 =", " v7 = mload 288\n v6 =")
        before = module("mstore 288, 77\n" + consume(), helper)
        initial = buffers.fixture(make={"return_buffer": 256})
        self.pair(before, self.lower(before, PIPELINE), fixture=initial)
        self.pair(
            before,
            state.module("mstore 288, 77\n" + publication(256, 77) + "ret 596"),
            fixture=initial,
        )

    def test_nested_calls_snapshot_overlapping_return_buffers(self):
        relay = "fn @relay() -> struct1 {\nbb0:\nv0 = icall @make\nret v0\n}\n"
        before = module(consume(callee="relay"), relay + make())
        initial = buffers.fixture(
            relay={"return_buffer": 256}, make={"return_buffer": 288}
        )
        self.pair(before, self.lower(before, PIPELINE), fixture=initial)
        self.pair(
            before,
            state.module(publication(288) + publication(256) + "ret 538"),
            fixture=initial,
        )

    def test_logical_and_physical_discarded_results_are_explicit(self):
        before = module("icall @make\nret 7", make())
        self.pair(before, state.module("ret 7"))
        initial = buffers.fixture(make={"return_buffer": 256})
        self.pair(before, state.module(publication(256) + "ret 7"), fixture=initial)
        self.pair(before, self.lower(before, PIPELINE), fixture=initial)
        self.pair(before, state.module("ret 7"), fixture=initial, status=1)

    def test_undefined_first_word_is_not_invented_or_stored(self):
        helper = "fn @make() -> struct0 {\nbb0:\nv0 = insert_value struct0, undef struct0, 1, true\nret v0\n}\n"
        before = module("mstore 256, 17\nicall @make\nv0 = mload 288\nret v0", helper)
        initial = buffers.fixture(make={"return_buffer": 256})
        self.pair(before, self.lower(before, "lower-structs"), fixture=initial)
        self.pair(
            before,
            state.module("mstore 256, 17\nmstore 288, 1\nmstore 32, 256\nret 1"),
            fixture=initial,
        )
        # An undefined extra word would be stored and cannot be assigned arbitrary bits.
        invalid = before.replace("1, true", "0, i8 7")
        result, _ = self.pair(invalid, invalid, fixture=initial, status=2)
        self.assertIn("undefined MIR scalar", result.stderr)

    def test_outer_boundary_does_not_publish_a_buffer(self):
        source = module("tail_call @make", make(), signature="fn @f() -> struct1")
        initial = buffers.fixture(f={"return_buffer": 256}, make={"return_buffer": 512})
        self.pair(source, self.lower(source, PIPELINE), fixture=initial)

    def test_empty_and_single_leaf_products_need_no_buffer_writes(self):
        for types, helper in (
            ("struct0: {}", "fn @make() -> struct0 {\nbb0:\nret undef struct0\n}\n"),
            (
                "struct0: {}\nstruct1: {struct0, i256}",
                "fn @make() -> struct1 {\nbb0:\nv0 = insert_value struct1, undef struct1, 1, 17\nret v0\n}\n",
            ),
        ):
            with self.subTest(types=types):
                before = (
                    "@module EmptyReturns\n@types\n"
                    + types
                    + "\n"
                    + state.module("icall @make\nret 7").split("\n", 1)[1]
                    + helper
                )
                initial = buffers.fixture(make={"return_buffer": 65536})
                self.pair(before, self.lower(before, "lower-structs"), fixture=initial)
                self.pair(before, state.module("ret 7"), fixture=initial)

    def test_returned_memory_slice_still_aliases_publication(self):
        helper = "fn @view() -> slice<memory> {\nbb0:\nv0 = inttoptr i256 288 to ptr<memory>\nv1 = make_memory_slice v0, 32\nret v1\n}\n"
        body = "mstore 288, 17\nv0 = icall @view\nv1 = extract_value slice<memory>, v0, 0\nv2 = mload v1\nret v2"
        before = state.module(body) + helper
        initial = buffers.fixture(view={"return_buffer": 256})
        self.pair(before, self.lower(before, "lower-slices"), fixture=initial)
        # The published length overwrites the viewed word; no object copy is made.
        expected = state.module(
            "mstore 288, 17\nmstore 288, 32\nmstore 32, 256\nret 32"
        )
        self.pair(before, expected, fixture=initial)
        self.pair(
            before, expected.replace("ret 32", "ret 17"), fixture=initial, status=1
        )

    def test_returned_calldata_slice_reads_its_original_bytes(self):
        helper = "fn @view() -> slice<calldata> {\nbb0:\nv0 = inttoptr i256 0 to ptr<calldata>\nv1 = make_calldata_slice v0, 32\nret v1\n}\n"
        body = "v0 = icall @view\nv1 = extract_value slice<calldata>, v0, 0\nv2 = calldataload v1\nret v2"
        before = state.module(body) + helper
        initial = {
            **buffers.fixture(view={"return_buffer": 256}),
            "calldata": "0x" + (42).to_bytes(32, "big").hex(),
        }
        self.pair(before, self.lower(before, "lower-slices"), fixture=initial)
        self.pair(
            before,
            state.module("mstore 288, 32\nmstore 32, 256\nret 42"),
            fixture=initial,
        )

    def test_returned_returndata_slice_tracks_later_calls(self):
        helper = "fn @view() -> slice<returndata> {\nbb0:\nv0 = inttoptr i256 0 to ptr<returndata>\nv1 = make_returndata_slice v0, 32\nret v1\n}\n"
        body = """mstore 0, 42
v0 = staticcall 100000, 4, 0, 32, 0, 0
v1 = icall @view
mstore 0, 99
v2 = staticcall 100000, 4, 0, 32, 0, 0
v3 = extract_value slice<returndata>, v1, 0
v4 = ptrtoint ptr<returndata> v3 to i256
v5 = extract_value slice<returndata>, v1, 1
returndatacopy 128, v4, v5
v6 = mload 128
ret v6"""
        before = state.module(body) + helper
        initial = buffers.fixture(view={"return_buffer": 256})
        after = self.lower(before, "lower-slices")
        self.pair(before, after, fixture=initial)
        # Retaining the old slice retains a range, not the earlier child output.
        self.pair(before, after.replace("ret v6", "ret 42"), fixture=initial, status=1)

    def test_symbolic_return_words_survive_real_lowering(self):
        before = """@module SymbolicReturns
@types
struct0: {i256, i256}
fn @f(arg0: i256) -> i256 {
bb0:
 v0 = icall @pair, arg0
 v1 = extract_value struct0, v0, 1
 ret v1
}
fn @pair(arg0: i256) -> struct0 {
bb0:
 v0 = add arg0, 1
 v1 = insert_value struct0, undef struct0, 0, arg0
 v2 = insert_value struct0, v1, 1, v0
 ret v2
}
"""
        initial = buffers.fixture(pair={"return_buffer": 256})
        self.pair(before, self.lower(before, PIPELINE), fixture=initial)
        self.pair(
            before,
            before.replace("add arg0, 1", "add arg0, 2"),
            fixture=initial,
            status=1,
        )

    def test_replay_bounds_callee_return_types_independently(self):
        types = "struct0: {i256}\n" + "\n".join(
            f"struct{i}: {{struct{i - 1}}}" for i in range(1, 40)
        )
        source = (
            "@module DeepReturn\n@types\n"
            + types
            + "\n"
            + state.module("icall @deep\nret 7").split("\n", 1)[1]
            + "fn @deep() -> struct39 {\nbb0:\nret undef struct39\n}\n"
        )
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "world.json"
            path.write_text(
                json.dumps(
                    {"version": 1, **buffers.fixture(deep={"return_buffer": 256})}
                )
            )
            initial = world.read(path, SOLAR, None, kind="mir")
        program = Program(
            ("world-entry", "mir"),
            [],
            "MIR.State.Run",
            "",
            canonical_ir=source,
            selector="f",
            evm_version="cancun",
            world=initial,
        )
        with self.assertRaisesRegex(Unsupported, "aggregate type limit"):
            native.replay(program, program, [], executable=state.REPLAY, timeout=10)

    def test_importer_requires_return_owner_identity(self):
        for identity in (None, "", 7):
            with self.subTest(identity=identity):
                function = {
                    "id": 0,
                    "params": [],
                    "returns": [],
                    "blocks": [],
                    "return_buffer": identity,
                }
                with self.assertRaisesRegex(Unsupported, "return-owner identity"):
                    mir_state.function_body(function, {})

    def test_documented_return_buffer_pair(self):
        folder = ROOT / "examples/mir/aggregate-returns"
        before = (folder / "before.mir").read_text()
        after = (folder / "after.mir").read_text()
        initial = json.loads((folder / "world.json").read_text())
        actual = self.lower(before, PIPELINE)
        self.assertEqual(actual.splitlines()[1:], after.splitlines())
        self.pair(before, after, fixture=initial)
        result, record = self.pair(
            before, (folder / "broken.mir").read_text(), fixture=initial, status=1
        )
        self.assertEqual(
            record["witness"]["observations"], record["witness"]["native_observations"]
        )
        self.assertIn("counterexample", result.stdout)
