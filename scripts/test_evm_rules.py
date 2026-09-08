# /// script
# requires-python = ">=3.11"
# dependencies = ["z3-solver==4.16.0.0"]
# ///
"""Regression tests for the trusted word model, ISLE reader and discovery gate."""

from contextlib import redirect_stderr, redirect_stdout
import io
import json
from pathlib import Path
from types import SimpleNamespace
import tempfile
import unittest
from unittest.mock import patch

import z3

from evm_rules.discovery import Cost, Prices, discover_rules, emit_rule, enumerate_rules, read_seeds
from evm_rules.isle import Context, ISLE, Rule, forms, verify_file
from evm_rules.mining import abstract_patterns, mine
from evm_rules.stack import verify_stack_file
from evm_rules.late import execute as execute_late, verify_late_file
from evm_rules.semantics import Expr, MASK, MODULUS, SIGN, Model, Unsupported, check, concrete, partition_shift
from verify_evm_rules import main


def expression(op, *args):
    return Expr(op, tuple(Expr.const(a) if isinstance(a, int) else a for a in args))


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
        for barrier in ("    sstore 0, v0", "    v2 = mload v0", "  bb1:", "    v3 = unknown v0"):
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
            rules, stats = enumerate_rules(prices, ["x"], ["not"], 1, 10, 5000, seeds=seeds)
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
        mask = [r for r in report["candidates"]
                if r["tree"] == ["not", ["sub", ["shl", "x", 1], 1]]]
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
        source = expression("or", expression("add", x, y),
                            expression("xor", x, expression("and", y, z)))
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
            return verify_stack_file(path)

    def test_actual_compiled_stack_rules(self):
        report = verify_stack_file(ISLE / "stack_peephole.isle")
        self.assertEqual(len(report["rules"]), 6)
        self.assertTrue(all(r["status"] == "proved" for r in report["rules"]))
        self.assertGreater(sum(len(r["variants"]) for r in report["rules"]), 900)

    def test_wrong_depth_and_opcode_have_replayed_counterexamples(self):
        for source in (
            "(rule (peep_nonpush (last2 (dup 2) (swap 1))) (rewrite 2 (Edit.Keep 1)))",
            "(rule (peep_nonpush (last2 (opcode $NOT) (opcode $NOT))) (rewrite 2 (Edit.OverwriteOne $ISZERO)))",
        ):
            result = self.verify(source)["rules"][0]
            self.assertEqual(result["status"], "counterexample")
            self.assertTrue(result["variants"][0]["replayed"])

    def test_unknown_effect_and_changed_extent_fail_closed(self):
        for source in (
            "(rule (peep_nonpush (last2 (opcode $MLOAD) (pop))) (rewrite 2 (Edit.Keep 0)))",
            "(rule (peep_nonpush (last2 (dup 1) (pop))) (rewrite 3 (Edit.Keep 0)))",
            "(rule (peep_nonpush (last2 (dup 1) (pop))) (rewrite 2 (Edit.Unknown)))",
        ):
            self.assertEqual(self.verify(source)["rules"][0]["status"], "unsupported")


class LateWordProofTests(unittest.TestCase):
    def verify(self, source):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "late_word.isle"
            path.write_text(source)
            return verify_late_file(path)

    def test_compiled_mask_window_and_boundaries(self):
        report = self.verify((ISLE / "late_word.isle").read_text())
        result = report["rules"][0]
        self.assertTrue(all(r["status"] == "proved" for r in report["rules"]))
        self.assertEqual((result["minimum_stack"], result["summary_before_peak"], result["summary_after_peak"]), (0, 3, 2))
        before, _ = execute_late([("push", 1), ("dup", 1), ("count", None), ("shl", None), ("sub", None)])
        after, _ = execute_late([("push", 0), ("not", None), ("count", None), ("shl", None), ("not", None)])
        for n in (0, 1, 255, 256, 257, MASK):
            expected = (1 << n) - 1 if n < 256 else MASK
            self.assertEqual(concrete(before[0], {"n": n}), expected)
            self.assertEqual(concrete(after[0], {"n": n}), expected)

    def test_changed_shift_replays_counterexample(self):
        source = (ISLE / "late_word.isle").read_text().replace("(opcode $SHL)", "(opcode $SHR)")
        self.assertTrue(all(r["status"] == "counterexample" for r in self.verify(source)["rules"]))

    def test_missing_contract_or_changed_edit_fails_closed(self):
        source = (ISLE / "late_word.isle").read_text()
        for change in (source.replace("(if-let true (closed_count window))", ""),
                       source.replace("(late_length window)", "4"),
                       source.replace("(Edit.LowMask)", "(Edit.Keep 1)"),
                       source.replace("(dup 1)", "(dup 2)"),
                       source.replace("push one", "push 2").replace("u256_is_one one", "u256_is_one 2")):
            self.assertTrue(any(r["status"] == "unsupported" for r in self.verify(change)["rules"]))


