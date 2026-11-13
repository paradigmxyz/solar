"""Typed products, location-tagged slices and independently replayed lowering pairs."""

import json
import subprocess
import unittest

import test_mir_state as state
from test_cli import ROOT, SOLAR


def module(
    body, *, types="struct0: {i256, i1}", signature="fn @f() -> i256", helpers=""
):
    return f"@module Products\n@types\n{types}\n{signature} {{\nbb0:\n{body}\n}}\n{helpers}"


class MirAggregateTests(unittest.TestCase):
    pair = state.MirStateTests.pair

    def test_actual_lowering_pairs_and_broken_fields(self):
        for name, transform in (
            ("aggregates", "lower-structs"),
            ("slices", "lower-slices"),
        ):
            with self.subTest(name=name):
                folder = ROOT / "examples/mir" / name
                before = (folder / "before.mir").read_text()
                after = subprocess.check_output(
                    [
                        SOLAR,
                        folder / "before.mir",
                        "-Ogas",
                        f"-Zmir-pipeline={transform}",
                        "-Zvalidate-ir=true",
                    ],
                    text=True,
                )
                self.assertEqual(
                    after.splitlines()[1:],
                    (folder / "after.mir").read_text().splitlines()[1:],
                )
                fixture = json.loads((folder / "world.json").read_text())
                self.pair(before, after, fixture=fixture)
                result, record = self.pair(
                    before,
                    (folder / "broken.mir").read_text(),
                    fixture=fixture,
                    status=1,
                )
                self.assertEqual(
                    record["witness"]["observations"],
                    record["witness"]["native_observations"],
                )
                if name == "aggregates":
                    # Both programs agree at zero; a sample cannot certify the pair.
                    self.assertEqual(record["witness"]["inputs"]["arg3"], "1")
                    self.assertIn("source arg0[0][0]", result.stdout)
                    self.assertEqual(
                        record["world"]["fixture"]["boundary"]["inputs"][0]["fields"],
                        [0, 0],
                    )

    def test_partial_initialization_reads_only_selected_field(self):
        prefix = "v0 = insert_value struct0, undef struct0, 0, 7\n"
        self.pair(
            module(prefix + "v1 = extract_value struct0, v0, 0\nret v1"),
            module("ret 7"),
        )
        # Merely copying an undefined component does not give it a scalar value.
        body = (
            prefix
            + "v1 = extract_value struct0, v0, 1\nv2 = select false, v1, true\nv3 = zext i1 v2 to i256\nret v3"
        )
        self.pair(module(body), module("ret 1"))
        source = module(
            prefix
            + "v1 = extract_value struct0, v0, 1\nv2 = zext i1 v1 to i256\nret v2"
        )
        result, _ = self.pair(source, source, status=2)
        self.assertIn("undefined MIR scalar", result.stderr)
        source = module(prefix + "ret v0", signature="fn @f() -> struct0")
        self.pair(source, source, status=2)

    def test_nested_results_and_empty_products(self):
        before = module(
            "v0 = insert_value struct0, undef struct0, 0, arg0\nv1 = insert_value struct0, v0, 1, arg1\nv2 = insert_value struct1, undef struct1, 0, v1\nv3 = insert_value struct1, v2, 1, 9\nret v3",
            types="struct0: {i256, i1}\nstruct1: {struct0, i256}",
            signature="fn @f(arg0: i256, arg1: i1) -> struct1",
        )
        after = "@module Products\nfn @f(arg0: i256, arg1: i1) [return_abi=[i256,i1,i256]] {\nbb0:\nret arg0, arg1, 9\n}\n"
        self.pair(before, after)
        _, record = self.pair(before, after.replace("arg1, 9", "arg1, 8"), status=1)
        self.assertEqual(
            record["witness"]["observations"][0]["values"], ["0", "0", "9"]
        )
        empty = module(
            "ret undef struct0", types="struct0: {}", signature="fn @f() -> struct0"
        )
        self.pair(empty, state.module("ret", "fn @f()"))

    def test_internal_aggregate_calls_tail_calls_and_discard(self):
        helper = "fn @pair(arg0: i256) -> struct0 {\nbb0:\nv0 = insert_value struct0, undef struct0, 0, arg0\nv1 = insert_value struct0, v0, 1, true\nret v1\n}\n"
        before = module(
            "v0 = icall @pair, 11\nv1 = extract_value struct0, v0, 0\nret v1",
            helpers=helper,
        )
        self.pair(before, module("ret 11"))
        before = module(
            "tail_call @pair, 11", signature="fn @f() -> struct0", helpers=helper
        )
        after = module(
            "v0 = insert_value struct0, undef struct0, 0, 11\nv1 = insert_value struct0, v0, 1, true\nret v1",
            signature="fn @f() -> struct0",
        )
        self.pair(before, after)
        self.pair(module("icall @pair, 11\nret 0", helpers=helper), module("ret 0"))

    def test_product_phis_read_old_values_simultaneously(self):
        source = """@module Phis
@types
struct0: {i256}
fn @f() -> i256 {
bb0:
 v0 = insert_value struct0, undef struct0, 0, 7
 v1 = insert_value struct0, undef struct0, 0, 9
 jump bb1
bb1:
 v2 = phi struct0, [bb0: v0], [bb2: v3]
 v3 = phi struct0, [bb0: v1], [bb2: v2]
 v4: i1 = phi [bb0: true], [bb2: false]
 jumpi v4, bb2, bb3
bb2:
 jump bb1
bb3:
 v5 = extract_value struct0, v2, 0
 v6 = extract_value struct0, v3, 0
 v7 = sub v5, v6
 ret v7
}
"""
        self.pair(source, module("ret 2"))
        self.pair(source, module("ret 0"), status=1)

    def test_memory_slice_aliases_without_allocating(self):
        body = "mstore 0, 42\nv0 = inttoptr i256 0 to ptr<memory>\nv1 = make_memory_slice v0, 32\nv2 = extract_value slice<memory>, v1, 0\nmstore v2, 7\nv3 = mload 0\nret v3"
        self.pair(state.module(body), state.module("mstore 0, 7\nret 7"))
        self.pair(
            state.module(body),
            state.module(body.replace("inttoptr i256 0", "inttoptr i256 32")),
            status=1,
        )
        body = f"v0 = inttoptr i256 {2**256 - 1} to ptr<memory>\nv1 = make_memory_slice v0, {2**256 - 1}\nv2 = msize\nret v2"
        self.pair(state.module(body), state.module("ret 0"))

    def test_returndata_slice_uses_the_current_buffer(self):
        prefix = "mstore 0, 42\nv0 = staticcall 100000, 4, 0, 32, 0, 0\n"
        body = (
            prefix
            + "v1 = inttoptr i256 0 to ptr<returndata>\nv2 = make_returndata_slice v1, 32\nv3 = extract_value slice<returndata>, v2, 0\nv4 = ptrtoint ptr<returndata> v3 to i256\nv5 = extract_value slice<returndata>, v2, 1\nreturndatacopy 64, v4, v5\nv6 = mload 64\nret v6"
        )
        self.pair(
            state.module(body),
            state.module(prefix + "returndatacopy 64, 0, 32\nret 42"),
        )

    def test_boundary_rejects_changed_pointer_spaces(self):
        before = state.module("ret arg0", "fn @f(arg0: ptr<memory>) -> ptr<memory>")
        after = before.replace("ptr<memory>", "ptr<calldata>")
        result, _ = self.pair(before, after, status=2)
        self.assertIn("pointer address spaces differ", result.stderr)

    def test_type_limits_and_physical_call_results_stay_unsupported(self):
        for fields in ("i256," * 256 + "i256",):
            source = module(
                "ret arg0",
                types="struct0: {" + fields + "}",
                signature="fn @f(arg0: struct0) -> struct0",
            )
            result, _ = self.pair(source, source, status=2)
            self.assertIn("type limit", result.stderr)
        types = "struct0: {i256}\n" + "\n".join(
            f"struct{i}: {{struct{i - 1}}}" for i in range(1, 34)
        )
        source = module(
            "ret arg0", types=types, signature="fn @f(arg0: struct33) -> struct33"
        )
        result, _ = self.pair(source, source, status=2)
        self.assertIn("type limit", result.stderr)
        helper = "fn @pair() [return_abi=[i256,i256]] {\nbb0:\nret 1, 2\n}\n"
        source = state.module("v0 = icall @pair\nret v0") + helper
        result, _ = self.pair(source, source, status=2)
        self.assertIn("missing MIR function placement", result.stderr)

    def test_repeated_type_metadata_is_bounded_before_export(self):
        fields = ",".join(["i256"] * 64)
        body = "v0 = insert_value struct0, undef struct0, 0, 7\n"
        body += "\n".join(f"v{i} = extract_value struct0, v0, 0" for i in range(1, 514))
        body += "\nret 0"
        source = module(body, types="struct0: {" + fields + "}")
        result, _ = self.pair(source, source, status=2)
        self.assertIn("model type budget", result.stderr)
