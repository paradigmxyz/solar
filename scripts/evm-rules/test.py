"""Regression tests for the Lean word model, ISLE reader and discovery gate."""

import functools
import io
import itertools
import json
import os
import random
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from concurrent.futures import ThreadPoolExecutor
from contextlib import ExitStack, redirect_stderr, redirect_stdout
from pathlib import Path
from queue import SimpleQueue
from types import SimpleNamespace
from unittest.mock import MagicMock, patch

from evm_rules.discovery import (
    Cost,
    Prices,
    discover_rules,
    emit_rule,
    enumerate_rules,
    read_seeds,
)
from evm_rules.expr import MASK, MODULUS, SIGN, Cond, Expr, Unsupported, concrete, holds
from evm_rules.isle import ISLE, Context, Rule, forms, rule_sources
from evm_rules.late import execute as execute_late
from evm_rules.lean import (
    COMMANDS,
    OPERATIONS,
    PRELUDE,
    canonical,
    lean_name,
    prop,
    simplify,
    term,
    theorem,
)
from evm_rules.memory import MemoryAddresses
from evm_rules.mining import abstract_patterns, mine
from evm_rules.prover import (
    LEAN_PROJECT,
    MANUAL_PROOFS,
    Checker,
    job,
    lean_environment,
    prove,
    simple_witness,
    stop,
)
from evm_rules.stack import requirements, stack_rules, stack_variants
from evm_rules.verification import (
    DEFAULT_FILES,
    obligations,
    proof_name,
    verify_files,
)
from verify import main

# These tests check proof results, not prover performance. Leave headroom for
# slower CI runners and for tests running in parallel: Lean limits the SAT
# solver by wall-clock time.
PROOF_TIMEOUT_MS = 120_000


def expression(op, *args):
    return Expr(op, tuple(Expr.const(a) if isinstance(a, int) else a for a in args))


def read_egraph_source():
    return "\n".join(text for _, text in rule_sources(ISLE / "mir/egraph"))


@functools.cache
def lean_path():
    """Build the Lean model once per run."""
    return lean_environment()


@functools.cache
def shared_checker():
    """One Lean checker for the whole run: it imports the model once."""
    return Checker(lean_path())


def tearDownModule():
    if shared_checker.cache_info().currsize:
        shared_checker().close()


def check(lhs, rhs, assumptions=(), timeout_ms=PROOF_TIMEOUT_MS):
    """Decide `assumptions → lhs = rhs` with the Lean model."""
    return shared_checker().check(lhs, rhs, assumptions, timeout_ms)


def check_all(queries, timeout_ms=PROOF_TIMEOUT_MS):
    """Decide each `(lhs, rhs, assumptions)` query, with one Lean checker per core."""
    idle = SimpleQueue()

    def run(query):
        worker = idle.get()
        try:
            return worker.check(*query, timeout_ms)
        finally:
            idle.put(worker)

    jobs = os.cpu_count() or 4
    with ExitStack() as stack, ThreadPoolExecutor(jobs) as pool:
        for _ in range(jobs):
            idle.put(stack.enter_context(Checker(lean_path())))
        return list(pool.map(run, queries))


