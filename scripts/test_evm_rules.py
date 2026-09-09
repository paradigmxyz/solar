# /// script
# requires-python = ">=3.11"
# dependencies = ["z3-solver==4.16.0.0"]
# ///
"""Regression tests for the trusted word model, ISLE reader and discovery gate."""

from contextlib import redirect_stderr, redirect_stdout
import io
import hashlib
import json
from pathlib import Path
from types import SimpleNamespace
import tempfile
import subprocess
import shutil
import unittest
from unittest.mock import patch

import z3

from evm_rules.artifacts import query_manifest, query_paths
from evm_rules.solver import Cvc5, solve_query
from evm_rules.discovery import Cost, Prices, discover_rules, emit_rule, enumerate_rules, read_seeds
from evm_rules.isle import Context, ISLE, Rule, forms, verify_file
from evm_rules.mining import abstract_patterns, mine
from evm_rules.memory import MemoryAddresses
from evm_rules.stack import verify_stack_file
from evm_rules.late import execute as execute_late, verify_late_file
from evm_rules.semantics import Expr, MASK, MODULUS, SIGN, Model, Unsupported, check, concrete, partition_bits, partition_shift, portable_query
from verify_evm_rules import main
from replay_evm_rules import main as replay_main, replay_query, replay_report


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

    def test_symbolic_exponents_with_literal_bases_match_integer_pow(self):
        exponent = Expr.var("exponent")
        for base in (0, 1, 2, 3, SIGN, MASK):
            expr = expression("exp", base, exponent)
            term = Model().eval(expr)
            for value in (0, 1, 2, 255, 256, SIGN, MASK):
                result = z3.simplify(z3.substitute(term, (z3.BitVec("exponent", 256), z3.BitVecVal(value, 256))))
                self.assertEqual(result.as_long(), concrete(expr, {"exponent": value}))

    def test_specialization_requires_the_original_guards(self):
        x = Expr.var("x")
        result, query = check(x, x, model=Model({"x": 0}))
        self.assertEqual(result["status"], "unsupported")
        self.assertIn("not implied", result["reason"])
        replay = z3.SolverFor("QF_BV")
        replay.add(*z3.parse_smt2_string(query))
        self.assertEqual(replay.check(), z3.sat)
        result, _ = check(x, x, [z3.BitVec("x", 256) == 0], model=Model({"x": 0}))
        self.assertEqual(result["status"], "proved")

    def test_partition_preserves_specialization_obligations(self):
        x, n = Expr.var("x"), Expr.var("n")
        lhs = expression("shl", n, x)
        result, _ = partition_shift(lhs, lhs, [], 5000, Model({"x": 0}))
        self.assertEqual(result["status"], "unsupported")

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

    def test_guard_shift_partition_covers_power_of_two_divisors(self):
        x, divisor, n = map(Expr.var, ("x", "divisor", "n"))
        model = Model()
        count = model.eval(n)
        guards = [z3.UGT(count, 0), z3.ULT(count, 256),
                  model.eval(divisor) == model.eval(expression("shl", n, 1))]
        lhs = expression("mod", x, divisor)
        rhs = expression("and", x, expression("sub", divisor, 1))
        result, queries = partition_shift(lhs, rhs, guards, 5000, model)
        self.assertEqual(result["status"], "proved")
        self.assertEqual((result["cases"], len(queries)), (257, 258))
        # The exponent is absent from both expressions and lives only in guards.
        self.assertNotIn("n", lhs.variables() | rhs.variables())
        # Keep the guard and original modulo in the independently replayable query.
        self.assertIn("bvurem", queries[2][1])
        self.assertIn("bvshl", queries[2][1])
        # Without the upper bound, MAX is a counterexample in the final range.
        result, _ = partition_shift(lhs, rhs, [guards[-1], count == MASK], 5000, model)
        self.assertEqual(result["status"], "counterexample")
        self.assertEqual(int(result["inputs"]["divisor"], 16), 0)
        self.assertTrue(result["replayed"])

    def test_multiple_indices_remain_independent(self):
        x, a, b = map(Expr.var, ("x", "a", "b"))
        lhs = expression("signextend", a, expression("shl", b, x))
        result, queries = partition_shift(lhs, lhs, [], 5000, Model())
        self.assertEqual(result["status"], "proved")
        # Select the 32-case SIGNEXTEND domain, not the 257-case shift domain.
        self.assertEqual((result["cases"], len(queries)), (32, 33))
        self.assertIn('"b"', queries[1][1])
        wrong = expression("signextend", a, expression("shl", a, x))
        result, _ = partition_shift(lhs, wrong, [], 5000, Model())
        self.assertEqual(result["status"], "counterexample")
        self.assertNotEqual(result["inputs"]["a"], result["inputs"]["b"])
        self.assertTrue(result["replayed"])

    def test_signextend_partition_keeps_the_whole_identity_range(self):
        x, n = Expr.var("x"), Expr.var("n")
        rhs = expression("signextend", n, x)
        lhs = expression("signextend", n, rhs)
        result, queries = partition_shift(lhs, rhs, [], 5000, Model())
        self.assertEqual(result["status"], "proved")
        self.assertEqual(result["proof_method"], "exhaustive-word-index-partition")
        self.assertEqual((result["cases"], len(queries)), (32, 33))
        # The error exists only at MAX, not at the range's first index, 31.
        wrong = expression("select", expression("eq", n, MASK), expression("not", rhs), rhs)
        result, _ = partition_shift(lhs, wrong, [], 5000, Model())
        self.assertEqual(result["status"], "counterexample")
        self.assertEqual(int(result["inputs"]["n"], 16), MASK)
        self.assertTrue(result["replayed"])

    def test_shared_shift_and_signextend_index_uses_the_larger_boundary(self):
        x, n = Expr.var("x"), Expr.var("n")
        lhs = expression("shl", n, expression("signextend", n, x))
        result, queries = partition_shift(lhs, lhs, [], 5000, Model())
        self.assertEqual(result["status"], "proved")
        self.assertEqual((result["cases"], len(queries)), (257, 258))

    def test_signextend_encoding_matches_shift_definition_exhaustively(self):
        index, value = z3.BitVecs("index value", 256)
        shift = 248 - index * 8
        old = z3.If(z3.ULT(index, z3.BitVecVal(31, 256)), (value << shift) >> shift, value)
        new = Model.apply("signextend", (index, value))
        cases = [index == n for n in range(31)] + [z3.UGE(index, 31)]
        for condition in cases:
            solver = z3.SolverFor("QF_BV")
            solver.set(timeout=5000)
            solver.add(condition, old != new)
            self.assertEqual(solver.check(), z3.unsat, condition)
        # Coverage itself must hold, independently of equality of the models.
        solver = z3.SolverFor("QF_BV")
        solver.add(z3.Not(z3.Or(cases)))
        self.assertEqual(solver.check(), z3.unsat)

    def test_signextend_wrong_zero_extension_replays(self):
        value = Expr.var("value")
        result, _ = check(expression("signextend", 0, value), expression("and", value, 255))
        self.assertEqual(result["status"], "counterexample")
        self.assertTrue(result["replayed"])