class SemanticsTests(unittest.TestCase):
    def assert_evaluation(self, op, args, expected):
        expr = expression(op, *args)
        self.assertEqual(concrete(expr, {}), expected & MASK)
        self.assertEqual(z3.simplify(Model().eval(expr)).as_long(), expected & MASK)

    def test_evm_boundaries(self):
        cases = [
            ("add", (MASK, 1), 0), ("sub", (0, 1), MASK),
            ("mul", (SIGN, 2), 0), ("div", (MASK, 0), 0),
            ("mod", (MASK, 0), 0), ("sdiv", (MASK, 0), 0),
            ("smod", (MASK, 0), 0), ("sdiv", (SIGN, MASK), SIGN),
            ("sdiv", (MODULUS - 5, 3), MODULUS - 1),
            ("sdiv", (5, MODULUS - 3), MODULUS - 1),
            ("smod", (MODULUS - 5, 3), MODULUS - 2),
            ("smod", (5, MODULUS - 3), 2),
            ("addmod", (MASK, 1, 3), 1), ("mulmod", (SIGN, 2, 3), 1),
            ("addmod", (MASK, MASK, 0), 0), ("mulmod", (MASK, MASK, 0), 0),
            ("exp", (0, 0), 1), ("exp", (MASK, 2), 1),
            ("shl", (256, 1), 0), ("shr", (MASK, MASK), 0),
            ("sar", (256, SIGN), MASK), ("sar", (MASK, 1), 0),
            ("byte", (0, SIGN), 128), ("byte", (31, 255), 255),
            ("byte", (32, MASK), 0), ("byte", (MASK, MASK), 0),
            ("signextend", (0, 128), MODULUS - 128),
            ("signextend", (0, 127), 127), ("signextend", (31, SIGN), SIGN),
            ("signextend", (MASK, 128), 128),
            ("clz", (0,), 256), ("clz", (1,), 255), ("clz", (SIGN,), 0),
            ("lt", (MASK, 1), 0), ("slt", (MASK, 1), 1),
            ("select", (2, 7, 9), 7), ("iszero", (2,), 0),
        ]
        for op, args, expected in cases:
            with self.subTest(op=op, args=args):
                self.assert_evaluation(op, args, expected)

    def test_symbolic_zero_division(self):
        x = Expr.var("x")
        for op in ("div", "sdiv", "mod", "smod"):
            result, _ = check(expression(op, x, 0), Expr.const(0))
            self.assertEqual(result["status"], "proved", op)

    def test_counterexample_replay(self):
        x = Expr.var("x")
        result, _ = check(expression("shr", 1, expression("shl", 1, x)), x)
        self.assertEqual(result["status"], "counterexample")
        self.assertTrue(result["replayed"])
        self.assertNotEqual(result["lhs_value"], result["rhs_value"])

    def test_unknown_never_proves(self):
        x = Expr.var("x")
        with patch.object(type(z3.Solver()), "check", side_effect=[z3.sat, z3.unknown]):
            result, _ = check(x, x)
        self.assertEqual(result["status"], "unknown")

    def test_equality_query_preserves_preconditions(self):
        x, y = Expr.var("x"), Expr.var("y")
        model = Model()
        lhs = expression("add", x, y)
        # The identity is conditional on y == 0. Replay the exported query too.
        result, query = check(lhs, x, [model.eval(y) == 0], model=model)
        self.assertEqual(result["status"], "proved")
        solver = z3.SolverFor("QF_BV")
        solver.from_string(query)
        self.assertEqual(solver.check(), z3.unsat)
        # The counterexample must satisfy the guard after the solver reset.
        result, _ = check(lhs, x, [model.eval(y) == 1], model=model)
        self.assertEqual(result["status"], "counterexample")
        self.assertEqual(int(result["inputs"]["y"], 16), 1)
        self.assertTrue(result["replayed"])
        # Contradictory guards cannot prove an identity vacuously.
        result, query = check(lhs, lhs, [model.eval(y) == 0, model.eval(y) == 1], model=model)
        self.assertEqual(result["status"], "inapplicable")
        self.assertEqual(query, "")

    def test_unsupported_operations_fail(self):
        for op in ("sload", "mload", "call", "keccak256", "mystery"):
            with self.assertRaises(Unsupported):
                Model().eval(expression(op, 0))
        with self.assertRaises(Unsupported):
            Model().eval(expression("add", 0))

    def test_shift_partition_covers_large_counts(self):
        x, y, n = map(Expr.var, ("x", "y", "n"))
        lhs = expression("shl", n, expression("or", x, y))
        rhs = expression("or", expression("shl", n, x), expression("shl", n, y))
        result, queries = partition_shift(lhs, rhs, [], 5000, Model())
        self.assertEqual(result["status"], "proved")
        self.assertEqual(result["cases"], 257)
        self.assertEqual(len(queries), 258)
        # Masking the count is wrong precisely in the last, saturating partition.
        lhs = expression("shr", n, x)
        rhs = expression("shr", expression("and", n, 255), x)
        result, _ = partition_shift(lhs, rhs, [], 5000, Model())
        self.assertEqual(result["status"], "counterexample")
        self.assertGreaterEqual(int(result["inputs"]["n"], 16), 256)
        self.assertTrue(result["replayed"])

    def test_incomplete_partition_never_proves(self):
        x, n = Expr.var("x"), Expr.var("n")
        lhs = expression("shl", n, x)
        with patch("evm_rules.semantics.time.monotonic", side_effect=[0, 1]):
            result, _ = partition_shift(lhs, lhs, [], 500, Model())
        self.assertEqual(result["status"], "unknown")


