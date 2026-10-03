"""Lean proof execution and complete source-rule coverage reports."""

import hashlib
import tempfile
from concurrent.futures import ThreadPoolExecutor
from functools import cache
from pathlib import Path

from .isle import ISLE, ROOT, Context, Rule, forms
from .late import obligations as late_obligations
from .lean import Lean, compile_source, prepare, theorem, witness
from .semantics import Unsupported, concrete
from .stack import obligations as stack_obligations

FILES = tuple(
    ISLE / (name + ".isle")
    for name in (
        "mir/word",
        "mir/word_sequence",
        "mir/egraph",
        "mir-to-evm/stack_select",
        "evm-ir/stack_peephole",
        "evm-ir/late_word",
    )
)


@cache
def library():
    directory = ROOT / "target/evm-rules/library"
    return prepare(directory)


def obligations(path):
    if path.name == "stack_peephole.isle":
        return stack_obligations(path)
    if path.name == "late_word.isle":
        return late_obligations(path)
    result = []
    for form, line in forms(path.read_text()):
        if form[0] == "rule":
            rule = Rule(form, line, str(path))
            context = Context()
            lhs, rhs = context.obligation(rule)
            result.append(
                (
                    rule,
                    [(lhs, rhs)],
                    context.assumptions,
                    {"contracts": sorted(context.contracts)},
                )
            )
    if not result:
        raise Unsupported(f"no rules in {path}")
    return result


def check(lhs, rhs, assumptions=(), timeout_ms=60000, directory=None, name="proof"):
    library()
    if directory is None:
        with tempfile.TemporaryDirectory() as temporary:
            return check(lhs, rhs, assumptions, timeout_ms, Path(temporary), name)
    directory.mkdir(parents=True, exist_ok=True)
    try:
        values = witness(assumptions, directory, name)
        source = theorem(lhs, rhs, assumptions, name, values)
    except Unsupported as error:
        return {"status": "unknown", "reason": str(error)}
    path = directory / f"{name}.lean"
    path.write_text(source)
    result = compile_source(path, max(timeout_ms / 1000, 1))
    (directory / f"{name}.log").write_text(result.stdout + result.stderr)
    details = {
        "proof": str(path),
        "proof_sha256": hashlib.sha256(source.encode()).hexdigest(),
    }
    if result.returncode == 0:
        return {"status": "proved", **details}
    if "The prover found a counterexample" in result.stdout:
        replay_path = directory / f"{name}_counterexample.lean"
        replay_path.write_text(
            theorem(lhs, rhs, assumptions, name, values, counterexample=True)
        )
        result = compile_source(replay_path, max(timeout_ms / 1000, 1))
        replay_path.with_suffix(".log").write_text(result.stdout + result.stderr)
        variables = (
            lhs.variables()
            | rhs.variables()
            | set().union(*(a.variables() for a in assumptions))
        )
        emitter = Lean(variables)
        inputs = emitter.assignments(result.stdout)
        try:
            if all(concrete(a, inputs) for a in assumptions) and concrete(
                lhs, inputs
            ) != concrete(rhs, inputs):
                return {
                    "status": "counterexample",
                    "inputs": {k: hex(v) for k, v in inputs.items()},
                    "replayed": True,
                    **details,
                }
        except Unsupported:
            pass
    return {
        "status": "unknown",
        "reason": (result.stdout + result.stderr)[-8192:],
        **details,
    }


def verify_file(path, timeout_ms=60000, artifacts=None, jobs=2):
    library()
    directory = artifacts or ROOT / "target/evm-rules/proofs"
    directory.mkdir(parents=True, exist_ok=True)
    try:
        entries = obligations(path)
    except Unsupported as error:
        return {
            "source": str(path),
            "source_sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            "rules": [{"line": 0, "status": "unsupported", "reason": str(error)}],
        }
    prefix = hashlib.sha256(str(path).encode()).hexdigest()[:12]

    def prove(entry):
        index, (rule, equalities, assumptions, metadata) = entry
        results = []
        for variant, (lhs, rhs) in enumerate(equalities):
            name = f"rule_{prefix}_{index}_{variant}"
            result = check(lhs, rhs, assumptions, timeout_ms, directory, name)
            results.append(result)
        status = next(
            (r["status"] for r in results if r["status"] != "proved"), "proved"
        )
        return {
            "line": rule.line,
            "rule_sha256": rule.digest,
            "status": status,
            "obligations": results,
            **(
                {
                    "reason": next(
                        r.get("reason", r["status"])
                        for r in results
                        if r["status"] != "proved"
                    )
                }
                if status != "proved"
                else {}
            ),
            **metadata,
        }

    with ThreadPoolExecutor(max_workers=jobs) as pool:
        results = list(pool.map(prove, enumerate(entries)))
    return {
        "source": str(path),
        "source_sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        "rules": results,
    }
