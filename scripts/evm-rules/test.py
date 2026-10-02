# /// script
# requires-python = ">=3.14"
# dependencies = []
# ///
"""Regression tests for Lean proofs, ISLE contracts and rule discovery."""

import json
import subprocess
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

from evm_rules.discovery import (
    Cost,
    Prices,
    discover_rules,
    emit_rule,
    enumerate_rules,
    read_seeds,
)
from evm_rules.isle import ISLE, Context, Rule, forms
from evm_rules.late import execute as execute_late
from evm_rules.lean import HEADER, Lean, compile_source, theorem
from evm_rules.memory import MemoryAddresses
from evm_rules.mining import abstract_patterns, mine
from evm_rules.proof import FILES, check, library, obligations, verify_file
from evm_rules.semantics import MASK, SIGN, Expr, Unsupported, concrete, expr
from evm_rules.stack import requirements


def expression(op, *args):
    return expr(op, *args)


class MiningTests(unittest.TestCase):
    def mine_source(self, source, **kwargs):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "input.mir"
            path.write_text(source)
            return mine([path], **kwargs)

    def test_function_local_ids_and_frequency(self):
        report = self.mine_source("""@module M
fn @first(arg0: u256, arg1: u256) {
  bb0:
    v0 = or arg0, arg1
    v1 = and arg0, arg1
    v2 = sub v0, v1
    ret v2
}
fn @second(arg0: u256, arg1: u256) {
  bb0:
    v10 = or arg0, arg1
    v11 = and arg0, arg1
    v12 = sub v10, v11
    ret v12
}
""")
        self.assertEqual(len(report["candidates"]), 1)
        row = report["candidates"][0]
        self.assertEqual(row["tree"], ["sub", ["or", "x", "y"], ["and", "x", "y"]])
        self.assertEqual(row["occurrences"], 2)
        self.assertEqual([e["line"] for e in row["examples"]], [6, 13])
        self.assertEqual(len(report["sources"][0]["sha256"]), 64)

    def test_boundaries_and_shared_producers(self):
        template = """fn @f(arg0: u256, arg1: u256) {
  bb0:
    v0 = not arg0
%s
    v1 = not v0
    ret v1
}
"""
        for barrier in (
            "    sstore 0, v0",
            "    v2 = mload v0",
            "  bb1:",
            "    v3 = unknown v0",
        ):
            report = self.mine_source(template % barrier)
            self.assertEqual(report["candidates"], [], barrier)
        # A second use after the root prevents claiming deletion of its producer.
        report = self.mine_source((template % "").replace("ret v1", "ret v0, v1"))
        self.assertEqual(report["candidates"], [])

    def test_mined_seed_is_proved_before_emission(self):
        report = self.mine_source("""fn @f(arg0: u256) {
  bb0:
    v0 = not arg0
    v1 = not v0
    ret v1
}
""")
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "seeds.json"
            path.write_text(json.dumps([r["tree"] for r in report["candidates"]]))
            prices = Prices("osaka")
            seeds = read_seeds(path, prices, ["x", "y", "z"])
            rules, stats = enumerate_rules(
                prices, ["x"], ["not"], 1, 10, 5000, seeds=seeds
            )
        self.assertEqual(stats["seeds_proved"], 1)
        self.assertEqual(rules[0][1], Expr.var("x"))

    def test_subtree_abstraction_exposes_dynamic_masks(self):
        source = """fn @mask(arg0: u256) {
  bb0:
    v0 = sub 32, arg0
    v1 = shl 3, v0
    v2 = shl v1, 1
    v3 = sub v2, 1
    v4 = not v3
    ret v4
}
"""
        report = self.mine_source(source, abstract_subtrees=True)
        mask = [
            r
            for r in report["candidates"]
            if r["tree"] == ["not", ["sub", ["shl", "x", 1], 1]]
        ]
        self.assertEqual(len(mask), 1)
        self.assertEqual(mask[0]["occurrences"], 1)
        self.assertEqual(mask[0]["abstract_occurrences"], 1)
        self.assertEqual(mask[0]["examples"][0]["line"], 7)
        self.assertTrue(mask[0]["examples"][0]["abstracted"])
        self.assertEqual(report["bounds"]["max_subtree_cuts"], 1)

    def test_abstraction_preserves_repetition_and_variable_bound(self):
        x, y, z = map(Expr.var, "xyz")
        shared = expression("or", x, y)
        source = expression("sub", expression("add", shared, z), shared)
        patterns = list(abstract_patterns(source))
        self.assertIn(expression("sub", expression("add", x, y), x), patterns)
        self.assertEqual(len(patterns), len(set(patterns)))
        # Cutting x + y must not leave four inputs (the cut, x, y and z).
        source = expression(
            "or", expression("add", x, y), expression("xor", x, expression("and", y, z))
        )
        patterns = list(abstract_patterns(source))
        self.assertTrue(patterns)
        self.assertTrue(all(len(pattern.variables()) <= 3 for pattern in patterns))

    def test_abstraction_generalizes_a_repeated_literal(self):
        x, y, z = map(Expr.var, "xyz")
        source = expression("eq", expression("and", x, 255), expression("and", y, 255))
        # The same mask remains the same input on both sides; other operands
        # are renamed in traversal order, including the newly exposed mask.
        general = expression("eq", expression("and", x, y), expression("and", z, y))
        self.assertIn(general, list(abstract_patterns(source)))