def verify_rules(path, *, processes=False, timeout_s=PROOF_TIMEOUT_MS // 1000):
    """Verify one rule file; each rule's single proof result is merged into it.

    By default the shared checker answers every theorem; `processes` checks each in
    its own `lean` process, as `--isolated` does.
    """
    with tempfile.TemporaryDirectory() as directory:
        report = verify_files(
            [path],
            Path(directory),
            lean_path(),
            jobs=os.cpu_count() or 4,
            timeout_s=timeout_s,
            progress=None,
            checker=None if processes else shared_checker(),
            isolated=processes,
        )["files"][0]
    for rule in report["rules"]:
        proofs = rule.get("proofs", [])
        if len(proofs) == 1:
            rule.update({k: v for k, v in proofs[0].items() if k != "status"})
        if "error" in rule:
            rule["reason"] = rule["error"]
    return report


def repr_rule(form):
    """Print a read rule form back as ISLE source."""
    if isinstance(form, str):
        return form
    return "(" + " ".join(repr_rule(part) for part in form) + ")"


def bind(expr, values):
    """Replace variables by constant words."""
    if expr.op == "var":
        return Expr.const(values[expr.args[0]]) if expr.args[0] in values else expr
    if expr.op == "const":
        return expr
    return Expr(expr.op, tuple(bind(arg, values) for arg in expr.args))


def lean_evaluates(expr, values, expected):
    """Whether the Lean model evaluates `expr` at `values` to `expected`."""
    closed = term(bind(expr, values))
    reply = shared_checker().ask(
        f"{COMMANDS}\n#guard {closed} == {hex(expected)}\n", 300
    )
    return reply is not None and reply[0]


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
                prices,
                ["x"],
                ["not"],
                1,
                10,
                5000,
                seeds=seeds,
                checker=shared_checker(),
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
            return verify_rules(path)

    def test_actual_compiled_stack_rules(self):
        report = verify_rules(ISLE / "evm-ir/stack_peephole.isle")
        self.assertEqual(len(report["rules"]), 7)
        self.assertTrue(all(r["status"] == "proved" for r in report["rules"]))
        self.assertGreater(sum(r["variants"] for r in report["rules"]), 900)

    def test_every_legal_depth_is_checked(self):
        # DUP and SWAP encode depths 1 to 235; EXCHANGE n m needs n < m and n + m <= 30.
        exchanges = sum(
            1 <= n < m and n + m <= 30 for n in range(1, 236) for m in range(1, 236)
        )
        _, rules = stack_rules(ISLE / "evm-ir/stack_peephole.isle")
        for rule in rules:
            variants = stack_variants(rule)
            with self.subTest(line=rule.line):
                if len(variants[0]["bindings"]) == 1:
                    self.assertEqual(len(variants), 235)
                if len(variants[0]["bindings"]) == 2:
                    self.assertEqual(len(variants), exchanges)
        self.assertEqual(requirements([("dup", (235,)), ("pop", ())]), (235, 1, 0))

    def test_wrong_depth_and_opcode_have_replayed_counterexamples(self):
        for source in (
            "(rule (peep_swap (last2 (dup 2) (swap 1))) (rewrite 2 (Edit.Keep 1)))",
            "(rule (peep_op (last2 (opcode $NOT) (opcode $NOT))) (rewrite 2 (Edit.OverwriteOne $ISZERO)))",
        ):
            result = self.verify(source)["rules"][0]
            self.assertEqual(result["status"], "counterexample")
            self.assertTrue(result["proofs"][0]["replayed"])

    def test_equality_shuffle_requires_symmetric_operands(self):
        correct = "(rule (peep_pop (unprotected_last5 (opcode $DUP2) (opcode $EQ) (opcode $ISZERO) (opcode $SWAP1) (opcode $POP))) (rewrite 5 (Edit.RemoveFirstKeepTwo)))"
        self.assertEqual(self.verify(correct)["rules"][0]["status"], "proved")
        wrong = correct.replace("$DUP2", "$DUP1")
        result = self.verify(wrong)["rules"][0]
        self.assertEqual(result["status"], "counterexample")
        self.assertTrue(result["proofs"][0]["replayed"])

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
            return verify_rules(path, processes=True)

    def test_compiled_mask_window_and_boundaries(self):
        report = self.verify((ISLE / "evm-ir/late_word.isle").read_text())
        result = report["rules"][0]
        self.assertTrue(all(r["status"] == "proved" for r in report["rules"]))
        details = result["details"]
        self.assertEqual(
            (
                details["minimum_stack"],
                details["summary_before_peak"],
                details["summary_after_peak"],
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


class SemanticsTests(unittest.TestCase):
    def assert_evaluation(self, op, args, expected):
        expr = expression(op, *args)
        self.assertEqual(concrete(expr, {}), expected & MASK)
        self.assertTrue(lean_evaluates(expr, {}, expected & MASK))

    def test_evm_boundaries(self):
        cases = [
            ("add", (MASK, 1), 0),
            ("sub", (0, 1), MASK),
            ("mul", (SIGN, 2), 0),
            ("div", (MASK, 0), 0),
            ("mod", (MASK, 0), 0),
            ("sdiv", (MASK, 0), 0),
            ("smod", (MASK, 0), 0),
            ("sdiv", (SIGN, MASK), SIGN),
            ("sdiv", (MODULUS - 5, 3), MODULUS - 1),
            ("sdiv", (5, MODULUS - 3), MODULUS - 1),
            ("smod", (MODULUS - 5, 3), MODULUS - 2),
            ("smod", (5, MODULUS - 3), 2),
            ("addmod", (MASK, 1, 3), 1),
            ("mulmod", (SIGN, 2, 3), 1),
            ("addmod", (MASK, MASK, 0), 0),
            ("mulmod", (MASK, MASK, 0), 0),
            ("exp", (0, 0), 1),
            ("exp", (MASK, 2), 1),
            ("shl", (256, 1), 0),
            ("shr", (MASK, MASK), 0),
            ("sar", (256, SIGN), MASK),
            ("sar", (MASK, 1), 0),
            ("byte", (0, SIGN), 128),
            ("byte", (31, 255), 255),
            ("byte", (32, MASK), 0),
            ("byte", (MASK, MASK), 0),
            ("signextend", (0, 128), MODULUS - 128),
            ("signextend", (0, 127), 127),
            ("signextend", (31, SIGN), SIGN),
            ("signextend", (MASK, 128), 128),
            ("clz", (0,), 256),
            ("clz", (1,), 255),
            ("clz", (SIGN,), 0),
            ("lt", (MASK, 1), 0),
            ("slt", (MASK, 1), 1),
            ("select", (2, 7, 9), 7),
            ("iszero", (2,), 0),
        ]
        for op, args, expected in cases:
            with self.subTest(op=op, args=args):
                self.assert_evaluation(op, args, expected)

    def test_symbolic_zero_division(self):
        x = Expr.var("x")
        for op in ("div", "sdiv", "mod", "smod"):
            result = check(expression(op, x, 0), Expr.const(0))
            self.assertEqual(result["status"], "proved", op)

    def test_counterexample_replay(self):
        x = Expr.var("x")
        result = check(expression("shr", 1, expression("shl", 1, x)), x)
        self.assertEqual(result["status"], "counterexample")
        self.assertTrue(result["replayed"])
        self.assertNotEqual(result["lhs_value"], result["rhs_value"])

    def test_unsupported_operations_fail(self):
        for op in ("sload", "mload", "call", "keccak256", "mystery"):
            with self.assertRaises(Unsupported):
                term(expression(op, 0))
        with self.assertRaises(Unsupported):
            term(expression("add", 0))

    def test_signextend_matches_the_shift_definition_at_every_index(self):
        value = Expr.var("value")
        for index in range(32):
            # signextend(i, v) == sar(248 - 8i, shl(248 - 8i, v)) below 31
            shift = Expr.const(248 - 8 * index) if index < 31 else None
            expected = (
                expression("sar", shift, expression("shl", shift, value))
                if shift is not None
                else value
            )
            with self.subTest(index=index):
                result = check(expression("signextend", index, value), expected)
                self.assertEqual(result["status"], "proved", result)
        # Indices beyond the word, however large, are the identity.
        index = Expr.var("index")
        result = check(
            expression("signextend", index, value),
            value,
            [Cond("uge", (index, Expr.const(31)))],
        )
        self.assertEqual(result["status"], "proved", result)

    def test_signextend_wrong_zero_extension_replays(self):
        value = Expr.var("value")
        result = check(
            expression("signextend", 0, value), expression("and", value, 255)
        )
        self.assertEqual(result["status"], "counterexample")
        self.assertTrue(result["replayed"])


class EnvironmentTests(unittest.TestCase):
    def test_actual_balance_mask_rules_require_all_address_bits(self):
        path = ISLE / "mir/egraph"
        rules = [
            Rule(form, line, str(path))
            for form, line in forms(read_egraph_source())
            if form[0] == "rule" and "Op.Balance" in repr(form) and "band" in repr(form)
        ]
        self.assertEqual(len(rules), 1)
        for rule in rules:
            with self.subTest(line=rule.line):
                cx = Context()
                lhs, rhs = cx.obligation(rule)
                # Lean relates the two reads through the rule's hand-written proof.
                proof = (MANUAL_PROOFS / f"egraph_{rule.digest[:16]}.lean").read_text()
                result = shared_checker().check(lhs, rhs, cx.assumptions, 30_000, proof)
                self.assertEqual(result["status"], "proved", result)
                # Dropping the low-bit guard must expose a different account balance.
                broken = Rule(
                    tuple(
                        part for part in rule.form if "u256_is_zero" not in repr(part)
                    ),
                    rule.line,
                    rule.source,
                )
                cx = Context()
                lhs, rhs = cx.obligation(broken)
                result = check(lhs, rhs, cx.assumptions, 5000)
                self.assertEqual(result["status"], "counterexample")
                self.assertTrue(result["replayed"])

    def test_actual_self_balance_rule_uses_a_shared_state_array(self):
        path = ISLE / "mir/egraph"
        rules = [
            Rule(form, line, str(path))
            for form, line in forms(read_egraph_source())
            if form[0] == "rule" and "current_address" in repr(form)
        ]
        self.assertEqual(len(rules), 1)
        cx = Context()
        lhs, rhs = cx.obligation(rules[0])
        self.assertEqual(check(lhs, rhs, cx.assumptions)["status"], "proved")
        self.assertIn(
            "(«@environment:balances» : BitVec 160 → Word)",
            theorem("t", lhs, rhs, simplify(cx.assumptions), "rfl"),
        )

    def test_wrong_account_replays_the_snapshot(self):
        lhs = expression("balance", Expr.var("other"))
        rhs = expression("selfbalance")
        result = check(lhs, rhs)
        self.assertEqual(result["status"], "counterexample")
        self.assertTrue(result["replayed"])
        state = result["environment"]
        environment = {
            "address": int(state["address"], 16),
            "balances": {int(k, 16): int(v, 16) for k, v in state["balances"].items()},
        }
        values = {k: int(v, 16) for k, v in result["inputs"].items()}
        self.assertNotEqual(values["other"] & ((1 << 160) - 1), environment["address"])
        self.assertEqual(
            concrete(lhs, values, environment), int(result["lhs_value"], 16)
        )
        self.assertEqual(
            concrete(rhs, values, environment), int(result["rhs_value"], 16)
        )

    def test_balance_addresses_truncate_to_160_bits(self):
        address = Expr.var("account")
        lhs = expression("balance", address)
        rhs = expression("balance", expression("and", address, (1 << 160) - 1))
        self.assertEqual(check(lhs, rhs)["status"], "proved")
        environment = {"address": 7, "balances": {7: MASK, (1 << 160) - 1: 19}}
        self.assertEqual(
            concrete(expression("balance", (1 << 160) + 7), {}, environment), MASK
        )
        self.assertEqual(concrete(expression("balance", MASK), {}, environment), 19)
        self.assertEqual(concrete(expression("balance", 8), {}, environment), 0)
        self.assertEqual(concrete(expression("address"), {}, environment), 7)
        self.assertEqual(concrete(expression("selfbalance"), {}, environment), MASK)

    def test_nested_reads_collect_every_observed_account(self):
        lhs = expression("balance", expression("balance", Expr.var("account")))
        result = check(lhs, expression("not", lhs))
        self.assertEqual(result["status"], "counterexample")
        self.assertTrue(result["replayed"])
        self.assertIn("environment", result)

    def test_state_changes_and_unknown_environment_operations_stay_unsupported(self):
        for op in ("call", "sstore", "selfdestruct", "origin"):
            with self.assertRaises(Unsupported):
                term(expression(op))
        with self.assertRaises(Unsupported):
            concrete(expression("selfbalance"), {})
        with self.assertRaises(Unsupported):
            Context().pattern("@environment:address")
        source = (
            (ISLE / "mir-to-evm/select.isle")
            .read_text()
            .replace("$ADDRESS)", "$ORIGIN)")
        )
        with self.assertRaises(Unsupported):
            Context(selection_source=source).pattern(("current_address",))

    def test_reader_rejects_snapshot_assumptions_about_earlier_producers(self):
        # This equality holds in one snapshot, but the two producer instructions
        # could straddle a call. The rule has no state-clobber guard.
        source = "(rule (simplify (Op.Sub (balance (current_address)) (selfbalance))) (imm 0))"
        form, line = forms(source)[0]
        with self.assertRaisesRegex(Unsupported, "instruction roots"):
            Context().obligation(Rule(form, line, "nested.isle"))


class CallEffectTests(unittest.TestCase):
    def call_rules(self):
        return [
            Rule(form, line, "mir/egraph")
            for form, line in forms(read_egraph_source())
            if form[0] == "rule"
            and form[1][0] == "rewrite"
            and form[1][1][0]
            in ("Op.Call", "Op.CallCode", "Op.StaticCall", "Op.DelegateCall")
            and "trunc" in repr(form)
        ]

    def test_actual_rules_preserve_call_effects(self):
        rules = self.call_rules()
        self.assertEqual(len(rules), 4)
        for rule in rules:
            context = Context()
            lhs, rhs = context.obligation(rule)
            result = check(lhs, rhs, context.assumptions, 5000)
            self.assertEqual(result["status"], "proved")

    def test_changed_call_operands_are_counterexamples(self):
        for rule in self.call_rules():
            for index in range(1, len(rule.form[-1])):
                replacement = list(rule.form[-1])
                replacement[index] = ("imm", ("u256", "1"))
                changed = Rule(
                    (*rule.form[:-1], tuple(replacement)), rule.line, rule.source
                )
                context = Context()
                lhs, rhs = context.obligation(changed)
                result = check(lhs, rhs, context.assumptions, 5000)
                self.assertEqual(result["status"], "counterexample")

    def test_empty_memory_regions_preserve_effects(self):
        rules = [
            Rule(form, line, "mir/egraph")
            for form, line in forms(read_egraph_source())
            if form[0] == "rule"
            and "same_value" in repr(form)
            and any(
                name in repr(form)
                for name in ("Op.Call", "Op.StaticCall", "Op.DelegateCall", "Op.Log")
            )
        ]
        self.assertEqual(len(rules), 13)
        for rule in rules:
            cx = Context()
            lhs, rhs = cx.obligation(rule)
            result = check(lhs, rhs, cx.assumptions, 5000)
            self.assertEqual(result["status"], "proved", rule.line)
        for source in (
            "(rule (rewrite (Op.Log0 offset size)) (Op.Log0 (imm (u256 0)) size))",
            "(rule (rewrite (Op.StaticCall gas addr offset size out count)) (Op.StaticCall gas addr (imm (u256 0)) size out count))",
        ):
            form, line = forms(source)[0]
            cx = Context()
            lhs, rhs = cx.obligation(Rule(form, line, "missing-size-guard"))
            result = check(lhs, rhs, cx.assumptions, 5000)
            self.assertEqual(result["status"], "counterexample")

    def test_calls_cannot_be_removed_changed_or_nested(self):
        rule = self.call_rules()[0]
        for replacement in (
            ("imm", ("u256", "0")),
            ("Op.CallCode", *rule.form[-1][1:]),
        ):
            changed = Rule((*rule.form[:-1], replacement), rule.line, rule.source)
            with self.assertRaisesRegex(Unsupported, "preserve its opcode and effect"):
                Context().obligation(changed)
        nested = ("Op.Add", rule.form[-1], ("zero",))
        changed = Rule(
            ("rule", ("rewrite", nested), ("imm", ("u256", "0"))), 1, "nested"
        )
        with self.assertRaisesRegex(Unsupported, "instruction roots"):
            Context().obligation(changed)


class MemoryAddressTests(unittest.TestCase):
    def test_actual_projection_rules_and_missing_guards(self):
        path = ISLE / "mir/egraph"
        rules = [
            Rule(form, line, str(path))
            for form, line in forms(read_egraph_source())
            if form[0] == "rule"
            and any(name in repr(form) for name in MemoryAddresses.SHAPES)
        ]
        self.assertEqual(len(rules), 3)
        for rule in rules:
            cx = Context()
            lhs, rhs = cx.obligation(rule)
            result = check(lhs, rhs, cx.assumptions, 5000)
            self.assertEqual(result["status"], "proved", rule.line)
            # Removing the actual source guard must expose a nonzero header
            # or field offset, rather than implicitly assuming the rewrite.
            unguarded = Rule(
                (rule.form[0], rule.form[1], rule.form[-1]), rule.line, rule.source
            )
            cx = Context()
            lhs, rhs = cx.obligation(unguarded)
            result = check(lhs, rhs, cx.assumptions, 5000)
            self.assertEqual(result["status"], "counterexample", rule.line)
            self.assertTrue(result["replayed"])

    def assert_concrete_and_symbolic(self, value, values, expected):
        self.assertTrue(lean_evaluates(value, values, expected))
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
        result = check(value, wrong, cx.assumptions, 5000)
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
            [*cx.assumptions, Cond("eq", (field, shape.fields))],
        )
        self.assertEqual(result["status"], "inapplicable")

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
        result = check(
            value, value, [*cx.assumptions, Cond("eq", (shape.kind, Expr.const(3)))]
        )
        self.assertEqual(result["status"], "inapplicable")

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


class RuleTests(unittest.TestCase):
    def test_narrow_integer_rules(self):
        entries = list(obligations(ISLE / "mir/word"))
        native = [entry for entry in entries if "integer_bits" in entry]
        self.assertEqual(len(native), 93 * 33)
        for bits in (1, *range(8, 257, 8)):
            selected = [entry for entry in native if entry["integer_bits"] == bits]
            with self.subTest(bits=bits):
                self.assertEqual(len(selected), 93)
                self.assertTrue(all(entry.get("theorems") for entry in selected))
                self.assertEqual([e["error"] for e in selected if "error" in e], [])
        self.assertEqual(len({entry["name"] for entry in entries}), len(entries))

    def test_native_width_failure_reaches_the_report(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "rules.isle"
            path.write_text("(rule (simplify (Op.Add x (one))) x)\n;; end\n")
            with (
                patch("evm_rules.verification.NATIVE_PREFIXES", {path: ";; end"}),
                patch("evm_rules.verification.INTEGER_WIDTHS", (1, 8)),
            ):
                report = verify_rules(path, processes=True)
            self.assertEqual(len(report["rules"]), 3)
            self.assertEqual(
                [rule.get("integer_bits") for rule in report["rules"]], [None, 1, 8]
            )
            for rule in report["rules"]:
                self.assertEqual(rule["status"], "counterexample", rule)
                self.assertTrue(rule["replayed"], rule)

    def test_native_rule_boundary_must_exist(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "rules.isle"
            path.write_text("(rule (simplify (Op.Add x (zero))) x)\n")
            with (
                patch("evm_rules.verification.NATIVE_PREFIXES", {path: ";; end"}),
                self.assertRaisesRegex(ValueError, "missing native rule boundary"),
            ):
                list(obligations(path))

    def test_native_signed_limits(self):
        rules = [
            Rule(form, line, str(path))
            for directory in ("word", "word_sequence", "egraph")
            for path, source in rule_sources(ISLE / "mir" / directory)
            for form, line in forms(source)
            if form[0] == "rule"
            and "integer_sign_bit" in repr(form)
            and "power_of_two_shift" not in repr(form)
        ]
        self.assertGreater(len(rules), 10)
        cases = []
        for bits in (1, 8, 16, 160, 248, 256):
            for rule in rules:
                cx = Context(integer_bits=bits)
                lhs, rhs = cx.obligation(rule)
                cases.append((bits, rule, (lhs, rhs, cx.assumptions)))
        results = check_all([query for *_, query in cases])
        for (bits, rule, (_, _, assumptions)), result in zip(cases, results):
            with self.subTest(bits=bits, rule=rule.form):
                if result["status"] == "inapplicable":
                    self.assertEqual(
                        check(Expr.const(0), Expr.const(1), assumptions)["status"],
                        "inapplicable",
                    )
                else:
                    self.assertEqual(result["status"], "proved", result)

    def test_native_overflow_guards(self):
        for bits in (1, *range(8, 257, 8)):
            cx = Context(integer_bits=bits)
            mask = (1 << bits) - 1
            for a, b in itertools.product((0, 1, mask // 2, mask), repeat=2):
                with self.subTest(bits=bits, a=a, b=b):
                    self.assertEqual(
                        holds(cx.constructor(("u256_mul_fits", str(a), str(b))), {}),
                        a * b <= mask,
                    )
                    self.assertEqual(
                        holds(cx.constructor(("u256_add_fits", str(a), str(b))), {}),
                        a + b <= mask,
                    )

    def test_narrow_integer_wrap_is_not_word_wrap(self):
        form, line = forms("(rule (simplify (Op.Add x (one))) x)")[0]
        cx = Context(integer_bits=8)
        lhs, rhs = cx.obligation(Rule(form, line, "narrow.isle"))
        result = check(lhs, rhs, cx.assumptions)
        self.assertEqual(result["status"], "counterexample")
        self.assertTrue(result["replayed"])

    def verify(self, source):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "rules.isle"
            path.write_text(source)
            return verify_rules(path)

    def test_every_selected_rule_has_a_theorem(self):
        # No rule of a selected file is skipped: each becomes at least one theorem.
        for path in DEFAULT_FILES:
            with self.subTest(path=path.name):
                entries = list(obligations(path))
                rule_forms = [
                    form for _, text in rule_sources(path) for form, _ in forms(text)
                ]
                self.assertEqual(
                    sum("integer_bits" not in entry for entry in entries),
                    sum(form[0] == "rule" for form in rule_forms),
                )
                skipped = [entry["name"] for entry in entries if "error" in entry]
                self.assertEqual(skipped, [])
                self.assertTrue(all(entry.get("theorems") for entry in entries))

    def test_reserved_variable_names_fail_closed(self):
        # Names starting with `@` belong to the checker; a rule cannot bind one.
        for source in (
            "(rule (rewrite (Op.Add @fresh:1 x)) x)",
            "(rule (rewrite (Op.Add @flag:x x)) x)",
            "(rule (rewrite (Op.Add @balance:0 x)) x)",
            "(rule (rewrite (Op.Add x (zero))) (if-let @value (u256 1)) x)",
        ):
            with self.subTest(source=source):
                result = self.verify(source)["rules"][0]
                self.assertEqual(result["status"], "unsupported", result)

    def test_rule_directory_preserves_sources(self):
        with tempfile.TemporaryDirectory() as directory:
            rules = Path(directory) / "rules"
            rules.mkdir()
            (rules / "prelude.isle").write_text("(decl multi rewrite (Op) Op)\n")
            identity = "(rule (rewrite (Op.Add x (zero))) (Op.Add x (imm (u256 0))))\n"
            for name in ("a", "b"):
                (rules / f"{name}.isle").write_text(identity)
            whole = verify_rules(rules)
            self.assertEqual(
                [r["status"] for r in whole["rules"]], ["proved", "proved"]
            )
            self.assertEqual(
                [r["source"] for r in whole["rules"]],
                [str(rules / "a.isle"), str(rules / "b.isle")],
            )
            self.assertEqual([r["line"] for r in whole["rules"]], [1, 1])
            self.assertEqual(len({r["name"] for r in whole["rules"]}), 2)
            (rules / "b.isle").write_text(
                "(rule (rewrite (Op.Add x (zero))) (Op.Not x))\n"
            )
            edited = verify_rules(rules)
            self.assertEqual(edited["rules"][1]["status"], "counterexample")
            self.assertNotEqual(whole["source_sha256"], edited["source_sha256"])

    def test_actual_source_is_checked_after_edit(self):
        before = self.verify("(rule (rewrite (Op.Sub (bnot x) (bnot y))) (Op.Sub y x))")
        after = self.verify("(rule (rewrite (Op.Sub (bnot x) (bnot y))) (Op.Sub x y))")
        self.assertEqual(before["rules"][0]["status"], "proved")
        self.assertEqual(after["rules"][0]["status"], "counterexample")
        self.assertNotEqual(before["source_sha256"], after["source_sha256"])

    def test_instruction_selection_drift_fails_closed(self):
        selection = (ISLE / "mir-to-evm/select.isle").read_text()
        for changed in (
            selection.replace("$ADD)", "$SUB)"),
            selection.replace("Op.Add _ _", "Op.Add x x"),
            selection.replace(
                "OpcodeLowering.Binary $ADD", "OpcodeLowering.Store $ADD"
            ),
        ):
            context = Context(changed)
            with self.assertRaises(Unsupported):
                context.pattern(("Op.Add", "x", "y"))

    def test_overflow_blocks_integer_cancellation(self):
        report = self.verify("""(rule (rewrite (Op.Div (mul x (iconst c)) (iconst c)))
          (if-let true (u256_eq c 2)) (Op.Add x (imm (u256 0))))""")
        self.assertEqual(report["rules"][0]["status"], "counterexample")

    def test_exp_rules_state_any_exponent_and_wrong_ones_replay(self):
        source = """(rule (rewrite (Op.Exp a (iconst c)))
          (if-let true (u256_eq c 3)) (Op.Mul a a))"""
        rule = self.verify(source)["rules"][0]
        self.assertEqual(rule["status"], "counterexample")
        self.assertEqual(rule["inputs"]["c"], "0x3")
        self.assertTrue(rule["replayed"])
        # Symbolic exponents are stated exactly; only their proofs need lemmas.
        lhs, rhs, assumptions = lean_rule(
            "(rule (simplify (Op.Exp (and a (one)) _)) a)"
        )
        self.assertIn(
            "Evm.exp a «@fresh:1»", theorem("t", lhs, rhs, assumptions, "rfl")
        )

    def test_i1_sign_extension_lowering(self):
        for bits in (160, 256):
            cx = Context()
            value = Expr.var("value")
            expected = cx.operation("Op.Sext", [value, Expr.const(1), Expr.const(bits)])
            negate = Expr("sub", (Expr.const(0), value))
            shifted = Expr("shr", (Expr.const(256 - bits), negate))
            multiply = Expr("mul", (value, Expr.const((1 << bits) - 1)))
            # Split the complete i1 domain to avoid bit-blasting a 256-bit multiplication.
            for bit in (0, 1):
                assumptions = [Cond("eq", (value, Expr.const(bit)))]
                for lowered in (shifted, multiply):
                    result = check(expected, lowered, assumptions, 5000)
                    self.assertEqual(
                        result["status"], "proved", (bits, bit, lowered, result)
                    )

    def test_actual_typed_extension_zero_rules(self):
        path = ISLE / "mir/egraph"
        rules = [
            Rule(form, line, str(path))
            for form, line in forms(read_egraph_source())
            if form[0] == "rule" and "imm_zero_like" in repr(form)
        ]
        self.assertEqual(len(rules), 4)
        for rule in rules:
            with self.subTest(rule=rule.form):
                cx = Context()
                lhs, rhs = cx.obligation(rule)
                result = check(lhs, rhs, cx.assumptions, 120_000)
                self.assertEqual(result["status"], "proved", result)

    def test_actual_integer_and_pointer_cast_rules(self):
        path = ISLE / "mir/egraph"
        source = read_egraph_source()
        start = source.index(";; Identity casts")
        rules = [
            Rule(form, line, str(path))
            for form, line in forms(source[start:])
            if form[0] == "rule"
        ]
        self.assertEqual(len(rules), 21)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "casts.isle"
            path.write_text("\n".join(repr_rule(rule.form) for rule in rules))
            # One process per rule proves the slowest casts in parallel, with the
            # SAT limit the command line uses.
            report = verify_rules(path, processes=True, timeout_s=120)
        for rule in report["rules"]:
            self.assertEqual(rule["status"], "proved", rule)

    def test_actual_power_of_two_remainder_rule(self):
        def contains(node, atom):
            return (
                node == atom
                or isinstance(node, tuple)
                and any(contains(child, atom) for child in node)
            )

        path = ISLE / "mir/egraph"
        rules = [
            Rule(form, line, str(path))
            for form, line in forms(read_egraph_source())
            if form[0] == "rule"
            and contains(form, "Op.Mod")
            and contains(form, "power_of_two_shift")
            and not contains(form, "integer_rewrite")
        ]
        self.assertEqual(len(rules), 1)
        context = Context()
        lhs, rhs = context.obligation(rules[0])
        result = check(lhs, rhs, context.assumptions)
        self.assertEqual(result["status"], "proved", (rules[0].line, result))

    def test_actual_nested_signextend_rule(self):
        path = ISLE / "mir/egraph"

        def has_nested_signextend(node):
            return isinstance(node, tuple) and (
                len(node) == 3
                and node[0] == "Op.SignExtend"
                and isinstance(node[2], tuple)
                and node[2][0] == "signextend"
                or any(has_nested_signextend(child) for child in node)
            )

        rules = [
            Rule(form, line, str(path))
            for form, line in forms(read_egraph_source())
            if form[0] == "rule" and has_nested_signextend(form)
        ]
        self.assertEqual(len(rules), 1)
        cx = Context()
        lhs, rhs = cx.obligation(rules[0])
        result = check(lhs, rhs, cx.assumptions, PROOF_TIMEOUT_MS)
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
        # One nonzero shift isolates guard necessity; CI proves every shift.
        for op in ("Eq", "Lt", "Gt"):
            source = f"""(rule (rewrite (Op.{op} (shl (iconst shift) x) (iconst c)))
              (if-let true (u256_eq shift 1)) {guard}
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
        report = self.verify("""(rule (simplify (Op.Eq (eq (and x (bool_value)) (zero)) (zero))) x)
          (rule (rewrite (Op.Sub x x)) (if-let false (u256_eq (u256 1) 1))
             (Op.Add x (imm (u256 0))))""")
        self.assertEqual(
            [r["status"] for r in report["rules"]], ["proved", "inapplicable"]
        )
        self.assertTrue(report["rules"][0]["contracts"])

    def test_clz_requires_the_known_sign_bit_contract(self):
        guard = "(if-let true (has_known_sign_bit a))"
        source = f"(rule (simplify (Op.Clz a)) {guard} (imm (u256 0)))"
        rule = self.verify(source)["rules"][0]
        self.assertEqual(rule["status"], "proved")
        self.assertTrue(
            any("has_known_sign_bit" in contract for contract in rule["contracts"])
        )
        for changed in (
            source.replace(guard, ""),
            source.replace(guard, "(if-let false (has_known_sign_bit a))"),
            source.replace("(u256 0)", "(u256 1)"),
        ):
            rule = self.verify(changed)["rules"][0]
            self.assertEqual(rule["status"], "counterexample")
            self.assertTrue(rule["replayed"])
        for constructor in (
            "has_known_sign_bit",
            "has_known_sign_bit a a",
            "is_zero_or_one a a",
            "below_const a",
            "below_const a a a",
        ):
            changed = source.replace("has_known_sign_bit a", constructor)
            rule = self.verify(changed)["rules"][0]
            self.assertEqual(rule["status"], "unsupported")
            self.assertIn("arity", rule["reason"])

    def test_unknown_terms_and_bare_if_are_not_skipped(self):
        report = self.verify("""(rule (rewrite (Op.Unknown x)) (Op.Add x x))
          (rule (rewrite (Op.Sub x x)) (if (u256_is_zero x)) (Op.Add x x))""")
        self.assertEqual(
            [r["status"] for r in report["rules"]], ["unsupported", "unsupported"]
        )

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
            reversed_source = source.replace(
                f"u256_{op.lower()} index value", f"u256_{op.lower()} value index"
            )
            rule = self.verify(reversed_source)["rules"][0]
            self.assertEqual(rule["status"], "counterexample")
            self.assertTrue(rule["replayed"])

    def test_constant_folds_keep_evm_operand_order(self):
        # Shifts and BYTE take the count or index before the value.
        for op in ("Shl", "Shr", "Byte"):
            source = f"""(rule (rewrite (Op.{op} (iconst index) (iconst value)))
              (imm (u256_{op.lower()} index value)))"""
            with self.subTest(op=op):
                self.assertEqual(self.verify(source)["rules"][0]["status"], "proved")
                swapped = source.replace("index value)", "value index)")
                result = self.verify(swapped)["rules"][0]
                self.assertEqual(result["status"], "counterexample", result)
                self.assertTrue(result["replayed"])

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
    def test_nonpositive_limits_are_rejected(self):
        for option in ("--timeout-s", "--jobs"):
            with (
                self.subTest(option=option),
                patch(
                    "sys.argv",
                    ["verify.py", "verify", "--output", "unused.json", option, "0"],
                ),
                redirect_stderr(io.StringIO()),
                self.assertRaises(SystemExit) as error,
            ):
                main()
            self.assertEqual(error.exception.code, 2)

    def test_verification_status_and_failure_diagnostics(self):
        sources = {
            "proved": "(rule (rewrite (Op.Sub (bnot x) (bnot y))) (Op.Sub y x))",
            "counterexample": "(rule (rewrite (Op.Sub (bnot x) (bnot y))) (Op.Sub x y))",
            "unsupported": "(rule (rewrite (Op.Unknown x)) (Op.Add x x))",
            "inapplicable": """(rule (rewrite (Op.Sub x x)) (if-let false (u256_eq (u256 1) 1))
             (Op.Add x (imm (u256 0))))""",
        }
        for status, source in sources.items():
            with (
                self.subTest(status=status),
                tempfile.TemporaryDirectory() as directory,
            ):
                path = Path(directory) / "rules.isle"
                path.write_text(source)
                output = Path(directory) / "proofs.json"
                stdout, stderr = io.StringIO(), io.StringIO()
                argv = [
                    "verify.py",
                    "verify",
                    str(path),
                    "--output",
                    str(output),
                    "--work-dir",
                    str(Path(directory) / "work"),
                    "--timeout-s",
                    "30",
                ]
                with (
                    patch("sys.argv", argv),
                    redirect_stdout(stdout),
                    redirect_stderr(stderr),
                ):
                    code = main()
                self.assertEqual(code, 0 if status == "proved" else 1)
                if status == "proved":
                    self.assertEqual(stderr.getvalue(), "")
                else:
                    self.assertTrue(
                        stderr.getvalue().startswith(f"{path}:1: {status}"),
                        stderr.getvalue(),
                    )
                self.assertEqual(
                    json.loads(stdout.getvalue().splitlines()[-1]), {status: 1}
                )
                self.assertEqual(json.loads(output.read_text())["counts"], {status: 1})

    @patch("evm_rules.prover.simple_witness", return_value=None)
    def test_changed_checker_rechecks_an_unchanged_theorem(self, _witness):
        # The theorem text stays the same while the result validation or the
        # applicability check changes: every run must apply the current checker, so
        # an earlier success cannot bypass it.
        source = """(rule (rewrite (Op.Div x (iconst c))) (if-let true (u256_eq c 1))
          (Op.Add x (imm (u256 0))))"""
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "rules.isle"
            path.write_text(source)
            work = Path(directory) / "work"

            def run():
                report = verify_files(
                    [path], work, lean_path(), jobs=1, timeout_s=30, progress=None
                )
                rule = report["files"][0]["rules"][0]
                return rule["status"], (work / rule["name"] / "Proof.lean").read_text()

            status, text = run()
            self.assertEqual(status, "proved")
            with patch("evm_rules.prover.applicable", return_value=None):
                self.assertEqual(run(), ("unknown", text))
            with patch(
                "evm_rules.prover.errors", return_value=[(1, "error", "rejected")]
            ):
                self.assertEqual(run(), ("failed", text))
            self.assertEqual(run(), ("proved", text))

    def test_failed_or_timed_out_lean_never_proves(self):
        x = Expr.var("x")
        with tempfile.TemporaryDirectory() as directory:
            task = job(Path(directory), "t", x, x, [], "rfl", 30, lean_path())
            # A Lean process that fails without a diagnostic is not a proof.
            crashed = MagicMock(returncode=1)
            crashed.communicate.return_value = ("", "crashed")
            with patch("evm_rules.prover.subprocess.Popen", return_value=crashed):
                self.assertEqual(prove(task)[1]["status"], "failed")
            # A process that outlives its limit is stopped and reported as a timeout.
            hung = MagicMock(returncode=None)
            hung.communicate.side_effect = subprocess.TimeoutExpired("lean", 30)
            with (
                patch("evm_rules.prover.subprocess.Popen", return_value=hung),
                patch("evm_rules.prover.stop") as stopped,
            ):
                self.assertEqual(prove(task)[1]["status"], "timeout")
            stopped.assert_called_once_with(hung)

    def test_simple_witness_checks_every_guard(self):
        x = Expr.var("x")
        zero, one = Expr.const(0), Expr.const(1)
        self.assertEqual(simple_witness([Cond("eq", (x, zero))]), {"x": "0x0"})
        self.assertEqual(simple_witness([Cond("eq", (zero, zero))]), {})
        self.assertEqual(simple_witness([Cond("eq", (x, one))]), {"x": "0x1"})
        self.assertIsNone(simple_witness([Cond("eq", (x, zero)), Cond("eq", (x, one))]))
        self.assertIsNone(simple_witness([Cond("unsupported", (x,))]))
        self.assertIsNone(simple_witness([Cond("eq", (Expr("address", ()), zero))]))

    def test_simple_witness_still_requires_a_lean_proof(self):
        x = Expr.var("x")
        guards = [Cond("eq", (x, Expr.const(0)))]
        with Checker(lean_path()) as checker:
            result = checker.check(x, x, guards, PROOF_TIMEOUT_MS)
            self.assertEqual(result["status"], "proved", result)
            self.assertEqual(result["witness"], {"x": "0x0"})
            self.assertEqual(checker.queries, 1)
            result = checker.check(x, Expr.const(1), guards, PROOF_TIMEOUT_MS)
            self.assertEqual(result["status"], "counterexample", result)
            self.assertTrue(result["replayed"])

    def test_worker_reuses_process_without_retaining_declarations(self):
        with Checker(lean_path()) as checker:
            source = COMMANDS + "\ntheorem previous (x : Word) : x = x := by rfl"
            self.assertTrue(checker.ask(source, 30)[0])
            process = checker.process
            self.assertTrue(checker.ask(source, 30)[0])
            self.assertIs(checker.process, process)
            reply = checker.ask(
                COMMANDS + "\ntheorem next (x : Word) : x = x := by exact previous x",
                30,
            )
            self.assertFalse(reply[0])
            self.assertIn("Unknown identifier", reply[1])

    def test_worker_preserves_proof_and_applicability_results(self):
        operand = Expr.var("x")
        cases = [
            ([], "rfl", "proved"),
            ([Cond("eq", (operand, Expr.const(0)))], "rfl", "proved"),
            (
                [
                    Cond("eq", (operand, Expr.const(0))),
                    Cond("eq", (operand, Expr.const(1))),
                ],
                "rfl",
                "inapplicable",
            ),
            ([], "sorry", "failed"),
            ([Cond("eq", (operand, Expr.const(0)))], "sorry", "failed"),
            ([], "simp only [Nat.add_comm]; rfl", "failed"),
        ]
        with (
            tempfile.TemporaryDirectory() as directory,
            Checker(lean_path()) as checker,
        ):
            for assumptions, tactic, expected in cases:
                task = job(
                    Path(directory),
                    "t",
                    operand,
                    operand,
                    assumptions,
                    tactic,
                    30,
                    lean_path(),
                )
                with self.subTest(tactic=tactic, assumptions=assumptions):
                    isolated = prove(task)[1]
                    reused = prove(task, checker)[1]
                    self.assertEqual(isolated["status"], expected, isolated)
                    self.assertEqual(reused["status"], expected, reused)
                    self.assertEqual(isolated.get("witness"), reused.get("witness"))
            wrong = job(
                Path(directory),
                "t",
                operand,
                Expr.const(0),
                [],
                "evm_decide 30",
                30,
                lean_path(),
            )
            result = prove(wrong, checker)[1]
            self.assertEqual(result["status"], "counterexample", result)
            self.assertTrue(result["replayed"])

    def test_worker_timeout_restarts_before_the_next_proof(self):
        operand = Expr.var("x")
        with (
            tempfile.TemporaryDirectory() as directory,
            Checker(lean_path()) as checker,
        ):
            task = job(
                Path(directory), "t", operand, operand, [], "rfl", 30, lean_path()
            )
            self.assertEqual(prove(task, checker)[1]["status"], "proved")
            process = checker.process
            with patch("evm_rules.prover.select.select", return_value=([], [], [])):
                self.assertEqual(prove(task, checker)[1]["status"], "timeout")
            self.assertIsNone(checker.process)
            self.assertIsNotNone(process.poll())
            self.assertEqual(prove(task, checker)[1]["status"], "proved")
            self.assertIsNot(checker.process, process)

    def test_parallel_verification_bounds_and_closes_workers(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "rules.isle"
            path.write_text(
                "\n".join("(rule (simplify (Op.Add x (zero))) x)" for _ in range(6))
            )
            workers = []

            def create_worker(environment):
                worker = Checker(environment)
                workers.append(worker)
                return worker

            with patch("evm_rules.verification.Checker", side_effect=create_worker):
                report = verify_files(
                    [path],
                    Path(directory) / "work",
                    lean_path(),
                    jobs=2,
                    timeout_s=30,
                    progress=None,
                )
            self.assertEqual(report["counts"], {"proved": 6})
            self.assertEqual(len(workers), 2)
            self.assertEqual(sum(worker.queries for worker in workers), 6)
            self.assertTrue(all(worker.process is None for worker in workers))

    def test_stopping_a_proof_stops_its_solver(self):
        # `bv_decide` runs the SAT solver as a child of `lean`; a timeout must stop it
        # too, or it keeps writing its proof file.
        process = subprocess.Popen(
            ["sh", "-c", "sleep 60 & echo $!; wait"],
            stdout=subprocess.PIPE,
            text=True,
            start_new_session=True,
        )
        assert process.stdout is not None
        child = int(process.stdout.readline())
        stop(process)
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            try:
                os.kill(child, 0)
            except ProcessLookupError:
                break
            time.sleep(0.05)
        else:
            self.fail("the child process outlived its stopped parent")


class DiscoveryTests(unittest.TestCase):
    def test_deep_seeds_search_a_small_replacement_frontier(self):
        x, y = Expr.var("x"), Expr.var("y")
        seed = expression("sub", expression("or", x, y), expression("and", x, y))
        rules, summary = enumerate_rules(
            Prices("osaka"),
            ["x", "y"],
            ["xor"],
            1,
            20,
            5000,
            seeds=[seed],
            checker=shared_checker(),
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
            checker=shared_checker(),
        )
        self.assertFalse(any(lhs == seed for lhs, _, _, _ in rules))
        self.assertGreater(summary["counterexamples"], 0)
        self.assertGreater(summary["samples"], 1)
        self.assertEqual(initial, [{"x": 0}])
        # A checker that never decides leaves the seed unresolved.
        unknown = SimpleNamespace(check=lambda *_, **__: {"status": "unknown"})
        rules, summary = enumerate_rules(
            Prices("osaka"),
            ["x"],
            ["not"],
            1,
            20,
            5000,
            seeds=[expression("not", expression("not", x))],
            checker=unknown,
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
                enumerate_rules(
                    Prices("osaka"),
                    variables,
                    ["not"],
                    1,
                    20,
                    5000,
                    checker=shared_checker(),
                )

    def test_cheaper_proved_representative_survives(self):
        rules, _ = enumerate_rules(
            Prices("osaka"),
            ["x", "y"],
            ["xor", "not"],
            3,
            1000,
            5000,
            include_constants=True,
            checker=shared_checker(),
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

    def test_sampling_collision_requires_smt_and_refines(self):
        _, summary = enumerate_rules(
            Prices("osaka"),
            ["x"],
            ["not"],
            1,
            20,
            5000,
            initial_samples=[{"x": 0}],
            checker=shared_checker(),
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
            checker=shared_checker(),
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
                Prices("osaka"),
                ["x"],
                ["and"],
                2,
                20,
                5000,
                constants=[123456789],
                checker=shared_checker(),
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
            checker=shared_checker(),
        )
        self.assertTrue(
            any(lhs.variables() and rhs == Expr.const(0) for lhs, rhs, _, _ in rules)
        )


def lean_rule(source):
    """The theorem parts of the first rule in an ISLE snippet."""
    form, line = forms(source)[0]
    context = Context()
    lhs, rhs = context.obligation(Rule(form, line, "snippet.isle"))
    return lhs, rhs, simplify(context.assumptions)


BELOW_CONST = (
    "(rule (simplify (Op.Div a (iconst d)))"
    " (if-let true (below_const a d)) (imm (u256 0)))"
)


class LeanStatementTests(unittest.TestCase):
    def test_rules_become_theorems_over_the_evm_definitions(self):
        lhs, rhs, assumptions = lean_rule(BELOW_CONST)
        self.assertEqual(
            theorem("t", lhs, rhs, assumptions, "evm_decide"),
            "theorem t (a d : Word) (h₁ : (a < d)) :\n"
            "    (Evm.div a d) = 0 := by\n"
            "  evm_decide\n",
        )

    def test_environment_reads_quantify_one_snapshot(self):
        lhs, rhs, assumptions = lean_rule(
            "(rule (rewrite (Op.Balance (current_address)))"
            " (if-let true (has_self_balance)) (Op.SelfBalance))"
        )
        self.assertEqual(
            theorem("t", lhs, rhs, assumptions, "evm_decide"),
            "theorem t («@environment:balances» : BitVec 160 → Word)"
            " («@environment:address» : BitVec 160) :\n"
            "    (Evm.balance «@environment:balances» (Evm.address «@environment:address»))"
            " = (Evm.selfbalance «@environment:balances» «@environment:address») := by\n"
            "  evm_decide\n",
        )

    def test_forced_flags_leave_word_preconditions(self):
        x = Expr.var("x")
        flag, free = Cond.flag("f"), Cond.flag("g")
        bound = Cond("ult", (x, Expr.const(3)))
        one = Cond("eq", (x, Expr.const(1)))
        self.assertEqual(
            simplify(
                [
                    Cond("implies", (flag, bound)),
                    Cond("iff", (Cond.const(True), flag)),
                    Cond("iff", (Cond.const(False), one)),
                    Cond("implies", (free, bound)),
                    bound,
                ]
            ),
            [bound, Cond("not", (one,)), Cond("implies", (free, bound))],
        )

    def test_unmodeled_terms_fail_closed(self):
        x = Expr.var("x")
        for build in (
            lambda: term(Expr("keccak256", (x,))),
            lambda: term(Expr("add", (x,))),
            lambda: prop(Cond("weird", ())),
            lambda: lean_name("a«b"),
        ):
            with self.subTest(build=build), self.assertRaises(Unsupported):
                build()

    def test_stack_variants_share_theorems_up_to_renaming(self):
        def double_not(name):
            value = Expr.var(name)
            return Expr("xor", (Expr("not", (Expr("not", (value,)),)), value))

        self.assertEqual(canonical(double_not("s3")), canonical(double_not("s7")))
        self.assertEqual(canonical(double_not("s7")).variables(), {"s0"})

    def test_the_tools_import_no_smt_solver(self):
        script = (
            "import sys; sys.modules['z3'] = None; import verify; "
            "from evm_rules.verification import DEFAULT_FILES, obligations; "
            "print(sum(len(e.get('theorems', [])) for path in DEFAULT_FILES "
            "for e in obligations(path)))"
        )
        process = subprocess.run(
            [sys.executable, "-c", script],
            cwd=Path(__file__).parent,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(process.returncode, 0, process.stderr)
        self.assertGreater(int(process.stdout), 440)


@unittest.skipUnless(
    shutil.which("lean") and shutil.which("lake"),
    "Lean is optional for local verifier tests",
)
class LeanProofTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.lean_path = lean_environment()

    def check(self, text):
        """Check Lean source with the semantics library; return success and output."""
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "Check.lean"
            path.write_text(PRELUDE + "\n" + text)
            process = subprocess.run(
                ["lean", "-DwarningAsError=true", str(path)],
                cwd=LEAN_PROJECT,
                capture_output=True,
                text=True,
                timeout=600,
                check=False,
                env={**os.environ, "LEAN_PATH": self.lean_path},
            )
        output = process.stdout + process.stderr
        return process.returncode == 0 and "error" not in output, output

    def test_model_agrees_with_an_independent_transcription(self):
        # The library proves every operation equal to the definitions of #1648; without the
        # import, those proofs would silently stop being checked.
        names = [
            "shl",
            "shr",
            "sar",
            "div",
            "mod",
            "sdiv",
            "smod",
            "addmod",
            "mulmod",
            "exp",
            "byte",
            "signextend",
            "clz",
            "lt",
            "gt",
            "slt",
            "sgt",
            "eq",
            "ne",
            "iszero",
            "select",
        ]
        ok, output = self.check(
            "\n".join(f"#check @EvmRules.{name}_agrees" for name in names)
        )
        self.assertTrue(ok, output[-2000:])

    def test_semantics_match_the_integer_evaluator(self):
        rng = random.Random(7)
        words = [0, 1, 2, 7, 8, 30, 31, 32, 33, 255, 256, 257, SIGN - 1, SIGN]
        words += [SIGN + 1, MASK - 1, MASK, *(rng.getrandbits(256) for _ in range(3))]
        lines = []
        for op, arity in sorted(OPERATIONS.items()):
            if arity == 3:
                inputs = [tuple(rng.choices(words, k=3)) for _ in range(150)]
            else:
                inputs = list(itertools.product(words, repeat=arity))
            for args in inputs:
                if op == "exp":
                    # `BitVec.pow` is linear in the exponent.
                    args = (args[0], args[1] % 300)
                expr = Expr(op, tuple(Expr.const(arg) for arg in args))
                lines.append(f"#guard {term(expr)} == {hex(concrete(expr, {}))}")
        ok, output = self.check("\n".join(lines))
        self.assertTrue(ok, output[-2000:])

    def test_preconditions_match_the_integer_evaluator(self):
        rng = random.Random(11)
        words = [
            0,
            1,
            2,
            255,
            SIGN - 1,
            SIGN,
            MASK,
            *(rng.getrandbits(256) for _ in range(3)),
        ]
        x, y = Expr.var("x"), Expr.var("y")
        conditions = [
            Cond(op, (x, y)) for op in ("eq", "ne", "ult", "ule", "ugt", "uge")
        ]
        conditions += [
            Cond("msb", (x,)),
            Cond("not", (Cond("ult", (x, y)),)),
            Cond("implies", (Cond("ult", (x, y)), Cond("ne", (x, y)))),
            Cond("iff", (Cond("eq", (x, y)), Cond("ule", (y, x)))),
            Cond("bool_word", (x, Cond("ult", (y, x)))),
        ]
        lines = [
            f"#guard (fun (x y : Word) => decide {prop(cond)}) {a} {b} == "
            + str(holds(cond, {"x": a, "y": b})).lower()
            for cond in conditions
            for a in words
            for b in words
        ]
        ok, output = self.check("\n".join(lines))
        self.assertTrue(ok, output[-2000:])

    def test_false_rules_fail_with_counterexamples(self):
        a, b, x = Expr.var("a"), Expr.var("b"), Expr.var("x")
        difference = Expr("sub", (a, b))
        equal = Cond("eq", (a, b))
        cases = {
            "wrong_constant": (
                Expr("add", (x, Expr.const(0))),
                Expr("add", (x, Expr.const(1))),
                [],
            ),
            "missing_guard": (difference, Expr.const(0), []),
            "logical_for_arithmetic": (
                Expr("shr", (Expr.const(1), x)),
                Expr("sar", (Expr.const(1), x)),
                [],
            ),
            "smt_zero_divisor": (Expr("mod", (x, Expr.const(0))), x, []),
        }
        for name, (lhs, rhs, assumptions) in cases.items():
            with self.subTest(name=name):
                ok, output = self.check(
                    theorem(name, lhs, rhs, assumptions, "evm_decide 60")
                )
                self.assertFalse(ok)
                # Preprocessing may refute a goal before it reaches the SAT solver.
                self.assertRegex(output, "counterexample|reduced to False")
        ok, output = self.check(
            theorem("guarded", difference, Expr.const(0), [equal], "evm_decide 60")
        )
        self.assertTrue(ok, output)

    def test_hand_proofs_check_the_current_rules(self):
        proofs = {path.stem: path.read_text() for path in MANUAL_PROOFS.glob("*.lean")}
        self.assertTrue(proofs)
        theorems = {}
        for path in DEFAULT_FILES:
            for entry in obligations(path):
                for index, (name, lhs, rhs, assumptions) in enumerate(
                    entry.get("theorems", [])
                ):
                    key = proof_name(path, entry, index)
                    if key in proofs:
                        theorems[key] = theorem(
                            name, lhs, rhs, assumptions, proofs[key]
                        )
        # Every proof names a current rule; an edited or deleted rule fails here.
        self.assertEqual(sorted(theorems), sorted(proofs))
        ok, output = self.check("\n".join(theorems.values()))
        self.assertTrue(ok, output[-3000:])

    def test_applicability_needs_a_witness_of_the_preconditions(self):
        lhs, rhs, guards = lean_rule(
            "(rule (simplify (Op.Sub a (iconst c))) (if-let true (u256_is_zero c)) a)"
        )
        vacuous = lean_rule(
            "(rule (simplify (Op.Add a (iconst c)))"
            " (if-let true (u256_lt c 1)) (if-let true (u256_gt c 1)) a)"
        )
        constant = lean_rule(
            "(rule (simplify (Op.Add a (zero)))"
            " (if-let false (u256_eq (u256 (integer_width)) 1)) a)"
        )
        cases = (
            ("found", (lhs, rhs, guards), "evm_decide 60"),
            ("constant", constant, "evm_decide 60"),
            ("vacuous", vacuous, "evm_decide 60"),
            # `sorry` is never accepted as a proof.
            ("unproved", (lhs, rhs, guards), "sorry"),
        )
        results = {}
        with tempfile.TemporaryDirectory() as directory:
            for name, (left, right, assumptions), tactic in cases:
                task = job(
                    Path(directory),
                    name,
                    left,
                    right,
                    assumptions,
                    tactic,
                    60,
                    self.lean_path,
                )
                results[name] = prove(task)[1]
        found = results["found"]
        self.assertEqual(found["status"], "proved", found)
        self.assertEqual(found["witness"]["c"], "0x0")
        self.assertEqual(results["constant"]["status"], "proved", results)
        self.assertEqual(results["constant"]["witness"], {})
        self.assertEqual(check(*constant)["status"], "proved")
        self.assertEqual(results["vacuous"]["status"], "inapplicable", results)
        self.assertEqual(results["unproved"]["status"], "failed", results)


class ArithTests(unittest.TestCase):
    def verify(self, source):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "rules.isle"
            path.write_text(source)
            return verify_rules(path)["rules"]

    def test_division_rules_prove_over_natural_numbers(self):
        rules = self.verify("""
            (rule (rewrite (Op.Eq (div (mul x (iconst c)) (iconst d)) x))
              (if-let true (u256_same c d)) (if-let true (u256_ge c 2))
              (Op.Lt x (imm (u256_add (u256_div (u256_max) c) (u256 1)))))
            (rule (simplify (Op.Lt x (div (mul x (iconst c)) (iconst d))))
              (if-let true (u256_le c d)) (imm_bool false))
            (rule (rewrite (Op.Sub (bnot x) (bnot y))) (Op.Sub y x))""")
        self.assertEqual(
            [(rule["status"], rule["method"]) for rule in rules],
            [
                ("proved", "evm_arith"),
                ("proved", "evm_arith"),
                ("proved", "evm_decide"),
            ],
        )

    def test_wrong_division_rules_replay_counterexamples(self):
        cases = {
            # A share above one exceeds x: x = 1, c = 2, d = 1.
            "share": "(rule (simplify (Op.Lt x (div (mul x (iconst c)) (iconst d))))"
            " (if-let true (u256_le d c)) (imm_bool false))",
            # Without the guard, a zero divisor keeps x instead of the zero remainder.
            "remainder": "(rule (rewrite (Op.Sub x (mul (div x (iconst c)) (iconst d))))"
            " (if-let true (u256_same c d)) (Op.Mod x (imm c)))",
        }
        for name, source in cases.items():
            with self.subTest(name):
                lhs, rhs, assumptions = lean_rule(source)
                # `evm_arith` fails, and bit-blasting still finds a counterexample.
                result = shared_checker().check(lhs, rhs, assumptions, 5_000)
                self.assertEqual(result["status"], "counterexample", result)
                self.assertTrue(result["replayed"])


class TacticTests(unittest.TestCase):
    """Each tactic of `evm_auto` proves the rule shapes it exists for on its own."""

    def prove(self, source, tactic):
        lhs, rhs, assumptions = lean_rule(source)
        return shared_checker().check(
            lhs, rhs, assumptions, PROOF_TIMEOUT_MS, tactic=tactic
        )

    def test_structural_rewrites_prove_nested_extensions(self):
        # Before bit-blasting, the identities of `Casts.lean` close these goals.
        for source in (
            """(rule (rewrite (Op.Sext (sext value from middle) middle to))
              (if-let true (u32_lt from middle)) (if-let true (u32_lt middle to))
              (Op.Sext value from to))""",
            """(rule (rewrite (Op.Eq (sext (and a (integer_bits from)) from to)
                                   (sext (and b (integer_bits from)) from to)))
              (if-let true (u32_lt from to)) (Op.Eq a b))""",
            """(rule (rewrite
                (Op.SignExtend (iconst outer) (signextend (iconst inner) value)))
              (Op.SignExtend (imm (u256_min outer inner)) value))""",
        ):
            with self.subTest(source):
                result = self.prove(source, "evm_struct")
                self.assertEqual(result["status"], "proved", result)

    def test_bits_prove_variable_shift_counts(self):
        source = """(rule (sequence_rewrite (Op.Shr s (band (shl s x) m)))
          (if-let shifted (make (Op.Shr s m)))
          (sequence (Op.And x shifted)))"""
        result = self.prove(source, "evm_bits")
        self.assertEqual(result["status"], "proved", result)

    def test_simp_lemmas_prove_shift_compositions_and_folds(self):
        # The lemmas of #1648 prove these without a hand-written script.
        for source in (
            """(rule (rewrite (Op.Shl (iconst outer) (shl (iconst inner) value)))
              (Op.Shl (imm (shift_sum outer inner)) value))""",
            "(rule 1 (simplify (Op.Exp a (one))) a)",
            "(rule 2 (simplify (Op.Mod a a)) (imm (u256 0)))",
        ):
            with self.subTest(source):
                result = self.prove(source, "evm_simp 30")
                self.assertEqual(result["status"], "proved", result)

    def test_ring_proves_products_and_boolean_selects(self):
        for source in (
            """(rule (sequence_rewrite (Op.Sub (mul x r) (mul r y)))
              (if-let combined (make (Op.Sub x y)))
              (sequence (Op.Mul r combined)))""",
            """(rule (sequence_rewrite (Op.Select (and c (bool_value)) (sub y x) y))
              (if-let gated (make (Op.Mul c x)))
              (sequence (Op.Sub y gated)))""",
        ):
            with self.subTest(source):
                result = self.prove(source, "evm_ring")
                self.assertEqual(result["status"], "proved", result)


if __name__ == "__main__":
    unittest.main()
