"""Check rule theorems with Lean.

`lean_environment` builds the EVM semantics in `lean/EvmRules` and the `evm_check`
executable with the toolchain pinned in `lean/lean-toolchain`. Verification checks
each theorem in its own `lean` process (`job` and `prove`), so every rule has its own
time limit and failure. Search makes thousands of small queries instead: `Checker`
keeps one `evm_check` process that has imported the model, and answers each query in
milliseconds once it is warm.

A precondition set must also be satisfiable, as a rule that never applies proves
nothing. Its theorem `preconditions → False` must fail with an assignment, and the
integer evaluator in `expr.py` must confirm that the assignment satisfies every
precondition. Counterexamples to an equality are replayed the same way, so a reported
counterexample always differs on concrete words.
"""

import hashlib
import json
import os
import re
import select
import subprocess
import time
from dataclasses import dataclass
from math import ceil
from pathlib import Path

from .expr import Cond, Expr, Unsupported, concrete, holds
from .lean import (
    ADDRESS,
    BALANCES,
    COMMANDS,
    PRELUDE,
    applicability,
    simplify,
    word_variables,
)
from .lean import theorem as lean_theorem
from .lean import witness as lean_witness

LEAN_PROJECT = Path(__file__).resolve().parents[1] / "lean"
# Hand-written proof scripts, named after their rules, for obligations `evm_auto`
# cannot finish. Their lemmas live in `lean/EvmRules/Lemmas.lean` and `Arith.lean`.
MANUAL_PROOFS = LEAN_PROJECT / "proofs"
CHECKER = LEAN_PROJECT / ".lake/build/bin/evm_check"
ADDRESS_MASK = (1 << 160) - 1
# `evm_auto` reports which of its tactics proved the theorem.
PROVED_BY = re.compile(r"proved by (evm_arith|evm_decide)")