class StackProofTests(unittest.TestCase):
    def verify(self, source):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "stack_peephole.isle"
            path.write_text(source)
            return verify_file(path)

    def test_actual_compiled_stack_rules(self):
        report = verify_file(ISLE / "evm-ir/stack_peephole.isle")
        self.assertEqual(len(report["rules"]), 7)
        self.assertTrue(all(r["status"] == "proved" for r in report["rules"]))
        self.assertGreater(sum(len(r["variants"]) for r in report["rules"]), 900)

    def test_wrong_depth_and_opcode_have_replayed_counterexamples(self):
        for source in (
            "(rule (peep_swap (last2 (dup 2) (swap 1))) (rewrite 2 (Edit.Keep 1)))",
            "(rule (peep_op (last2 (opcode $NOT) (opcode $NOT))) (rewrite 2 (Edit.OverwriteOne $ISZERO)))",
        ):
            result = self.verify(source)["rules"][0]
            self.assertEqual(result["status"], "counterexample")
            self.assertTrue(result["obligations"][0]["replayed"])

    def test_equality_shuffle_requires_symmetric_operands(self):
        correct = "(rule (peep_pop (unprotected_last5 (opcode $DUP2) (opcode $EQ) (opcode $ISZERO) (opcode $SWAP1) (opcode $POP))) (rewrite 5 (Edit.RemoveFirstKeepTwo)))"
        self.assertEqual(self.verify(correct)["rules"][0]["status"], "proved")
        wrong = correct.replace("$DUP2", "$DUP1")
        result = self.verify(wrong)["rules"][0]
        self.assertEqual(result["status"], "counterexample")
        self.assertTrue(result["obligations"][0]["replayed"])

    def test_unknown_effect_and_changed_extent_fail_closed(self):
        for source in (
            "(rule (peep_pop (last2 (opcode $MLOAD) (pop))) (rewrite 2 (Edit.Keep 0)))",
            "(rule (peep_pop (last2 (dup 1) (pop))) (rewrite 3 (Edit.Keep 0)))",
            "(rule (peep_pop (last2 (dup 1) (pop))) (rewrite 2 (Edit.Unknown)))",
            "(rule (peep_op (last2 (dup 1) (pop))) (rewrite 2 (Edit.Keep 0)))",
        ):
            self.assertEqual(self.verify(source)["rules"][0]["status"], "unsupported")


