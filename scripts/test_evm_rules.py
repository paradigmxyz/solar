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

from evm_rules.discovery import Cost, Prices, discover_rules, emit_rule, enumerate_rules
from evm_rules.isle import Context, ISLE, Rule, forms, verify_file
from evm_rules.semantics import Expr, MASK, MODULUS, SIGN, Model, Unsupported, check, concrete, partition_shift
from verify_evm_rules import main


def expression(op, *args):
    return Expr(op, tuple(Expr.const(a) if isinstance(a, int) else a for a in args))


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