def lean_environment():
    """Build the semantics library and checker; return the `LEAN_PATH` that finds them."""
    subprocess.run(
        ["lake", "build"], cwd=LEAN_PROJECT, capture_output=True, text=True, check=True
    )
    return subprocess.run(
        ["lake", "env", "printenv", "LEAN_PATH"],
        cwd=LEAN_PROJECT,
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()


def library_digest():
    """Hash every input of a proof except the theorem itself, for proof caching."""
    digest = hashlib.sha256()
    for path in sorted(LEAN_PROJECT.glob("EvmRules/**/*.lean")) + [
        LEAN_PROJECT / "EvmRules.lean",
        LEAN_PROJECT / "lean-toolchain",
    ]:
        digest.update(path.relative_to(LEAN_PROJECT).as_posix().encode() + b"\0")
        digest.update(path.read_bytes() + b"\0")
    return digest.hexdigest()


def errors(output, path):
    """Each diagnostic for the checked file: (line, level, message)."""
    pattern = re.compile(
        rf"^{re.escape(str(path))}:(\d+):\d+: (error|warning): ", re.MULTILINE
    )
    matches = list(pattern.finditer(output))
    return [
        (
            int(match[1]),
            match[2],
            output[
                match.start() : matches[i + 1].start() if i + 1 < len(matches) else None
            ],
        )
        for i, match in enumerate(matches)
    ]


class Balances(dict):
    """A balance snapshot that gives every account a distinct balance, or zero."""

    def __init__(self, distinct):
        super().__init__()
        self.distinct = distinct

    def get(self, key, default=None):
        return key + 1 if self.distinct else 0


def environments(variables, values):
    """Concrete environments for replaying an assignment that reads account state."""
    if ADDRESS not in variables and BALANCES not in variables:
        return [None]
    address = values.get(ADDRESS, 0)
    return [
        {"address": address, "balances": Balances(distinct)}
        for distinct in (True, False)
    ]


def assignment(message, variables):
    """Split a reported assignment into words and flags, defaulting the rest."""
    values = lean_witness(message, variables)
    words = {
        name: values.get(name, 0) for name, sort in variables.items() if sort == "Word"
    }
    flags = {
        name: values.get(name, False)
        for name, sort in variables.items()
        if sort == "Bool"
    }
    return values, words, flags


def applicable(message, variables, assumptions):
    """Check the assignment of a failed contradiction proof in the integer semantics."""
    values, words, flags = assignment(message, variables)
    for environment in environments(variables, values):
        try:
            if all(holds(cond, words, flags, environment) for cond in assumptions):
                return {name: hex(value) for name, value in words.items()} | flags
        except Unsupported:
            continue
    return None


def observed(environment, exprs, words):
    """The executing account and every balance a replay reads, for the report."""
    balances = {}

    def visit(expr):
        for arg in expr.args:
            if isinstance(arg, Expr):
                visit(arg)
        if expr.op in ("balance", "selfbalance"):
            account = (
                concrete(expr.args[0], words, environment) & ADDRESS_MASK
                if expr.op == "balance"
                else environment["address"]
            )
            balances[hex(account)] = hex(environment["balances"].get(account, 0))

    for expr in exprs:
        visit(expr)
    return {"address": hex(environment["address"]), "balances": balances}


def counterexample(message, variables, lhs, rhs, assumptions):
    """Replay a reported counterexample; return its details only if the words differ."""
    values, words, flags = assignment(message, variables)
    for environment in environments(variables, values):
        try:
            left = concrete(lhs, words, environment)
            right = concrete(rhs, words, environment)
            if left != right and all(
                holds(cond, words, flags, environment) for cond in assumptions
            ):
                result = {
                    "status": "counterexample",
                    "inputs": {name: hex(value) for name, value in words.items()},
                    "lhs_value": hex(left),
                    "rhs_value": hex(right),
                    "replayed": True,
                }
                if environment is not None:
                    result["environment"] = observed(environment, [lhs, rhs], words)
                return result
        except Unsupported:
            continue
    return None


def ackermann(items):
    """Replace every balance read by a word with congruence, for counterexample search.

    `bv_decide` treats two reads of the balance function as unrelated words, so its
    assignments may read one account twice with different balances. Fresh words with
    `account_i = account_j → balance_i = balance_j` describe exactly the snapshots,
    so their counterexamples replay. Returns the rewritten items, the extra
    preconditions, and each fresh word's account.
    """
    reads = {}

    def replace(expr):
        if expr.op in ("var", "const"):
            return expr
        args = tuple(replace(arg) for arg in expr.args)
        if expr.op in ("balance", "selfbalance"):
            account = args[0] if expr.op == "balance" else Expr("address", ())
            account = Expr("and", (account, Expr.const(ADDRESS_MASK)))
            return reads.setdefault(account, Expr.var(f"@balance:{len(reads)}"))
        return Expr(expr.op, args)

    def rewrite(item):
        if isinstance(item, Expr):
            return replace(item)
        return Cond(
            item.op,
            tuple(rewrite(a) if isinstance(a, (Expr, Cond)) else a for a in item.args),
        )

    rewritten = [rewrite(item) for item in items]
    accounts = list(reads.items())
    congruence = [
        Cond("implies", (Cond("eq", (a, b)), Cond("eq", (word_a, word_b))))
        for i, (a, word_a) in enumerate(accounts)
        for b, word_b in accounts[i + 1 :]
    ]
    return rewritten, congruence, {word.args[0]: a for a, word in accounts}


@dataclass(frozen=True)
class Task:
    """One theorem file: the rule's statement and, with preconditions, a search for them."""

    work_dir: Path
    name: str
    text: str
    search_line: int | None
    variables: dict
    lhs: Expr
    rhs: Expr
    assumptions: list
    timeout: int
    lean_path: str


def job(work_dir, name, lhs, rhs, assumptions, tactic, timeout, lean_path):
    """The task that checks one theorem and, with preconditions, its applicability."""
    text = PRELUDE + "\n" + lean_theorem(name, lhs, rhs, assumptions, tactic)
    search_line, variables = None, {}
    if assumptions:
        search, variables = applicability(f"{name}_applicable", assumptions, timeout)
        search_line = text.count("\n") + 2
        text += "\n" + search
    limit = timeout * (2 if assumptions else 1) + 30
    return Task(
        work_dir,
        name,
        text,
        search_line,
        variables,
        lhs,
        rhs,
        list(assumptions),
        limit,
        lean_path,
    )


def prove(task):
    """Check one theorem file; the applicability search, if any, must fail with a witness.

    A failed proof whose counterexample replays in the integer semantics is reported as
    that counterexample.
    """
    name, text, search_line = task.name, task.text, task.search_line
    variables, assumptions = task.variables, task.assumptions
    directory = task.work_dir / name
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / "Proof.lean"
    path.write_text(text)
    start = time.monotonic()
    try:
        process = subprocess.run(
            ["lean", str(path)],
            cwd=LEAN_PROJECT,
            capture_output=True,
            text=True,
            timeout=task.timeout,
            check=False,
            env={**os.environ, "LEAN_PATH": task.lean_path},
        )
    except subprocess.TimeoutExpired:
        return name, {
            "status": "timeout",
            "seconds": round(time.monotonic() - start, 2),
        }
    result = {"seconds": round(time.monotonic() - start, 2)}
    output = process.stdout + process.stderr
    reported = errors(output, path)
    # The theorem must have no error, and no warning that it relies on `sorry`.
    failures = [
        message
        for line, level, message in reported
        if (search_line is None or line < search_line)
        and (level == "error" or "sorry" in message)
    ]
    if failures or (process.returncode != 0 and not reported):
        result.update(status="failed", output="\n".join(failures or [output])[-3000:])
        main = {}
        for item in (task.lhs, task.rhs, *assumptions):
            word_variables(item, main)
        for message in failures:
            if "counterexample" in message or "reduced to False" in message:
                found = counterexample(message, main, task.lhs, task.rhs, assumptions)
                if found is not None:
                    result.update(found)
                    break
        return name, result
    result["status"] = "proved"
    if tactic := PROVED_BY.search(output):
        result["tactic"] = tactic[1]
    if search_line is None:
        return name, result
    search = [
        message
        for line, level, message in reported
        if line >= search_line and level == "error"
    ]
    if not search:
        result.update(
            status="inapplicable", reason="the preconditions are contradictory"
        )
    elif "counterexample" not in search[0]:
        result.update(
            status="unknown",
            reason="the applicability search did not finish",
            output=search[0][-2000:],
        )
    elif (found := applicable(search[0], variables, assumptions)) is None:
        result.update(
            status="unknown",
            reason="the applicability witness does not satisfy the preconditions",
            output=search[0][-2000:],
        )
    else:
        result["witness"] = found
    return name, result


class Checker:
    """One `evm_check` process that decides small rule queries without restarting Lean.

    `check` returns the result shapes of a verification: `proved`, `inapplicable`, a
    replayed `counterexample`, or `unknown` with a reason. A query that outlives its
    limit restarts the process.
    """

    def __init__(self, lean_path=None):
        self.lean_path = lean_path if lean_path is not None else lean_environment()
        self.process = None
        self.queries = 0

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()

    def close(self):
        if self.process is not None:
            self.process.kill()
            self.process.wait()
            for stream in (self.process.stdin, self.process.stdout):
                if stream is not None:
                    stream.close()
            self.process = None

    def ask(self, source, timeout_s):
        """Elaborate commands in the model's environment; None when the time runs out."""
        if self.process is None:
            self.process = subprocess.Popen(
                [str(CHECKER)],
                cwd=LEAN_PROJECT,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,
                text=True,
                env={**os.environ, "LEAN_PATH": self.lean_path},
            )
        process = self.process
        assert process.stdin is not None and process.stdout is not None
        self.queries += 1
        process.stdin.write(json.dumps({"id": self.queries, "source": source}) + "\n")
        process.stdin.flush()
        ready, _, _ = select.select([process.stdout], [], [], timeout_s)
        line = process.stdout.readline() if ready else ""
        if not line:
            self.close()
            return None
        reply = json.loads(line)
        return reply["ok"], "\n".join(message["text"] for message in reply["messages"])

    def check(self, lhs, rhs, assumptions=(), timeout_ms=5000, tactic=None):
        """Decide `assumptions → lhs = rhs`, first requiring satisfiable assumptions.

        `tactic` replaces `evm_auto` for the equality, as a hand-written proof does.
        """
        assumptions = simplify(assumptions)
        witness = None
        seconds = max(1, ceil(timeout_ms / 1000))
        # The checker's own limit covers preprocessing as well as the SAT solver.
        limit = 2 * seconds + 30
        if assumptions:
            search, variables = applicability("query_applicable", assumptions, seconds)
            reply = self.ask(COMMANDS + "\n" + search, limit)
            if reply is None:
                return {"status": "unknown", "reason": "applicability search timed out"}
            ok, text = reply
            if ok:
                return {
                    "status": "inapplicable",
                    "reason": "the preconditions are contradictory",
                }
            witness = (
                applicable(text, variables, assumptions)
                if "counterexample" in text
                else None
            )
            if witness is None:
                return {"status": "unknown", "reason": "applicability is unconfirmed"}
        statement = lean_theorem(
            "query", lhs, rhs, assumptions, tactic or f"evm_auto {seconds}"
        )
        reply = self.ask(COMMANDS + "\n" + statement, limit)
        if reply is None:
            return {"status": "unknown", "reason": "the proof timed out"}
        ok, text = reply
        if ok:
            result = {"status": "proved"} | ({"witness": witness} if witness else {})
            if proved_by := PROVED_BY.search(text):
                result["tactic"] = proved_by[1]
            return result
        variables = {}
        for item in (lhs, rhs, *assumptions):
            word_variables(item, variables)
        # Preprocessing may refute the goal outright; then every input differs.
        if "counterexample" in text or "reduced to False" in text:
            if found := counterexample(text, variables, lhs, rhs, assumptions):
                return found
            if ADDRESS in variables or BALANCES in variables:
                return self.snapshot_counterexample(lhs, rhs, assumptions, seconds)
            return {"status": "unknown", "reason": "the counterexample did not replay"}
        if "timed out" in text:
            return {"status": "unknown", "reason": "the SAT solver timed out"}
        return {"status": "unknown", "reason": text[-500:]}

    def snapshot_counterexample(self, lhs, rhs, assumptions, seconds):
        """Search for a counterexample over explicit balance snapshots and replay it."""
        (left, right, *conditions), congruence, accounts = ackermann(
            [lhs, rhs, *assumptions]
        )
        statement = lean_theorem(
            "query", left, right, conditions + congruence, f"evm_decide {seconds}"
        )
        reply = self.ask(COMMANDS + "\n" + statement, 2 * seconds + 30)
        unknown = {"status": "unknown", "reason": "the counterexample did not replay"}
        if reply is None or reply[0] or "counterexample" not in reply[1]:
            return unknown
        variables = {}
        for item in (left, right, *conditions, *congruence):
            word_variables(item, variables)
        values, words, _ = assignment(reply[1], variables)
        address = values.get(ADDRESS, 0)
        environment = {"address": address, "balances": {}}
        for name, account in accounts.items():
            key = concrete(account, words, environment)
            environment["balances"].setdefault(key, words.get(name, 0))
        inputs = {k: v for k, v in words.items() if not k.startswith("@balance:")}
        try:
            before = concrete(lhs, inputs, environment)
            after = concrete(rhs, inputs, environment)
            holds_all = all(holds(c, inputs, {}, environment) for c in assumptions)
        except Unsupported:
            return unknown
        if before == after or not holds_all:
            return unknown
        return {
            "status": "counterexample",
            "inputs": {name: hex(value) for name, value in inputs.items()},
            "lhs_value": hex(before),
            "rhs_value": hex(after),
            "replayed": True,
            "environment": {
                "address": hex(address),
                "balances": {
                    hex(k): hex(v) for k, v in environment["balances"].items()
                },
            },
        }