class RuleTests(unittest.TestCase):
    def verify(self, source):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "rules.isle"
            path.write_text(source)
            return verify_file(path, 5000)

    def test_actual_source_is_checked_after_edit(self):
        before = self.verify("(rule (rewrite (Op.Sub (bnot x) (bnot y))) (Op.Sub y x))")
        after = self.verify("(rule (rewrite (Op.Sub (bnot x) (bnot y))) (Op.Sub x y))")
        self.assertEqual(before["rules"][0]["status"], "proved")
        self.assertEqual(after["rules"][0]["status"], "counterexample")
        self.assertNotEqual(before["source_sha256"], after["source_sha256"])

    def test_instruction_selection_drift_fails_closed(self):
        selection = (ISLE / "select.isle").read_text()
        for changed in (selection.replace("$ADD)", "$SUB)"),
                        selection.replace("Op.Add _ _", "Op.Add x x"),
                        selection.replace("OpcodeLowering.Binary $ADD", "OpcodeLowering.Store $ADD")):
            context = Context(changed)
            with self.assertRaises(Unsupported):
                context.pattern(("Op.Add", "x", "y"))

    def test_overflow_blocks_integer_cancellation(self):
        report = self.verify("""(rule (rewrite (Op.Div (mul x (iconst c)) (iconst c)))
          (if-let true (u256_eq c 2)) (Op.Add x (imm (u256 0))))""")
        self.assertEqual(report["rules"][0]["status"], "counterexample")

    def test_shift_cancellation_requires_a_lossless_input(self):
        guard = "(if-let true (mask_covers (u256_shr shift (u256_max)) x))"
        source = f"""(rule (rewrite (Op.Shr (iconst shift) (shl (iconst shift) x)))
          (if-let true (u256_lt shift 256)) {guard}
          (Op.Add x (imm (u256 0))))"""
        self.assertEqual(self.verify(source)["rules"][0]["status"], "proved")
        result = self.verify(source.replace(guard, ""))["rules"][0]
        self.assertEqual(result["status"], "counterexample")
        self.assertTrue(result["replayed"])

    def test_shifted_comparison_requires_constant_alignment(self):
        guard = "(if-let true (u256_same (u256_shl shift (u256_shr shift c)) c))"
        for op in ("Eq", "Lt", "Gt"):
            source = f"""(rule (rewrite (Op.{op} (shl (iconst shift) x) (iconst c)))
              (if-let true (u256_lt shift 256)) {guard}
              (if-let true (mask_covers (u256_shr shift (u256_max)) x))
              (Op.{op} x (imm (u256_shr shift c))))"""
            self.assertEqual(self.verify(source)["rules"][0]["status"], "proved")
            # Gt needs no alignment guard for floor division; Eq and Lt do.
            if op != "Gt":
                result = self.verify(source.replace(guard, ""))["rules"][0]
                self.assertEqual(result["status"], "counterexample")
                self.assertTrue(result["replayed"])

    def test_distinct_ssa_ids_can_hold_equal_words(self):
        report = self.verify("""(rule (rewrite (Op.Eq a b))
          (if-let true (differ a b)) (Op.Add (imm (u256 0)) (imm (u256 0))))""")
        rule = report["rules"][0]
        self.assertEqual(rule["status"], "counterexample")
        self.assertEqual(rule["inputs"]["a"], rule["inputs"]["b"])

    def test_guard_and_pattern_semantics(self):
        report = self.verify("""(rule (simplify (Op.IsZero (iszero (and x (bool_value))))) x)
          (rule (rewrite (Op.Sub x x)) (if-let false (u256_eq (u256 1) 1))
             (Op.Add x (imm (u256 0))))""")
        self.assertEqual([r["status"] for r in report["rules"]], ["proved", "inapplicable"])
        self.assertTrue(report["rules"][0]["contracts"])

    def test_unknown_terms_and_bare_if_are_not_skipped(self):
        report = self.verify("""(rule (rewrite (Op.Unknown x)) (Op.Add x x))
          (rule (rewrite (Op.Sub x x)) (if (u256_is_zero x)) (Op.Add x x))""")
        self.assertEqual([r["status"] for r in report["rules"]], ["unsupported", "unsupported"])

    def test_reader_rejects_truncation_and_empty_rule_files(self):
        for source in ("(rule", ")", "bare"):
            with self.assertRaises(ValueError):
                forms(source)
        with self.assertRaises(ValueError):
            self.verify(";; no proof obligations\n")

    def test_constant_slice_constructors_use_evm_operand_order(self):
        for op in ("Shl", "Shr", "Byte"):
            source = f"""(rule (rewrite (Op.{op} (iconst index) (iconst value)))
              (Op.Add (imm (u256_{op.lower()} index value)) (imm (u256 0))))"""
            self.assertEqual(self.verify(source)["rules"][0]["status"], "proved")
            reversed_source = source.replace(f"u256_{op.lower()} index value", f"u256_{op.lower()} value index")
            rule = self.verify(reversed_source)["rules"][0]
            self.assertEqual(rule["status"], "counterexample")
            self.assertTrue(rule["replayed"])

    def test_byte_index_guard_prevents_wrapping_into_word(self):
        guard = "(if-let true (u256_lt index 32))"
        source = f"""(rule (rewrite (Op.Byte (iconst index) (shl (iconst shift) x)))
          (if-let true (u256_eq shift 8)) {guard}
          (Op.Byte (imm (u256_add index (u256 1))) x))"""
        self.assertEqual(self.verify(source)["rules"][0]["status"], "proved")
        rule = self.verify(source.replace(guard, ""))["rules"][0]
        self.assertEqual(rule["status"], "counterexample")
        self.assertTrue(rule["replayed"])
        self.assertGreaterEqual(int(rule["inputs"]["index"], 16), 32)


