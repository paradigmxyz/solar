# /// script
# requires-python = ">=3.14"
# ///
"""Prove the actual ISLE rules in Lean against EVM semantics written in Lean.

The rule reader builds every obligation as solver-independent terms, and
`evm_rules.lean` states it as a theorem over the definitions in
`lean/EvmRules/Word.lean`. `lean` checks each theorem with the toolchain pinned
in `lean/lean-toolchain`. No SMT solver is involved; see README.md.
"""

import argparse
import concurrent.futures
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

from evm_rules.expr import Expr, Unsupported, holds
from evm_rules.isle import ISLE, Context, Rule, forms
from evm_rules.late import late_obligation, late_rules
from evm_rules.lean import PRELUDE, applicability, canonical, simplify, theorem, witness
from evm_rules.stack import stack_rules, stack_variants

LEAN_PROJECT = Path(__file__).resolve().parent / "lean"
# Hand-written proof scripts, by theorem name, for obligations `evm_decide` cannot
# finish. Their lemmas live in `lean/EvmRules/Lemmas.lean`.
MANUAL_PROOFS = LEAN_PROJECT / "proofs"
DEFAULT_FILES = [
    ISLE / "mir/word.isle",
    ISLE / "mir/word_sequence.isle",
    ISLE / "mir-to-evm/stack_select.isle",
    ISLE / "evm-ir/stack_peephole.isle",
    ISLE / "evm-ir/late_word.isle",
    ISLE / "mir/egraph.isle",
]
FAILURES = (Unsupported, ValueError, TypeError, IndexError)


def obligations(path):
    """Yield one entry per rule, with its theorems or the reason it is unsupported.

    A theorem is a (name, lhs, rhs, preconditions) tuple. A physical stack rule
    has one theorem per distinct shape of its per-depth variants: each variant
    is that theorem with its variables renamed.
    """
    if path.name == "stack_peephole.isle":
        _, rules = stack_rules(path)
        for rule in rules:
            entry = {"line": rule.line, "name": f"{path.stem}_L{rule.line}"}
            try:
                shapes = {}
                for variant in stack_variants(rule):
                    shape = canonical(variant["difference"])
                    shapes[shape] = shapes.get(shape, 0) + 1
                entry["variants"] = sum(shapes.values())
                entry["theorems"] = [
                    (f"{entry['name']}_{index}", shape, Expr.const(0), [])
                    for index, shape in enumerate(shapes)
                ]
            except FAILURES as error:
                entry["error"] = str(error)
            yield entry
        return
    if path.name == "late_word.isle":
        _, rules = late_rules(path)
        for rule in rules:
            entry = {"line": rule.line, "name": f"{path.stem}_L{rule.line}"}
            try:
                details, lhs, rhs = late_obligation(rule)
                entry.update(details=details, theorems=[(entry["name"], lhs, rhs, [])])
            except FAILURES as error:
                entry["error"] = str(error)
            yield entry
        return
    for form, line in forms(path.read_text()):
        if form[0] != "rule":
            continue
        entry = {"line": line, "name": f"{path.stem}_L{line}"}
        context = Context()
        try:
            lhs, rhs = context.obligation(Rule(form, line, str(path)))
            assumptions = simplify(context.assumptions)
            entry["theorems"] = [(entry["name"], lhs, rhs, assumptions)]
        except Unsupported as error:
            entry["error"] = str(error)
        entry["contracts"] = sorted(context.contracts)
        yield entry


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


def applicable(message, variables, assumptions):
    """Check the assignment of a failed contradiction proof in the integer semantics."""
    values = witness(message, variables)
    words = {
        name: values.get(name, 0) for name, sort in variables.items() if sort == "Word"
    }
    flags = {
        name: values.get(name, False)
        for name, sort in variables.items()
        if sort == "Bool"
    }
    if all(holds(cond, words, flags) for cond in assumptions):
        return {name: hex(value) for name, value in words.items()} | flags
    return None


def prove(task):
    """Check one theorem file; the applicability search, if any, must fail with a witness."""
    work_dir, name, text, search_line, variables, assumptions, timeout, lean_path = task
    directory = work_dir / name
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
            timeout=timeout,
            check=False,
            env={**os.environ, "LEAN_PATH": lean_path},
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
        return name, result
    result["status"] = "proved"
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


def lean_environment():
    """Build the semantics library and return the `LEAN_PATH` that finds it."""
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


def job(work_dir, name, lhs, rhs, assumptions, tactic, timeout, lean_path):
    """The task that checks one theorem and, with preconditions, its applicability."""
    text = PRELUDE + "\n" + theorem(name, lhs, rhs, assumptions, tactic)
    search_line, variables = None, {}
    if assumptions:
        search, variables = applicability(f"{name}_applicable", assumptions, timeout)
        search_line = text.count("\n") + 2
        text += "\n" + search
    limit = timeout * (2 if assumptions else 1) + 30
    return work_dir, name, text, search_line, variables, assumptions, limit, lean_path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("files", nargs="*", type=Path, default=DEFAULT_FILES)
    parser.add_argument("--jobs", type=int, default=os.cpu_count() or 4)
    parser.add_argument(
        "--timeout-s",
        type=int,
        default=120,
        help="SAT limit per proof; each rule may run twice that, plus 30 seconds",
    )
    parser.add_argument("--work-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    try:
        lean_path = lean_environment()
    except subprocess.CalledProcessError as error:
        print(error.stdout + error.stderr, file=sys.stderr)
        return 1
    tactic = f"evm_decide {args.timeout_s}"
    manual = {path.stem: path.read_text() for path in MANUAL_PROOFS.glob("*.lean")}
    rules, tasks, unused = [], [], set(manual)
    for path in args.files:
        for entry in obligations(path):
            entry["file"] = path.name
            rules.append(entry)
            for name, lhs, rhs, assumptions in entry.get("theorems", []):
                unused.discard(name)
                try:
                    tasks.append(
                        job(
                            args.work_dir,
                            name,
                            lhs,
                            rhs,
                            assumptions,
                            manual.get(name, tactic),
                            args.timeout_s,
                            lean_path,
                        )
                    )
                except Unsupported as error:
                    entry["error"] = str(error)
    if unused:
        print(f"hand-written proofs without a rule: {sorted(unused)}", file=sys.stderr)
        return 1
    proofs = {}
    with concurrent.futures.ThreadPoolExecutor(args.jobs) as pool:
        for name, result in pool.map(prove, tasks):
            result["method"] = "manual" if name in manual else "evm_decide"
            proofs[name] = result
            print(
                f"{result['status']:12s} {result['seconds']:7.2f}s {name}", flush=True
            )
    counts = {}
    for entry in rules:
        theorems = [proofs.get(name, {}) for name, *_ in entry.pop("theorems", [])]
        if "error" in entry:
            entry["status"] = "unsupported"
        else:
            entry["status"] = next(
                (
                    p.get("status", "unsupported")
                    for p in theorems
                    if p.get("status") != "proved"
                ),
                "proved",
            )
            entry["proofs"] = theorems
        counts[entry["status"]] = counts.get(entry["status"], 0) + 1
    report = {"tactic": tactic, "counts": counts, "rules": rules}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=1, default=str))
    print(json.dumps(counts))
    return 0 if set(counts) == {"proved"} else 1


if __name__ == "__main__":
    sys.exit(main())