class OutputBitPartitionTests(unittest.TestCase):
    def test_every_bit_keeps_the_original_guards_and_inputs(self):
        x, y = map(Expr.var, ("x", "y"))
        guards = [z3.BitVec("x", 256) == z3.BitVec("y", 256)]
        result, queries = partition_bits(x, y, guards, 5000, Model())
        self.assertEqual(result, {"status": "proved", "proof_method": "exhaustive-output-bit-partition", "bits": 256})
        self.assertEqual([name for name, _ in queries], [f"bit-{i}" for i in range(256)])
        for _, query in queries:
            self.assertIn('"x"', query)
            self.assertIn('"y"', query)
            solver = z3.SolverFor("QF_BV")
            solver.add(*z3.parse_smt2_string(query))
            self.assertEqual(solver.check(), z3.unsat)
        # Removing the guard exposes a full-word, independently replayed witness.
        result, _ = partition_bits(x, y, [], 5000, Model())
        self.assertEqual(result["status"], "counterexample")
        self.assertTrue(result["replayed"])

    def test_last_bit_and_large_shift_counts_are_not_omitted(self):
        x, n = map(Expr.var, ("x", "n"))
        lhs = expression("shr", n, x)
        wrong = expression("select", expression("eq", n, MASK), expression("xor", lhs, SIGN), lhs)
        result, queries = partition_bits(lhs, wrong, [], 5000, Model())
        self.assertEqual(len(queries), 256)
        self.assertEqual(result["status"], "counterexample")
        self.assertEqual(int(result["inputs"]["n"], 16), MASK)
        self.assertTrue(result["replayed"])

    def test_specialization_still_requires_guards(self):
        x = Expr.var("x")
        result, _ = partition_bits(x, x, [], 5000, Model({"x": 0}))
        self.assertEqual(result["status"], "unsupported")
        result, queries = partition_bits(x, x, [z3.BitVec("x", 256) == 0], 5000, Model({"x": 0}))
        self.assertEqual(result["status"], "proved")
        self.assertEqual(len(queries), 256)

    def test_budget_exhaustion_keeps_a_partial_prefix_unproved(self):
        x = Expr.var("x")
        with patch("evm_rules.semantics.time.monotonic", side_effect=[0, 0, 1]):
            result, queries = partition_bits(x, x, [], 500, Model())
        self.assertEqual(result["status"], "unknown")
        self.assertEqual(len(queries), 1)

    def test_verifier_replaces_incomplete_index_partitions_with_all_bits(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "rules.isle"
            path.write_text("(rule (simplify (Op.Add a (zero))) a)")
            with (patch("evm_rules.isle.check", return_value=({"status": "unknown"}, "original-query")),
                  patch("evm_rules.isle.partition_shift", return_value=(
                      {"status": "unknown"}, [("case-0", "partial-query")]))):
                report = verify_file(path, 100, Path(directory) / "smt", bit_partition_timeout_ms=5000)
            rule = report["rules"][0]
            self.assertEqual(rule["status"], "proved")
            self.assertEqual(len(query_paths({"files": [report]}, require_proved=True)), 256)
            self.assertTrue(all("-bit-" in name for name in rule["smt2"]))

    def test_bits_never_hide_definitive_failures_or_unknown_applicability(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "rules.isle"
            path.write_text("(rule (simplify (Op.Add a (zero))) a)")
            for status, query in (("counterexample", "q"), ("unsupported", "q"),
                                  ("inapplicable", ""), ("unknown", "")):
                with (self.subTest(status=status),
                      patch("evm_rules.isle.check", return_value=({"status": status}, query)),
                      patch("evm_rules.isle.partition_bits") as bits):
                    rule = verify_file(path, 100, bit_partition_timeout_ms=5000)["rules"][0]
                    self.assertEqual(rule["status"], status)
                    bits.assert_not_called()
            for status in ("sat", "error"):
                with (self.subTest(fallback=status),
                      patch("evm_rules.isle.check", return_value=({"status": "unknown"}, "q")),
                      patch("evm_rules.isle.partition_bits") as bits):
                    fallback = SimpleNamespace(solve=lambda _: {"status": status})
                    rule = verify_file(path, 100, fallback=fallback, bit_partition_timeout_ms=5000)["rules"][0]
                    self.assertEqual(rule["status"], "unknown")
                    bits.assert_not_called()

    def test_failed_bit_attempt_preserves_the_whole_query_and_fallback(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "rules.isle"
            path.write_text("(rule (simplify (Op.Add a (zero))) a)")
            fallback = SimpleNamespace(solve=lambda _: {"status": "timeout"})
            with (patch("evm_rules.isle.check", return_value=({"status": "unknown"}, "whole-query")),
                  patch("evm_rules.isle.partition_bits", return_value=(
                      {"status": "unknown"}, [("bit-0", "partial-bit-query")]))):
                rule = verify_file(path, 100, Path(directory) / "smt", fallback=fallback,
                                   bit_partition_timeout_ms=5000)["rules"][0]
            self.assertEqual(rule["status"], "unknown")
            self.assertEqual(rule["fallback"]["status"], "timeout")
            self.assertEqual([Path(name).read_text() for name in rule["smt2"]],
                             ["partial-bit-query", "whole-query"])

    def test_manifest_rejects_missing_bits_and_shorter_word_widths(self):
        queries = [f"bit-{i}.smt2" for i in range(256)]
        rule = {"status": "proved", "proof_method": "exhaustive-output-bit-partition", "bits": 256, "smt2": queries}
        self.assertEqual(query_paths({"files": [{"rules": [rule]}]}, require_proved=True), queries)
        for bits, count in ((255, 255), (256, 255), (255, 256), (257, 256)):
            rule.update(bits=bits, smt2=queries[:count])
            with self.subTest(bits=bits, count=count), self.assertRaisesRegex(ValueError, "256 output bits"):
                query_paths({"files": [{"rules": [rule]}]}, require_proved=True)


class EnvironmentTests(unittest.TestCase):
    def test_actual_balance_mask_rules_require_all_address_bits(self):
        path = ISLE / "egraph.isle"
        rules = [Rule(form, line, str(path)) for form, line in forms(path.read_text())
                 if form[0] == "rule" and "Op.Balance" in repr(form) and "band" in repr(form)]
        self.assertEqual(len(rules), 2)
        for rule in rules:
            with self.subTest(line=rule.line):
                cx = Context()
                lhs, rhs = cx.obligation(rule)
                self.assertEqual(check(lhs, rhs, cx.assumptions, 5000, cx.model)[0]["status"], "proved")
                # Dropping the low-bit guard must expose a different account balance.
                broken = Rule([part for part in rule.form if "u256_is_zero" not in repr(part)],
                              rule.line, rule.source)
                cx = Context()
                lhs, rhs = cx.obligation(broken)
                result, _ = check(lhs, rhs, cx.assumptions, 5000, cx.model)
                self.assertEqual(result["status"], "counterexample")
                self.assertTrue(result["replayed"])

    def test_actual_self_balance_rule_uses_a_shared_state_array(self):
        path = ISLE / "egraph.isle"
        rules = [Rule(form, line, str(path)) for form, line in forms(path.read_text())
                 if form[0] == "rule" and "current_address" in repr(form)]
        self.assertEqual(len(rules), 1)
        cx = Context()
        lhs, rhs = cx.obligation(rules[0])
        result, query = check(lhs, rhs, cx.assumptions, 5000, cx.model)
        self.assertEqual(result["status"], "proved")
        self.assertIn("(set-logic QF_ABV)", query)
        self.assertIn("(Array (_ BitVec 160) (_ BitVec 256))", query)
        solver = z3.SolverFor("QF_ABV")
        solver.add(*z3.parse_smt2_string(query))
        self.assertEqual(solver.check(), z3.unsat)

    def test_wrong_account_replays_the_snapshot(self):
        lhs = expression("balance", Expr.var("other"))
        rhs = expression("selfbalance")
        result, _ = check(lhs, rhs)
        self.assertEqual(result["status"], "counterexample")
        self.assertTrue(result["replayed"])
        state = result["environment"]
        environment = {"address": int(state["address"], 16),
                       "balances": {int(k, 16): int(v, 16) for k, v in state["balances"].items()}}
        values = {k: int(v, 16) for k, v in result["inputs"].items()}
        self.assertNotEqual(values["other"] & ((1 << 160) - 1), environment["address"])
        self.assertEqual(concrete(lhs, values, environment), int(result["lhs_value"], 16))
        self.assertEqual(concrete(rhs, values, environment), int(result["rhs_value"], 16))

    def test_balance_addresses_truncate_to_160_bits(self):
        address = Expr.var("account")
        lhs = expression("balance", address)
        rhs = expression("balance", expression("and", address, (1 << 160) - 1))
        self.assertEqual(check(lhs, rhs)[0]["status"], "proved")
        environment = {"address": 7, "balances": {7: MASK, (1 << 160) - 1: 19}}
        self.assertEqual(concrete(expression("balance", (1 << 160) + 7), {}, environment), MASK)
        self.assertEqual(concrete(expression("balance", MASK), {}, environment), 19)
        self.assertEqual(concrete(expression("balance", 8), {}, environment), 0)
        self.assertEqual(concrete(expression("address"), {}, environment), 7)
        self.assertEqual(concrete(expression("selfbalance"), {}, environment), MASK)

    def test_nested_reads_collect_every_observed_account(self):
        lhs = expression("balance", expression("balance", Expr.var("account")))
        result, _ = check(lhs, expression("not", lhs))
        self.assertEqual(result["status"], "counterexample")
        self.assertTrue(result["replayed"])
        self.assertIn("environment", result)

    def test_array_queries_work_in_bit_and_shift_partitions(self):
        lhs = expression("shl", Expr.var("n"), expression("balance", expression("address")))
        rhs = expression("shl", Expr.var("n"), expression("selfbalance"))
        for partition in (partition_bits, partition_shift):
            result, queries = partition(lhs, rhs, [], 5000, Model())
            self.assertEqual(result["status"], "proved")
            self.assertTrue(any("QF_ABV" in query for _, query in queries))

    def test_state_changes_and_unknown_environment_operations_stay_unsupported(self):
        for op in ("call", "sstore", "selfdestruct", "origin"):
            with self.assertRaises(Unsupported):
                Model().eval(expression(op))
        with self.assertRaises(Unsupported):
            concrete(expression("selfbalance"), {})
        with self.assertRaises(Unsupported):
            Context().pattern("@environment:address")
        source = (ISLE / "select.isle").read_text().replace("$ADDRESS)", "$ORIGIN)")
        with self.assertRaises(Unsupported):
            Context(selection_source=source).pattern(("current_address",))

    def test_reader_rejects_snapshot_assumptions_about_earlier_producers(self):
        # This equality holds in one snapshot, but the two producer instructions
        # could straddle a call. The rule has no state-clobber guard.
        source = "(rule (simplify (Op.Sub (balance (current_address)) (selfbalance))) (imm 0))"
        form, line = forms(source)[0]
        with self.assertRaisesRegex(Unsupported, "instruction roots"):
            Context().obligation(Rule(form, line, "nested.isle"))

    def test_non_balance_array_sorts_and_uninterpreted_functions_are_rejected(self):
        array = z3.Array("unsupported", z3.IntSort(), z3.BitVecSort(256))
        solver = z3.Solver()
        solver.add(z3.Select(array, 0) == 1)
        with self.assertRaises(Unsupported):
            portable_query(solver)
        function = z3.Function("unknown", z3.BitVecSort(256), z3.BitVecSort(256))
        solver = z3.Solver()
        solver.add(function(z3.BitVecVal(0, 256)) == 1)
        with self.assertRaises(Unsupported):
            portable_query(solver)


class MemoryAddressTests(unittest.TestCase):
    def test_actual_projection_rules_and_missing_guards(self):
        path = ISLE / "egraph.isle"
        rules = [Rule(form, line, str(path)) for form, line in forms(path.read_text())
                 if form[0] == "rule" and any(name in repr(form) for name in MemoryAddresses.SHAPES)]
        self.assertEqual(len(rules), 3)
        for rule in rules:
            cx = Context()
            lhs, rhs = cx.obligation(rule)
            result, _ = check(lhs, rhs, cx.assumptions, 5000, cx.model)
            self.assertEqual(result["status"], "proved", rule.line)
            # Removing the actual source guard must expose a nonzero header
            # or field offset, rather than implicitly assuming the rewrite.
            unguarded = Rule((rule.form[0], rule.form[1], rule.form[-1]), rule.line, rule.source)
            cx = Context()
            lhs, rhs = cx.obligation(unguarded)
            result, _ = check(lhs, rhs, cx.assumptions, 5000, cx.model)
            self.assertEqual(result["status"], "counterexample", rule.line)
            self.assertTrue(result["replayed"])

    def assert_concrete_and_symbolic(self, value, values, expected):
        term = Model().eval(value)
        bindings = [(z3.BitVec(name, 256), z3.BitVecVal(number, 256)) for name, number in values.items()]
        self.assertEqual(z3.simplify(z3.substitute(term, *bindings)).as_long(), expected)
        self.assertEqual(concrete(value, values), expected)

    def test_data_headers_and_slice_payload_pointers(self):
        cx = Context()
        object, kind = map(Expr.var, ("object", "kind"))
        value = cx.operation("Op.MemoryObjectData", [object, kind])
        flag = cx.memory.slices[object].args[0]
        for tag, header in ((0, 32), (1, 32), (2, 0), (3, 0)):
            for is_slice in (0, 1):
                for pointer in (0, 128, MASK):
                    self.assert_concrete_and_symbolic(value, {"object": pointer, "kind": tag, flag: is_slice},
                                                      (pointer + (0 if is_slice else header)) & MASK)
        # Adding a header to a slice pointer is wrong, even for dynamic data.
        wrong = expression("add", object, cx.memory.data_offset(kind))
        result, _ = check(value, wrong, cx.assumptions, 5000, cx.model)
        self.assertEqual(result["status"], "counterexample")
        self.assertEqual(int(result["inputs"][flag], 16), 1)

    def test_field_offsets_saturate_before_full_word_address_addition(self):
        cx = Context()
        object, layout, field = map(Expr.var, ("object", "layout", "field"))
        value = cx.operation("Op.MemoryObjectFieldAddr", [object, layout, field])
        for index in (0, 1, (1 << 59) - 1, 1 << 59, (1 << 64) - 2):
            expected_offset = min(index * 32, (1 << 64) - 1)
            self.assert_concrete_and_symbolic(value, {"object": MASK, "field": index},
                                              (MASK + expected_offset) & MASK)
        # Fields outside a struct's declared range have no valid projection.
        shape = cx.memory.layouts[layout]
        result, _ = check(value, value, [*cx.assumptions, cx.model.eval(field) == cx.model.eval(shape.fields)])
        self.assertEqual(result["status"], "inapplicable")

    def test_element_strides_keep_zero_full_u32_and_wrapping_indices(self):
        cx = Context()
        object, layout, index = map(Expr.var, ("object", "layout", "index"))
        value = cx.operation("Op.MemoryObjectElementAddr", [object, layout, index])
        shape = cx.memory.layouts[layout]
        for tag in range(3):
            for words in (0, 1, (1 << 32) - 1):
                for i in (0, 1, MASK):
                    values = {"object": MASK, "index": i, shape.kind.args[0]: tag, shape.element_words.args[0]: words}
                    stride = 32 if tag == 0 else words * 32
                    expected = (MASK + (32 if tag < 2 else 0) + i * stride) & MASK
                    self.assert_concrete_and_symbolic(value, values, expected)
        result, _ = check(value, value, [*cx.assumptions, cx.model.eval(shape.kind) == 3])
        self.assertEqual(result["status"], "inapplicable")

    def test_generated_schema_drift_and_wrong_arity_fail_closed(self):
        cx = Context()
        object, kind = map(Expr.var, ("object", "kind"))
        with patch("evm_rules.isle.forms", return_value=[(("type", "Op", "extern", (
                "enum", ("MemoryObjectData", ("kind", "MemoryObjectKind"), ("object", "Value")))), 1)]):
            with self.assertRaisesRegex(Unsupported, "changed memory address schema"):
                cx.operation("Op.MemoryObjectData", [object, kind])
        with self.assertRaises(Unsupported):
            cx.operation("Op.MemoryObjectData", [object])
        cx.memory.kind(kind)
        with self.assertRaises(Unsupported):
            cx.memory.layout(kind)


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

    def test_exp_specializes_only_proved_guard_constants(self):
        source = """(rule (rewrite (Op.Exp a (iconst c)))
          (if-let true (u256_eq c 2)) (Op.Mul a a))"""
        rule = self.verify(source)["rules"][0]
        self.assertEqual(rule["status"], "proved")
        self.assertEqual(rule["constant_specializations"], {"c": "0x2"})
        wrong = source.replace("u256_eq c 2", "u256_eq c 3").replace(
            "(Op.Mul a a)", "(if-let true (u256_eq a 2)) (Op.Mul a a)")
        rule = self.verify(wrong)["rules"][0]
        self.assertEqual(rule["status"], "counterexample")
        self.assertEqual(rule["inputs"]["c"], "0x3")
        self.assertTrue(rule["replayed"])
        for changed in (source.replace("(if-let true (u256_eq c 2))", ""),
                        source.replace("if-let true", "if-let false")):
            self.assertEqual(self.verify(changed)["rules"][0]["status"], "unsupported")
        source = "(rule (simplify (Op.Exp (and a (one)) _)) a)"
        rule = self.verify(source)["rules"][0]
        self.assertEqual(rule["status"], "proved")
        self.assertEqual(rule["constant_specializations"], {"a": "0x1"})

    def test_actual_compiled_exp_rules(self):
        def uses_exp(node):
            return node == "Op.Exp" or isinstance(node, tuple) and any(uses_exp(child) for child in node)
        path = ISLE / "egraph.isle"
        rules = [Rule(form, line, str(path)) for form, line in forms(path.read_text())
                 if form[0] == "rule" and uses_exp(form)]
        self.assertEqual(len(rules), 4)
        for rule in rules:
            context = Context()
            lhs, rhs = context.obligation(rule)
            result, _ = check(lhs, rhs, context.assumptions, model=context.model)
            self.assertEqual(result["status"], "proved", (rule.line, result))

    def test_actual_power_of_two_remainder_rule(self):
        def contains(node, atom):
            return node == atom or isinstance(node, tuple) and any(contains(child, atom) for child in node)
        path = ISLE / "egraph.isle"
        rules = [Rule(form, line, str(path)) for form, line in forms(path.read_text())
                 if form[0] == "rule" and contains(form, "Op.Mod") and contains(form, "power_of_two_shift")]
        self.assertEqual(len(rules), 1)
        context = Context()
        lhs, rhs = context.obligation(rules[0])
        applicability = z3.SolverFor("QF_BV")
        applicability.set(timeout=5000)
        applicability.add(*context.assumptions)
        self.assertEqual(applicability.check(), z3.sat)
        result, queries = partition_shift(lhs, rhs, context.assumptions, 5000, context.model)
        self.assertEqual(result["status"], "proved", (rules[0].line, result))
        self.assertEqual((result["cases"], len(queries)), (257, 258))

    def test_actual_nested_signextend_rule(self):
        path = ISLE / "egraph.isle"
        def has_nested_signextend(node):
            return isinstance(node, tuple) and (
                len(node) == 3 and node[0] == "Op.SignExtend"
                and isinstance(node[2], tuple) and node[2][0] == "signextend"
                or any(has_nested_signextend(child) for child in node))
        rules = [Rule(form, line, str(path)) for form, line in forms(path.read_text())
                 if form[0] == "rule" and has_nested_signextend(form)]
        self.assertEqual(len(rules), 1)
        cx = Context()
        lhs, rhs = cx.obligation(rules[0])
        result, _ = check(lhs, rhs, cx.assumptions, 5000, cx.model)
        self.assertEqual(result["status"], "proved", result)

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

    def test_clz_requires_the_known_sign_bit_contract(self):
        guard = "(if-let true (has_known_sign_bit a))"
        source = f"(rule (simplify (Op.Clz a)) {guard} (imm (u256 0)))"
        rule = self.verify(source)["rules"][0]
        self.assertEqual(rule["status"], "proved")
        self.assertTrue(any("has_known_sign_bit" in contract for contract in rule["contracts"]))
        for changed in (source.replace(guard, ""),
                        source.replace(guard, "(if-let false (has_known_sign_bit a))"),
                        source.replace("(u256 0)", "(u256 1)")):
            rule = self.verify(changed)["rules"][0]
            self.assertEqual(rule["status"], "counterexample")
            self.assertTrue(rule["replayed"])
        for constructor in ("has_known_sign_bit", "has_known_sign_bit a a",
                            "is_zero_or_one a a", "below_const a", "below_const a a a"):
            changed = source.replace("has_known_sign_bit a", constructor)
            rule = self.verify(changed)["rules"][0]
            self.assertEqual(rule["status"], "unsupported")
            self.assertIn("arity", rule["reason"])

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


class ProofArtifactTests(unittest.TestCase):
    def test_portable_names_preserve_sorts_and_avoid_collisions(self):
        a = z3.BitVec("@fresh:1", 256)
        b = z3.BitVec("solar_query_0", 256)
        flag = z3.Bool("@fresh:1")
        solver = z3.SolverFor("QF_BV")
        solver.add(a == 1, b == 2, flag)
        source = portable_query(solver)
        self.assertTrue(source.startswith("(set-logic QF_BV)\n"))
        self.assertNotIn("(declare-fun |@", source)
        replay = z3.SolverFor("QF_BV")
        replay.add(*z3.parse_smt2_string(source))
        self.assertEqual(replay.check(), z3.sat)
        # The live solver still uses the original witness names.
        self.assertEqual(solver.check(), z3.sat)
        self.assertEqual(solver.model().eval(a).as_long(), 1)
        self.assertEqual(solver.model().eval(b).as_long(), 2)
        solver.add(a == b)
        replay.reset()
        replay.add(*z3.parse_smt2_string(portable_query(solver)))
        self.assertEqual(replay.check(), z3.unsat)

    def test_forced_partition_records_coverage_and_all_counts(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "shift.isle"
            path.write_text("(rule (rewrite (Op.Shl n x)) (Op.Shl n x))")
            report = verify_file(path, 5000, Path(directory) / "smt", partition_shifts=True)
            rule = report["rules"][0]
            self.assertEqual(rule["status"], "proved")
            self.assertEqual(rule["proof_method"], "exhaustive-shift-partition")
            self.assertEqual(rule["cases"], 257)
            self.assertEqual(len(rule["smt2"]), 258)
            coverage = z3.SolverFor("QF_BV")
            coverage.add(*z3.parse_smt2_file(rule["smt2"][0]))
            self.assertEqual(coverage.check(), z3.unsat)
            # Export the raw substituted obligation, not a pre-simplified false.
            self.assertIn("bvshl", Path(rule["smt2"][2]).read_text())

    def test_portable_comments_cannot_inject_commands(self):
        solver = z3.SolverFor("QF_BV")
        solver.add(z3.BitVec(".name\n(check-sat)", 256) == 1)
        source = portable_query(solver)
        self.assertEqual(source.count("\n(check-sat)"), 1)
        replay = z3.SolverFor("QF_BV")
        replay.add(*z3.parse_smt2_string(source))
        self.assertEqual(replay.check(), z3.sat)

    def test_manifest_includes_words_partitions_and_stack_variants(self):
        report = {"files": [{"rules": [
            {"status": "proved", "smt2": ["word.smt2"]},
            {"status": "proved", "smt2": ["coverage.smt2", "case.smt2"],
             "proof_method": "exhaustive-shift-partition", "cases": 1},
            {"status": "proved", "variants": [{"status": "proved", "query": "stack.smt2"}]},
        ]}]}
        self.assertEqual(query_paths(report, require_proved=True),
                         ["word.smt2", "coverage.smt2", "case.smt2", "stack.smt2"])
        report["files"][0]["rules"][1]["smt2"].pop()
        for method in ("exhaustive-shift-partition", "exhaustive-word-index-partition"):
            report["files"][0]["rules"][1]["proof_method"] = method
            with self.assertRaisesRegex(ValueError, "partition"):
                query_paths(report, require_proved=True)

    def test_incomplete_reports_never_replay_as_proved(self):
        for rule in ({"status": "unknown"}, {"status": "proved"},
                     {"status": "proved", "variants": []},
                     {"status": "proved", "variants": [{"status": "proved"}]},
                     {"status": "proved", "variants": [{"status": "unknown", "query": "x"}]},
                     {"status": "proved", "smt2": ["x", "x"]},
                     {"status": "proved", "smt2": ["x"], "proof_method": "solver-fallback",
                      "fallback": {"status": "sat"}},
                     {"status": "proved", "smt2": ["x", "y"], "proof_method": "solver-fallback",
                      "fallback": {"status": "unsat"}}):
            with self.subTest(rule=rule), self.assertRaises(ValueError):
                query_paths({"files": [{"rules": [rule]}]}, require_proved=True)
        with self.assertRaises(ValueError):
            query_paths({"files": []}, require_proved=True)
        with self.assertRaises(ValueError):
            query_paths({"files": [{"rules": []}]}, require_proved=True)

    def test_replay_requires_unsat_and_successful_exit(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "query.smt2"
            path.write_text("(set-logic QF_BV)\n(assert false)\n(check-sat)\n")
            digest = hashlib.sha256(path.read_bytes()).hexdigest()
            for output, code, expected in ((b"unsat\n", 0, "unsat"), (b"sat\n", 0, "sat"),
                                           (b"unknown\n", 0, "unknown"), (b"", 0, "error"),
                                           (b"unsat\nunsat\n", 0, "error"),
                                           (b"unsat\n", 1, "error")):
                with patch("replay_evm_rules.subprocess.run", return_value=
                           subprocess.CompletedProcess([], code, output, b"")) as run:
                    result = replay_query(str(path), digest, "cvc5", 100)
                    self.assertEqual(run.call_count, 2 if expected == "unknown" else 1)
                self.assertEqual(result["status"], expected)
            with patch("replay_evm_rules.subprocess.run", side_effect=subprocess.TimeoutExpired([], 1)):
                self.assertEqual(replay_query(str(path), digest, "cvc5", 100)["status"], "timeout")
            with patch("replay_evm_rules.subprocess.run", side_effect=[
                subprocess.TimeoutExpired([], 1),
                subprocess.CompletedProcess([], 0, b"unsat\n", b""),
            ]):
                result = replay_query(str(path), digest, "cvc5", 100)
                self.assertEqual([attempt["status"] for attempt in result["attempts"]], ["timeout", "unsat"])
                self.assertEqual(result["attempts"][1]["flags"], ["--solve-bv-as-int=sum"])
            path.write_text("(assert false)\n")
            with patch("replay_evm_rules.subprocess.run") as run:
                self.assertEqual(replay_query(str(path), digest, "cvc5", 100)["status"], "error")
                run.assert_not_called()

    def test_report_replay_checks_exact_manifest_and_hashes(self):
        with tempfile.TemporaryDirectory() as directory:
            query = Path(directory) / "query.smt2"
            query.write_text("(set-logic QF_BV)\n(assert false)\n(check-sat)\n")
            report = {"schema": "solar:evm-word-rules@1", "word_bits": 256,
                      "files": [{"rules": [{"status": "proved", "smt2": [str(query)]}]}]}
            report["query_sha256"] = query_manifest(report)
            path = Path(directory) / "proofs.json"
            path.write_text(json.dumps(report))
            def run(command, **kwargs):
                if command[-1] == "--version":
                    return subprocess.CompletedProcess(command, 0, "cvc5 test version", "")
                self.assertEqual(kwargs["input"], query.read_bytes())
                return subprocess.CompletedProcess(command, 0, b"unsat\n", b"")
            with (patch("replay_evm_rules.shutil.which", return_value="/test/cvc5"),
                  patch("replay_evm_rules.subprocess.run", side_effect=run)):
                result = replay_report(path, jobs=1)
            self.assertEqual(result["counts"], {"unsat": 1})
            self.assertEqual(result["rule_count"], 1)
            self.assertEqual(result["query_count"], 1)
            self.assertEqual(result["proof_report_sha256"], hashlib.sha256(path.read_bytes()).hexdigest())
            report["query_sha256"]["extra.smt2"] = "0" * 64
            path.write_text(json.dumps(report))
            with self.assertRaisesRegex(ValueError, "exactly every"):
                replay_report(path)

    def test_replay_cli_fails_for_every_non_unsat_status(self):
        for status in ("unsat", "sat", "unknown", "timeout", "error"):
            with self.subTest(status=status), tempfile.TemporaryDirectory() as directory:
                output = Path(directory) / "replay.json"
                with (patch("sys.argv", ["replay_evm_rules.py", "proofs.json", "--output", str(output)]),
                      patch("replay_evm_rules.replay_report", return_value={"counts": {status: 1}, "queries": []}),
                      redirect_stdout(io.StringIO())):
                    code = replay_main()
                self.assertEqual(code, 0 if status == "unsat" else 1)
                self.assertEqual(json.loads(output.read_text())["counts"], {status: 1})


class SolverFallbackTests(unittest.TestCase):
    def test_only_complete_unsat_query_can_replace_partial_partitions(self):
        source = "(rule (simplify (Op.Add a (zero))) a)"
        original = "(set-logic QF_BV)\n(assert false)\n(check-sat)\n"
        for status in ("unsat", "sat", "unknown", "timeout", "error"):
            with self.subTest(status=status), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "rules.isle"
                path.write_text(source)
                def solve(query):
                    self.assertEqual(query, original)
                    return {"status": status, "solver": {"name": "cvc5"},
                            "query_sha256": hashlib.sha256(query.encode()).hexdigest()}
                with (patch("evm_rules.isle.check", return_value=({"status": "unknown"}, original)),
                      patch("evm_rules.isle.partition_shift", return_value=(
                          {"status": "unknown"}, [("case-0", "partial-query")]))):
                    report = verify_file(path, 100, Path(directory) / "smt", fallback=SimpleNamespace(solve=solve))
                rule = report["rules"][0]
                self.assertEqual(rule["status"], "proved" if status == "unsat" else "unknown")
                queries = [Path(p).read_text() for p in rule["smt2"]]
                self.assertEqual(queries, [original] if status == "unsat" else ["partial-query", original])
                if status == "unsat":
                    self.assertEqual(rule["proof_method"], "solver-fallback")
                    self.assertEqual(query_paths({"files": [report]}, require_proved=True), rule["smt2"])

    def test_fallback_never_overrides_a_counterexample_or_inapplicability(self):
        for status in ("counterexample", "inapplicable", "unsupported"):
            with self.subTest(status=status), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "rules.isle"
                path.write_text("(rule (simplify (Op.Add a (zero))) a)")
                def forbidden(query):
                    self.fail("fallback must not override a definitive failure")
                with patch("evm_rules.isle.check", return_value=({"status": status}, "query")):
                    rule = verify_file(path, 100, fallback=SimpleNamespace(solve=forbidden))["rules"][0]
                self.assertEqual(rule["status"], status)

    def test_fallback_does_not_prove_unknown_applicability(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "rules.isle"
            path.write_text("(rule (simplify (Op.Add a (zero))) a)")
            def forbidden(query):
                self.fail("no equality query exists until applicability is established")
            with patch("evm_rules.isle.check", return_value=({"status": "unknown"}, "")):
                rule = verify_file(path, 100, fallback=SimpleNamespace(solve=forbidden))["rules"][0]
            self.assertEqual(rule["status"], "unknown")

    def test_nonzero_sat_exit_cannot_be_retried_as_timeout(self):
        with patch("evm_rules.solver.subprocess.run", return_value=
                   subprocess.CompletedProcess([], 1, b"sat\n", b"cvc5 interrupted by timeout.")) as run:
            self.assertEqual(solve_query(b"query", "cvc5", 100)["status"], "error")
            run.assert_called_once()

    @unittest.skipUnless(shutil.which("cvc5"), "cvc5 is optional for local verifier tests")
    def test_actual_arithmetic_rules_with_cvc5_fallback(self):
        def selected(node):
            if not isinstance(node, tuple):
                return node == "below_const"
            return (len(node) == 3 and node[0] in ("Op.Mod", "Op.SMod") and node[1] == node[2]
                    or any(selected(child) for child in node))
        path = ISLE / "egraph.isle"
        rules = [Rule(form, line, str(path)) for form, line in forms(path.read_text())
                 if form[0] == "rule" and selected(form)]
        self.assertEqual(len(rules), 8)
        fallback = Cvc5(timeout_ms=1000)
        for rule in rules:
            cx = Context()
            lhs, rhs = cx.obligation(rule)
            result, query = check(lhs, rhs, cx.assumptions, 1000, cx.model)
            if result["status"] == "unknown" and query:
                result = fallback.solve(query)
                self.assertEqual(result["status"], "unsat", (rule.line, result))
            else:
                self.assertEqual(result["status"], "proved", (rule.line, result))


class CliTests(unittest.TestCase):
    def test_negative_bit_partition_budget_is_rejected(self):
        with (patch("sys.argv", ["verify_evm_rules.py", "verify", "--output", "unused.json",
                                 "--bit-partition-timeout-ms", "-1"]),
              redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as error):
            main()
        self.assertEqual(error.exception.code, 2)

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
