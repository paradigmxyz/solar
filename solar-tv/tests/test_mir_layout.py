"""Placement/link windows and GAS sampling never invent unresolved inputs."""

import json
import sys
import tempfile
import unittest
from pathlib import Path

import test_mir_state as state
from test_cli import ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import native, world
from validator.common import Program, Unsupported


def fixture(**layout):
    return {
        "mir_layout": layout,
        "memory": {
            "data": "0x" + (bytes(64) + (768).to_bytes(32, "big")).hex(),
            "size": 192,
        },
    }


def declarations(source, preamble):
    return source.replace("@module State\n", "@module State\n" + preamble + "\n")


class MirLayoutTests(unittest.TestCase):
    pair = state.MirStateTests.pair

    def test_heap_and_frame_queries_do_not_initialize_memory(self):
        before = state.module(
            "v0 = heap_floor\nv1 = internal_frame_addr 64\nv2 = ptrtoint ptr<memory> v1 to i256\nv3 = add v0, v2\nv4 = mload 64\nv5 = add v3, v4\nret v5"
        )
        world = fixture(functions={"f": {"heap_floor": 512, "frame_base": 256}})
        self.pair(
            before,
            state.module("v0 = mload 64\nv1 = add v0, 832\nret v1"),
            fixture=world,
        )
        self.pair(before, before.replace("addr 64", "addr 65"), fixture=world, status=1)
        result, _ = self.pair(before, before, status=2)
        self.assertIn("missing", result.stderr)

    def test_constructor_range_refers_to_initial_memory(self):
        text = state.module(
            "v0 = constructor_args_base\nv1 = constructor_args_end\nv2 = sub v1, v0\nret v2"
        )
        text = text.replace("-> i256 {", "-> i256 [constructor] {")
        world = fixture(constructor={"offset": 128, "size": 64})
        self.pair(
            text,
            state.module("ret 64").replace("-> i256 {", "-> i256 [constructor] {"),
            fixture=world,
        )
        self.pair(
            text,
            state.module("ret 32").replace("-> i256 {", "-> i256 [constructor] {"),
            fixture=world,
            status=1,
        )

    def test_linked_data_size_addend_and_alignment(self):
        prefix = '@data\n child_0: creation_code "child.sol:Child"'
        world = fixture(code="0x00010203040506", data={"0": {"offset": 2, "size": 3}})
        for suffix, result in [
            ("", 3),
            (", 31, aligned", 32),
            (", 18446744073709551615", 2**64 + 2),
        ]:
            with self.subTest(suffix=suffix):
                before = declarations(
                    state.module(f"v0 = datasize child_0{suffix}\nret v0"), prefix
                )
                after = declarations(state.module(f"ret {result}"), prefix)
                self.pair(before, after, fixture=world)
        self.pair(
            before, after.replace(str(result), str(result + 1)), fixture=world, status=1
        )

    def test_datasize_requires_a_deferred_declaration(self):
        prefix = '@data\n payload_0: hex"010203"'
        source = declarations(state.module("v0 = datasize payload_0\nret v0"), prefix)
        expected = declarations(state.module("ret 3"), prefix)
        result, _ = self.pair(source, expected, status=2)
        self.assertIn("datasize requires deferred data", result.stderr)

    def test_code_size_copy_padding_and_empty_ranges(self):
        world = fixture(code="0x01020304")
        self.pair(
            state.module("v0 = codesize\nret v0"), state.module("ret 4"), fixture=world
        )
        self.pair(
            state.module("codecopy 0, 2, 4\nv0 = mload 0\nret v0"),
            state.module(f"mstore 0, {0x0304 << 240}\nv0 = mload 0\nret v0"),
            fixture=world,
        )
        self.pair(
            state.module("codecopy 0, 2, 4\nret 0"),
            state.module("codecopy 0, 3, 4\nret 0"),
            fixture=world,
            status=1,
        )
        self.pair(
            state.module(f"codecopy {2**256 - 1}, {2**256 - 1}, 0\nret 0"),
            state.module("ret 0"),
            fixture=world,
        )
        self.pair(
            state.module("v0 = codesize\nret v0"),
            state.module("ret 0"),
            fixture=fixture(code="0x"),
        )

    def test_immutable_staging_alias_and_runtime_relocation(self):
        prefix = "@immutables\n initial: uint256"
        before = declarations(
            state.module(
                "storeimmutable initial, 17\nmstore 256, 19\nv0 = loadimmutable initial\nret v0"
            ),
            prefix,
        )
        after = declarations(
            state.module("mstore 256, 17\nmstore 256, 19\nv0 = mload 256\nret v0"),
            prefix,
        )
        self.pair(before, after, fixture=fixture(immutable_slots={"0": 256}))
        read = declarations(state.module("v0 = loadimmutable initial\nret v0"), prefix)
        self.pair(
            read,
            declarations(state.module("ret 17"), prefix),
            fixture=fixture(immutable_values={"0": "17"}),
        )
        self.pair(
            read,
            declarations(state.module("ret 18"), prefix),
            fixture=fixture(immutable_values={"0": "17"}),
            status=1,
        )
        result, _ = self.pair(
            before, before, fixture=fixture(immutable_values={"0": "17"}), status=2
        )
        self.assertIn("constructor staging", result.stderr)

    def test_library_relocation_retains_i160(self):
        prefix = '@libraries\n L_0: "A.sol:L"'
        before = declarations(
            state.module("v0 = library_address L_0\nv1 = zext i160 v0 to i256\nret v1"),
            prefix,
        )
        after = declarations(state.module("ret 17"), prefix)
        self.pair(before, after, fixture=fixture(libraries={"0": "17"}))
        self.pair(
            before,
            after.replace("ret 17", "ret 18"),
            fixture=fixture(libraries={"0": "17"}),
            status=1,
        )
        self.pair(
            before,
            after.replace("A.sol:L", "B.sol:L"),
            fixture=fixture(libraries={"0": "17"}),
            status=2,
        )

    def test_gas_opcode_window_charges_before_reading(self):
        before = state.module("v0 = gas\nret v0")
        for gas in (0, 1, 2, 100):
            with self.subTest(gas=gas):
                after = state.module("invalid" if gas < 2 else f"ret {gas - 2}")
                self.pair(before, after, fixture=fixture(gas_before=gas))
        self.pair(
            before, state.module("ret 100"), fixture=fixture(gas_before=100), status=1
        )
        self.pair(
            before, state.module("ret 0"), fixture=fixture(gas_before=1), status=1
        )
        complex_body = state.module("v0 = gas\nv1 = add v0, 1\nret v1")
        result, _ = self.pair(
            complex_body, complex_body, fixture=fixture(gas_before=100), status=2
        )
        self.assertIn("isolated opcode window", result.stderr)

    def test_invalid_contexts_and_missing_inputs_are_rejected(self):
        source = state.module("v0 = heap_floor\nret v0")
        for bad in (
            {"functions": {"f": {"heap_floor": 512}}},
            {"functions": {"f": {"frame_base": 129}}},
            {"constructor": {"offset": 192, "size": 1}},
            {"code": "0x01", "data": {"0": {"offset": 1, "size": 1}}},
            {"code": "0x01", "data": {"00": {"offset": 0, "size": 1}}},
            {"immutable_slots": {"0": 256}, "immutable_values": {"0": "1"}},
            {"libraries": {"0": str(2**160)}},
            {"gas_before": 30000001},
        ):
            with self.subTest(bad=bad):
                world = {"mir_layout": bad}
                result, _ = self.pair(source, source, fixture=world, status=2)
                self.assertNotIn("verified\n", result.stdout)

    def test_replay_independently_rejects_invalid_layouts(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "world.json"
            path.write_text(json.dumps({"version": 1, **fixture()}))
            initial = world.read(path, SOLAR, None, kind="mir")
        for malformed in (
            {"functions": {"f": {"frame_base": 129}}},
            {"code": [1], "data": {"0": {"offset": 1, "size": 1}}},
            {"code": [1], "data": {"00": {"offset": 0, "size": 1}}},
            {"immutable_slots": {"0": 256}, "immutable_values": {"0": "1"}},
            {"libraries": {"0": str(2**160)}},
            {"gas_before": 30000001},
        ):
            with self.subTest(layout=malformed):
                initial["mir_layout"] = malformed
                program = Program(
                    ("world-entry", "mir"),
                    [],
                    "MIR.State.Run",
                    "",
                    canonical_ir=state.module("ret 0"),
                    selector="f",
                    evm_version="cancun",
                    world=initial,
                )
                with self.assertRaises(Unsupported):
                    native.replay(
                        program, program, [], executable=state.REPLAY, timeout=10
                    )

    def test_documented_placement_and_gas_examples(self):
        for name in ("layout", "gas-sample"):
            folder = ROOT / "examples/mir" / name
            source = (folder / "before.mir").read_text()
            initial = json.loads((folder / "world.json").read_text())
            for after, status in (("after.mir", 0), ("broken.mir", 1)):
                with self.subTest(example=name, after=after):
                    self.pair(
                        source,
                        (folder / after).read_text(),
                        fixture=initial,
                        status=status,
                    )

    def test_recursive_activations_cannot_share_one_frame_binding(self):
        source = state.module(
            "v0 = internal_frame_addr 0\nv1: i1 = eq arg0, 0\njumpi v1, bb1, bb2\n"
            "bb1:\nret 0\nbb2:\nv2 = sub arg0, 1\nv3 = icall @f, v2\nret v3",
            "fn @f(arg0: i256) -> i256",
        )
        result, _ = self.pair(
            source,
            source,
            fixture=fixture(functions={"f": {"frame_base": 256}}),
            status=2,
        )
        self.assertIn("activation", result.stderr)

    def test_closed_initial_reads_preserve_symbolic_bytes(self):
        source = state.module(
            "mstore 0, arg0\nv0 = mload 64\nv1 = add arg0, v0\nret v1",
            "fn @f(arg0: i256) -> i256",
        )
        expected = state.module(
            "mstore 0, arg0\nv0 = add arg0, 128\nret v0",
            "fn @f(arg0: i256) -> i256",
        )
        initial = {
            "memory": {"data": "0x" + (bytes(64) + (128).to_bytes(32, "big")).hex()}
        }
        self.pair(source, expected, fixture=initial)