class LateWordProofTests(unittest.TestCase):
    def verify(self, source):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "late_word.isle"
            path.write_text(source)
            return verify_file(path)

    def test_compiled_mask_window_and_boundaries(self):
        report = self.verify((ISLE / "evm-ir/late_word.isle").read_text())
        result = report["rules"][0]
        self.assertTrue(all(r["status"] == "proved" for r in report["rules"]))
        self.assertEqual(
            (
                result["minimum_stack"],
                result["summary_before_peak"],
                result["summary_after_peak"],
            ),
            (0, 3, 2),
        )
        before, _ = execute_late(
            [("push", 1), ("dup", 1), ("count", None), ("shl", None), ("sub", None)]
        )
        after, _ = execute_late(
            [("push", 0), ("not", None), ("count", None), ("shl", None), ("not", None)]
        )
        for n in (0, 1, 255, 256, 257, MASK):
            expected = (1 << n) - 1 if n < 256 else MASK
            self.assertEqual(concrete(before[0], {"n": n}), expected)
            self.assertEqual(concrete(after[0], {"n": n}), expected)

    def test_changed_shift_replays_counterexample(self):
        source = (
            (ISLE / "evm-ir/late_word.isle")
            .read_text()
            .replace("(opcode $SHL)", "(opcode $SHR)")
        )
        self.assertTrue(
            all(r["status"] == "counterexample" for r in self.verify(source)["rules"])
        )

    def test_missing_contract_or_changed_edit_fails_closed(self):
        source = (ISLE / "evm-ir/late_word.isle").read_text()
        for change in (
            source.replace("(if-let true (closed_count window))", ""),
            source.replace("(late_length window)", "4"),
            source.replace("(Edit.LowMask)", "(Edit.Keep 1)"),
            source.replace("(dup 1)", "(dup 2)"),
            source.replace("push one", "push 2").replace(
                "u256_is_one one", "u256_is_one 2"
            ),
        ):
            self.assertTrue(
                any(r["status"] == "unsupported" for r in self.verify(change)["rules"])
            )