class CliTests(unittest.TestCase):
    def test_verification_status_and_failure_diagnostics(self):
        for status in ("proved", "unknown", "counterexample", "unsupported", "inapplicable"):
            with self.subTest(status=status), tempfile.TemporaryDirectory() as directory:
                output = Path(directory) / "proofs.json"
                rule = {"line": 84, "status": status}
                if status == "unknown":
                    rule["reason"] = "timeout"
                files = {"source": "rules.isle", "rules": [rule]}
                stdout, stderr = io.StringIO(), io.StringIO()
                with (patch("sys.argv", ["verify_evm_rules.py", "verify", "rules.isle", "--output", str(output)]),
                      patch("verify_evm_rules.verify_file", return_value=files),
                      redirect_stdout(stdout), redirect_stderr(stderr)):
                    code = main()
                self.assertEqual(code, 0 if status == "proved" else 1)
                reason = ": timeout" if status == "unknown" else ""
                expected = "" if status == "proved" else f"rules.isle:84: {status}{reason}\n"
                self.assertEqual(stderr.getvalue(), expected)
                self.assertEqual(json.loads(stdout.getvalue()), {status: 1})
                self.assertEqual(json.loads(output.read_text())["counts"], {status: 1})


class DiscoveryTests(unittest.TestCase):
    def test_deep_seeds_search_a_small_replacement_frontier(self):
        x, y = Expr.var("x"), Expr.var("y")
        seed = expression("sub", expression("or", x, y), expression("and", x, y))
        rules, summary = enumerate_rules(Prices("osaka"), ["x", "y"], ["xor"], 1, 20, 5000, seeds=[seed])
        self.assertIn((seed, expression("xor", x, y)), [(lhs, rhs) for lhs, rhs, _, _ in rules])
        self.assertEqual(summary["seeds_proved"], 1)
        self.assertLess(summary["expressions"], 20)

    def test_seed_sample_collisions_and_unknowns_are_not_proofs(self):
        x = Expr.var("x")
        seed = expression("add", x, 1)
        initial = [{"x": 0}]
        rules, summary = enumerate_rules(Prices("osaka"), ["x"], ["not"], 1, 20, 5000,
                                         initial_samples=initial, seeds=[seed])
        self.assertFalse(any(lhs == seed for lhs, _, _, _ in rules))
        self.assertGreater(summary["counterexamples"], 0)
        self.assertGreater(summary["samples"], 1)
        self.assertEqual(initial, [{"x": 0}])
        with patch("evm_rules.discovery.check", return_value=({"status": "unknown"}, "")):
            rules, summary = enumerate_rules(Prices("osaka"), ["x"], ["not"], 1, 20, 5000,
                                             seeds=[expression("not", expression("not", x))])
        self.assertFalse(rules)
        self.assertGreater(summary["unknown"], 0)

    def test_seed_reader_rejects_invalid_or_unbounded_inputs(self):
        deep = "x"
        for _ in range(17):
            deep = ["not", deep]
        invalid = [[], [True], ["x"], [["add", "x"]], [["not", "y"]],
                   [["not", 123456789]], [["storage", "x"]], [deep], [["not", "x"]] * 129]
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "seeds.json"
            for rows in invalid:
                path.write_text(json.dumps(rows))
                with self.subTest(rows=rows), self.assertRaises(ValueError):
                    read_seeds(path, Prices("osaka"), ["x"])
            path.write_text(json.dumps([["not", "x"], ["not", "x"]]))
            self.assertEqual(read_seeds(path, Prices("osaka"), ["x"]), [expression("not", Expr.var("x"))])

    def test_seed_cli_proves_and_documents_emitted_source(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "seeds.json"
            path.write_text(json.dumps([["sub", ["or", "x", "y"], ["and", "x", "y"]]]))
            output = Path(directory) / "candidates.isle"
            report = discover_rules(SimpleNamespace(
                runs=200, max_rules=32, evm_version="osaka", objective="gas", seed_expressions=path,
                variables=["x", "y"], ops=["xor"], max_ops=1, max_expressions=20,
                timeout_ms=5000, include_constants=False, emit_isle=output))
            self.assertTrue(report["accepted"])
            self.assertEqual(report["summary"]["seeds_proved"], 1)
            self.assertEqual(len(report["seeds_sha256"]), 64)
            self.assertIn(";; ((x | y) - (x & y)) => (x ^ y)", output.read_text())

    def test_empty_search_does_not_leave_stale_candidates(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "candidates.isle"
            output.write_text("stale rule")
            report = discover_rules(SimpleNamespace(
                runs=200, max_rules=32, evm_version="osaka", objective="gas",
                variables=["x"], ops=["not"], max_ops=1, max_expressions=10,
                timeout_ms=5000, include_constants=False, emit_isle=output))
            self.assertEqual(report["emitted_verification"]["status"], "no_candidates")
            self.assertEqual(forms(output.read_text()), [])

    def test_search_bounds_and_variable_validation(self):
        for variables in (["x", "x"], ["_"], ["true"], ["x)"]):
            with self.assertRaises(ValueError):
                enumerate_rules(Prices("osaka"), variables, ["not"], 1, 20, 5000)

    def test_cheaper_proved_representative_survives(self):
        rules, _ = enumerate_rules(Prices("osaka"), ["x", "y"], ["xor", "not"], 3, 1000, 5000,
                                    include_constants=True)
        self.assertTrue(any(rhs.op == "not" for _, rhs, _, _ in rules))

    def test_target_snapshot_and_economic_objectives(self):
        old, new = Prices("homestead"), Prices("osaka")
        self.assertNotIn("shl", old.ops)
        self.assertIn("shl", new.ops)
        self.assertLess(new.constants[0].gas, old.constants[0].gas)
        small, fast = Cost(5, 1), Cost(3, 8)
        self.assertLess(Prices("osaka", "lifetime", 0).key(small), Prices("osaka", "lifetime", 0).key(fast))
        self.assertGreater(Prices("osaka", "lifetime", 10000).key(small), Prices("osaka", "lifetime", 10000).key(fast))

    def test_sampling_collision_requires_smt_and_refines(self):
        _, summary = enumerate_rules(Prices("osaka"), ["x"], ["not"], 1, 20, 5000,
                                     initial_samples=[{"x": 0}])
        self.assertGreater(summary["counterexamples"], 0)
        self.assertGreater(summary["samples"], 1)

    def test_emitted_candidate_round_trips_through_actual_isle(self):
        x, y = Expr.var("x"), Expr.var("y")
        lhs = expression("xor", expression("or", x, y), expression("and", x, y))
        rhs = expression("xor", x, y)
        form, line = forms(emit_rule(lhs, rhs))[0]
        context = Context()
        actual_lhs, actual_rhs = context.obligation(Rule(form, line, "generated"))
        result, _ = check(actual_lhs, actual_rhs, context.assumptions, model=context.model)
        self.assertEqual(result["status"], "proved")

    def test_multi_operation_discovery_and_emission(self):
        rules, _ = enumerate_rules(Prices("osaka"), ["x", "y"], ["not", "and", "or"],
                                   3, 500, 5000, max_rhs_ops=2)
        recipes = [(lhs, rhs) for lhs, rhs, _, _ in rules if rhs.operators() == 2]
        self.assertTrue(recipes)
        for lhs, rhs in recipes:
            form, line = forms(emit_rule(lhs, rhs))[0]
            self.assertEqual(form[1][0], "sequence_rewrite")
            context = Context()
            left, right = context.obligation(Rule(form, line, "generated"))
            self.assertEqual(check(left, right, context.assumptions, model=context.model)[0]["status"], "proved")

    def test_large_literal_guard_and_recipe_are_verified(self):
        x = Expr.var("x")
        lhs = expression("lt", x, 1 << 160)
        rhs = expression("iszero", expression("shr", 160, x))
        source = emit_rule(lhs, rhs)
        for expected, text in (("proved", source), ("counterexample", source.replace("(u256 160)", "(u256 159)"))):
            form, line = forms(text)[0]
            context = Context()
            left, right = context.obligation(Rule(form, line, "generated"))
            self.assertEqual(check(left, right, context.assumptions, model=context.model)[0]["status"], expected)
        self.assertIn((1 << 160) - 1, Prices("osaka").constants)
        with self.assertRaises(ValueError):
            enumerate_rules(Prices("osaka"), ["x"], ["and"], 2, 20, 5000, constants=[123456789])

    def test_specialized_inputs_keep_canonical_constant_results(self):
        rules, _ = enumerate_rules(Prices("osaka"), ["x"], ["xor"], 2, 30, 5000,
                                   include_constants=True, constants=[255])
        self.assertTrue(any(lhs.variables() and rhs == Expr.const(0) for lhs, rhs, _, _ in rules))


if __name__ == "__main__":
    unittest.main()