class DiscoveryTests(unittest.TestCase):
    def test_deep_seeds_search_a_small_replacement_frontier(self):
        x, y = Expr.var("x"), Expr.var("y")
        seed = expression("sub", expression("or", x, y), expression("and", x, y))
        rules, summary = enumerate_rules(
            Prices("osaka"), ["x", "y"], ["xor"], 1, 20, 5000, seeds=[seed]
        )
        self.assertIn(
            (seed, expression("xor", x, y)), [(lhs, rhs) for lhs, rhs, _, _ in rules]
        )
        self.assertEqual(summary["seeds_proved"], 1)
        self.assertLess(summary["expressions"], 20)

    def test_seed_sample_collisions_and_unknowns_are_not_proofs(self):
        x = Expr.var("x")
        seed = expression("add", x, 1)
        initial = [{"x": 0}]
        rules, summary = enumerate_rules(
            Prices("osaka"),
            ["x"],
            ["not"],
            1,
            20,
            5000,
            initial_samples=initial,
            seeds=[seed],
        )
        self.assertFalse(any(lhs == seed for lhs, _, _, _ in rules))
        self.assertGreater(summary["counterexamples"], 0)
        self.assertGreater(summary["samples"], 1)
        self.assertEqual(initial, [{"x": 0}])
        with patch("evm_rules.discovery.check", return_value={"status": "unknown"}):
            rules, summary = enumerate_rules(
                Prices("osaka"),
                ["x"],
                ["not"],
                1,
                20,
                5000,
                seeds=[expression("not", expression("not", x))],
            )
        self.assertFalse(rules)
        self.assertGreater(summary["unknown"], 0)

    def test_seed_reader_rejects_invalid_or_unbounded_inputs(self):
        deep = "x"
        for _ in range(17):
            deep = ["not", deep]
        invalid = [
            [],
            [True],
            ["x"],
            [["add", "x"]],
            [["not", "y"]],
            [["not", 123456789]],
            [["storage", "x"]],
            [deep],
            [["not", "x"]] * 129,
        ]
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "seeds.json"
            for rows in invalid:
                path.write_text(json.dumps(rows))
                with self.subTest(rows=rows), self.assertRaises(ValueError):
                    read_seeds(path, Prices("osaka"), ["x"])
            path.write_text(json.dumps([["not", "x"], ["not", "x"]]))
            self.assertEqual(
                read_seeds(path, Prices("osaka"), ["x"]),
                [expression("not", Expr.var("x"))],
            )

    def test_seed_cli_proves_and_documents_emitted_source(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "seeds.json"
            path.write_text(json.dumps([["sub", ["or", "x", "y"], ["and", "x", "y"]]]))
            output = Path(directory) / "candidates.isle"
            report = discover_rules(
                SimpleNamespace(
                    runs=200,
                    max_rules=32,
                    evm_version="osaka",
                    objective="gas",
                    seed_expressions=path,
                    variables=["x", "y"],
                    ops=["xor"],
                    max_ops=1,
                    max_expressions=20,
                    timeout_ms=5000,
                    include_constants=False,
                    emit_isle=output,
                )
            )
            self.assertTrue(report["accepted"])
            self.assertEqual(report["summary"]["seeds_proved"], 1)
            self.assertEqual(len(report["seeds_sha256"]), 64)
            self.assertIn(";; ((x | y) - (x & y)) => (x ^ y)", output.read_text())

    def test_empty_search_does_not_leave_stale_candidates(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "candidates.isle"
            output.write_text("stale rule")
            report = discover_rules(
                SimpleNamespace(
                    runs=200,
                    max_rules=32,
                    evm_version="osaka",
                    objective="gas",
                    variables=["x"],
                    ops=["not"],
                    max_ops=1,
                    max_expressions=10,
                    timeout_ms=5000,
                    include_constants=False,
                    emit_isle=output,
                )
            )
            self.assertEqual(report["emitted_verification"]["status"], "no_candidates")
            self.assertEqual(forms(output.read_text()), [])

    def test_search_bounds_and_variable_validation(self):
        for variables in (["x", "x"], ["_"], ["true"], ["x)"]):
            with self.assertRaises(ValueError):
                enumerate_rules(Prices("osaka"), variables, ["not"], 1, 20, 5000)

    def test_cheaper_proved_representative_survives(self):
        rules, _ = enumerate_rules(
            Prices("osaka"),
            ["x", "y"],
            ["xor", "not"],
            3,
            1000,
            5000,
            include_constants=True,
        )
        self.assertTrue(any(rhs.op == "not" for _, rhs, _, _ in rules))

    def test_target_snapshot_and_economic_objectives(self):
        old, new = Prices("homestead"), Prices("osaka")
        self.assertNotIn("shl", old.ops)
        self.assertIn("shl", new.ops)
        self.assertLess(new.constants[0].gas, old.constants[0].gas)
        small, fast = Cost(5, 1), Cost(3, 8)
        self.assertLess(
            Prices("osaka", "lifetime", 0).key(small),
            Prices("osaka", "lifetime", 0).key(fast),
        )
        self.assertGreater(
            Prices("osaka", "lifetime", 10000).key(small),
            Prices("osaka", "lifetime", 10000).key(fast),
        )

    def test_sampling_collision_requires_proof_and_refines(self):
        _, summary = enumerate_rules(
            Prices("osaka"), ["x"], ["not"], 1, 20, 5000, initial_samples=[{"x": 0}]
        )
        self.assertGreater(summary["counterexamples"], 0)
        self.assertGreater(summary["samples"], 1)

    def test_emitted_candidate_round_trips_through_actual_isle(self):
        x, y = Expr.var("x"), Expr.var("y")
        lhs = expression("xor", expression("or", x, y), expression("and", x, y))
        rhs = expression("xor", x, y)
        form, line = forms(emit_rule(lhs, rhs))[0]
        context = Context()
        actual_lhs, actual_rhs = context.obligation(Rule(form, line, "generated"))
        result = check(actual_lhs, actual_rhs, context.assumptions)
        self.assertEqual(result["status"], "proved")

    def test_multi_operation_discovery_and_emission(self):
        rules, _ = enumerate_rules(
            Prices("osaka"),
            ["x", "y"],
            ["not", "and", "or"],
            3,
            500,
            5000,
            max_rhs_ops=2,
        )
        recipes = [(lhs, rhs) for lhs, rhs, _, _ in rules if rhs.operators() == 2]
        self.assertTrue(recipes)
        for lhs, rhs in recipes:
            form, line = forms(emit_rule(lhs, rhs))[0]
            self.assertEqual(form[1][0], "sequence_rewrite")
            context = Context()
            left, right = context.obligation(Rule(form, line, "generated"))
            self.assertEqual(
                check(left, right, context.assumptions)["status"],
                "proved",
            )

    def test_large_literal_guard_and_recipe_are_verified(self):
        x = Expr.var("x")
        lhs = expression("lt", x, 1 << 160)
        rhs = expression("eq", expression("shr", 160, x), 0)
        source = emit_rule(lhs, rhs)
        for expected, text in (
            ("proved", source),
            ("counterexample", source.replace("(u256 160)", "(u256 159)")),
        ):
            form, line = forms(text)[0]
            context = Context()
            left, right = context.obligation(Rule(form, line, "generated"))
            self.assertEqual(
                check(left, right, context.assumptions)["status"],
                expected,
            )
        self.assertIn((1 << 160) - 1, Prices("osaka").constants)
        with self.assertRaises(ValueError):
            enumerate_rules(
                Prices("osaka"), ["x"], ["and"], 2, 20, 5000, constants=[123456789]
            )

    def test_specialized_inputs_keep_canonical_constant_results(self):
        rules, _ = enumerate_rules(
            Prices("osaka"),
            ["x"],
            ["xor"],
            2,
            30,
            5000,
            include_constants=True,
            constants=[255],
        )
        self.assertTrue(
            any(lhs.variables() and rhs == Expr.const(0) for lhs, rhs, _, _ in rules)
        )


class MemoryAddressTests(unittest.TestCase):
    def test_actual_projection_rules_and_missing_guards(self):
        path = ISLE / "mir/egraph.isle"
        rules = [
            Rule(form, line, str(path))
            for form, line in forms(path.read_text())
            if form[0] == "rule"
            and any(name in repr(form) for name in MemoryAddresses.SHAPES)
        ]
        self.assertEqual(len(rules), 3)
        for rule in rules:
            cx = Context()
            lhs, rhs = cx.obligation(rule)
            result = check(lhs, rhs, cx.assumptions)
            self.assertEqual(result["status"], "proved", rule.line)
            # Removing the actual source guard must expose a nonzero header
            # or field offset, rather than implicitly assuming the rewrite.
            unguarded = Rule(
                (rule.form[0], rule.form[1], rule.form[-1]), rule.line, rule.source
            )
            cx = Context()
            lhs, rhs = cx.obligation(unguarded)
            result = check(lhs, rhs, cx.assumptions)
            self.assertEqual(result["status"], "counterexample", rule.line)
            self.assertTrue(result["replayed"])

    def setUp(self):
        self.examples = HEADER

    def tearDown(self):
        library()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "memory.lean"
            path.write_text(self.examples)
            result = compile_source(path)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def assert_concrete_and_symbolic(self, value, values, expected):
        emitter = Lean(value.variables())
        emitter.variables = {key: f"{values[key]}#256" for key in emitter.variables}
        self.examples += (
            f"example : {emitter.term(value)} = {expected}#256 := by decide\n"
        )
        self.assertEqual(concrete(value, values), expected)

    def test_data_headers_and_slice_payload_pointers(self):
        cx = Context()
        object, kind = map(Expr.var, ("object", "kind"))
        value = cx.operation("Op.MemoryObjectData", [object, kind])
        flag = cx.memory.slices[object].args[0]
        for tag, header in ((0, 32), (1, 32), (2, 0), (3, 0)):
            for is_slice in (0, 1):
                for pointer in (0, 128, MASK):
                    self.assert_concrete_and_symbolic(
                        value,
                        {"object": pointer, "kind": tag, flag: is_slice},
                        (pointer + (0 if is_slice else header)) & MASK,
                    )
        # Adding a header to a slice pointer is wrong, even for dynamic data.
        wrong = expression("add", object, cx.memory.data_offset(kind))
        result = check(value, wrong, cx.assumptions)
        self.assertEqual(result["status"], "counterexample")
        self.assertEqual(int(result["inputs"][flag], 16), 1)

    def test_field_offsets_saturate_before_full_word_address_addition(self):
        cx = Context()
        object, layout, field = map(Expr.var, ("object", "layout", "field"))
        value = cx.operation("Op.MemoryObjectFieldAddr", [object, layout, field])
        for index in (0, 1, (1 << 59) - 1, 1 << 59, (1 << 64) - 2):
            expected_offset = min(index * 32, (1 << 64) - 1)
            self.assert_concrete_and_symbolic(
                value, {"object": MASK, "field": index}, (MASK + expected_offset) & MASK
            )
        # Fields outside a struct's declared range have no valid projection.
        shape = cx.memory.layouts[layout]
        result = check(
            value,
            value,
            [*cx.assumptions, expr("eq", field, shape.fields)],
        )
        self.assertEqual(result["status"], "unknown")

    def test_element_strides_keep_zero_full_u32_and_wrapping_indices(self):
        cx = Context()
        object, layout, index = map(Expr.var, ("object", "layout", "index"))
        value = cx.operation("Op.MemoryObjectElementAddr", [object, layout, index])
        shape = cx.memory.layouts[layout]
        for tag in range(3):
            for words in (0, 1, (1 << 32) - 1):
                for i in (0, 1, MASK):
                    values = {
                        "object": MASK,
                        "index": i,
                        shape.kind.args[0]: tag,
                        shape.element_words.args[0]: words,
                    }
                    stride = 32 if tag == 0 else words * 32
                    expected = (MASK + (32 if tag < 2 else 0) + i * stride) & MASK
                    self.assert_concrete_and_symbolic(value, values, expected)
        result = check(value, value, [*cx.assumptions, expr("eq", shape.kind, 3)])
        self.assertEqual(result["status"], "unknown")

    def test_generated_schema_drift_and_wrong_arity_fail_closed(self):
        cx = Context()
        object, kind = map(Expr.var, ("object", "kind"))
        with (
            patch(
                "evm_rules.isle.forms",
                return_value=[
                    (
                        (
                            "type",
                            "Op",
                            "extern",
                            (
                                "enum",
                                (
                                    "MemoryObjectData",
                                    ("kind", "MemoryObjectKind"),
                                    ("object", "Value"),
                                ),
                            ),
                        ),
                        1,
                    )
                ],
            ),
            self.assertRaisesRegex(Unsupported, "changed memory address schema"),
        ):
            cx.operation("Op.MemoryObjectData", [object, kind])
        with self.assertRaises(Unsupported):
            cx.operation("Op.MemoryObjectData", [object])
        cx.memory.kind(kind)
        with self.assertRaises(Unsupported):
            cx.memory.layout(kind)


class ProofTests(unittest.TestCase):
    def source(self, text):
        form, line = forms(text)[0]
        context = Context()
        left, right = context.obligation(Rule(form, line, "test"))
        return check(left, right, context.assumptions)

    def compile(self, source):
        library()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "Test.lean"
            path.write_text(source)
            return compile_source(path)

    def test_complete_source_coverage(self):
        for path in FILES:
            entries = obligations(path)
            self.assertEqual(
                len(entries),
                sum(form[0] == "rule" for form, _ in forms(path.read_text())),
            )
            self.assertTrue(all(equalities for _, equalities, _, _ in entries))

    def test_required_guard_and_negative_contract(self):
        guard = "(if-let true (mask_covers mask x))"
        source = f"(rule (rewrite (Op.And x (iconst mask))) {guard} (Op.Add x (imm (u256 0))))"
        self.assertEqual(self.source(source)["status"], "proved")
        for changed in (
            source.replace(guard, ""),
            source.replace("if-let true", "if-let false"),
        ):
            result = self.source(changed)
            self.assertEqual(result["status"], "counterexample", result)
            self.assertTrue(result["replayed"])

    def test_impossible_guard_fails(self):
        result = self.source("""(rule (rewrite (Op.Add x (zero)))
            (if-let true (u256_is_zero x)) (if-let true (u256_is_one x)) x)""")
        self.assertNotEqual(result["status"], "proved")

    def test_wrong_witness_and_sorry_fail(self):
        x = Expr.var("x")
        source = theorem(x, x, [expr("eq", x, 0)], "bad_witness", {"x": 1})
        self.assertNotEqual(self.compile(source).returncode, 0)
        self.assertNotEqual(
            self.compile(HEADER + "example : False := by sorry\n").returncode, 0
        )

    def test_reader_fails_closed(self):
        for text in ("(rule", ")", "bare"):
            with self.assertRaises(ValueError):
                forms(text)
        for source in (
            "(rule (rewrite (Op.Unknown x)) x)",
            "(rule (rewrite (Op.Sub x x)) (if (u256_is_zero x)) x)",
            "(rule (rewrite (Op.Add @fresh:1 x)) x)",
            "(rule (rewrite (Op.Add @flag:x x)) x)",
            "(rule (rewrite (Op.Add x y)) (if-let true (below_const x)) x)",
        ):
            with self.subTest(source=source), self.assertRaises(Unsupported):
                self.source(source)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "empty.isle"
            path.write_text(";; no obligations\n")
            self.assertEqual(verify_file(path)["rules"][0]["status"], "unsupported")
        with self.assertRaises(Unsupported):
            Lean({"x"}).term(expr("unknown", Expr.var("x")))

    def test_ssa_identity_does_not_imply_word_inequality(self):
        result = self.source("""(rule (rewrite (Op.Eq a b))
            (if-let true (differ a b)) (imm (u256 0)))""")
        self.assertEqual(result["status"], "counterexample", result)
        self.assertEqual(result["inputs"]["a"], result["inputs"]["b"])

    def test_guarded_shift_cancellation(self):
        guard = "(if-let true (mask_covers (u256_shr shift (u256_max)) x))"
        source = f"""(rule (rewrite (Op.Shr (iconst shift) (shl (iconst shift) x)))
            (if-let true (u256_eq shift 1)) {guard} x)"""
        self.assertEqual(self.source(source)["status"], "proved")
        self.assertEqual(
            self.source(source.replace(guard, ""))["status"], "counterexample"
        )

    def test_byte_guard_prevents_wrapped_index(self):
        guard = "(if-let true (u256_lt index 32))"
        source = f"""(rule (rewrite (Op.Byte (iconst index) (shl (iconst shift) x)))
            (if-let true (u256_eq shift 8)) {guard}
            (Op.Byte (imm (u256_add index (u256 1))) x))"""
        self.assertEqual(self.source(source)["status"], "proved")
        result = self.source(source.replace(guard, ""))
        self.assertEqual(result["status"], "counterexample", result)
        self.assertGreaterEqual(int(result["inputs"]["index"], 16), 32)

    def test_constant_constructor_operand_order(self):
        for op in ("Shl", "Shr", "Byte"):
            source = f"""(rule (rewrite (Op.{op} (iconst index) (iconst value)))
                (imm (u256_{op.lower()} index value)))"""
            self.assertEqual(self.source(source)["status"], "proved")
            changed = source.replace(
                f"u256_{op.lower()} index value", f"u256_{op.lower()} value index"
            )
            self.assertEqual(self.source(changed)["status"], "counterexample")

    def test_call_effects_cannot_be_removed_or_changed(self):
        source = "(rule (rewrite (Op.Call gas address value a b c d)) (Op.Call gas address value a b c d))"
        self.assertEqual(self.source(source)["status"], "proved")
        self.assertEqual(
            self.source(
                "(rule (rewrite (Op.Call gas address value a b c d)) (Op.Call (imm (u256 1)) address value a b c d))"
            )["status"],
            "counterexample",
        )
        for replacement in (
            "(imm (u256 0))",
            "(Op.CallCode gas address value a b c d)",
        ):
            with self.assertRaises(Unsupported):
                self.source(
                    "(rule (rewrite (Op.Call gas address value a b c d)) "
                    + replacement
                    + ")"
                )
        with self.assertRaises(Unsupported):
            self.source("(rule (rewrite (Op.Add (call g a v b c d e) x)) x)")

    def test_balance_snapshot_cannot_cover_earlier_producers(self):
        with self.assertRaises(Unsupported):
            self.source("(rule (rewrite (Op.Add (balance x) y)) y)")

    def test_stack_depth_coverage_and_requirements(self):
        entries = obligations(ISLE / "evm-ir/stack_peephole.isle")
        for _, _, _, metadata in entries:
            variants = metadata["variants"]
            self.assertTrue(variants)
            if len(variants[0]["bindings"]) == 1:
                self.assertEqual(len(variants), 235)
            if len(variants[0]["bindings"]) == 2:
                self.assertEqual(
                    len(variants),
                    sum(
                        1 <= n < m and n + m <= 30
                        for n in range(1, 236)
                        for m in range(1, 236)
                    ),
                )
        self.assertEqual(requirements([("dup", (235,)), ("pop", ())]), (235, 1, 0))

    def test_physical_edits_require_exact_windows_and_guards(self):
        for name, old, new in (
            ("stack_peephole", "rewrite 2", "rewrite 3"),
            ("late_word", "protected_count", "closed_count"),
        ):
            source = (ISLE / f"evm-ir/{name}.isle").read_text().replace(old, new)
            with tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / f"{name}.isle"
                path.write_text(source)
                with self.assertRaises(Unsupported):
                    obligations(path)

    def test_process_failure_never_proves(self):
        x = Expr.var("x")
        with patch(
            "evm_rules.proof.compile_source",
            return_value=subprocess.CompletedProcess([], 1, "", "failed"),
        ):
            self.assertEqual(check(x, x)["status"], "unknown")

    def test_timeout_is_a_failure(self):
        with patch(
            "evm_rules.lean.subprocess.run",
            side_effect=subprocess.TimeoutExpired("lean", 1),
        ):
            self.assertNotEqual(compile_source(Path("ignored.lean")).returncode, 0)

    def test_concrete_semantics_boundaries(self):
        cases = []
        for op in (
            "add",
            "sub",
            "mul",
            "div",
            "mod",
            "sdiv",
            "smod",
            "and",
            "or",
            "xor",
            "lt",
            "gt",
            "slt",
            "sgt",
            "eq",
            "ne",
        ):
            for a, b in ((0, 0), (SIGN, MASK), (MASK, 1), (3, 2)):
                cases.append(expr(op, a, b))
        for op in ("shl", "shr", "sar", "byte", "signextend"):
            for index in (0, 30, 31, 32, 255, 256, MASK):
                cases.append(expr(op, index, SIGN | 255))
        for op in ("not", "iszero", "clz"):
            for value in (0, 1, SIGN, MASK):
                cases.append(expr(op, value))
        for op in ("addmod", "mulmod"):
            for modulus in (0, 1, MASK):
                cases.append(expr(op, MASK, MASK, modulus))
        for exponent in (0, 1, 2, 7):
            cases.append(expr("exp", 3, exponent))
        source = HEADER
        for expression in cases:
            actual = Lean(set()).term(expression)
            source += (
                f"example : {actual} = {concrete(expression, {})}#256 := by decide\n"
            )
        result = self.compile(source)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
