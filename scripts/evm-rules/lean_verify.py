# /// script
# requires-python = ">=3.14"
# dependencies = ["z3-solver==4.16.0.0"]
# ///
"""Prove the actual ISLE rules' word obligations in Lean instead of Z3 and cvc5.

Each rule's whole query is built exactly as `verify.py` builds it, translated
into a Lean theorem, and checked by `lean` with the toolchain pinned in
`lean/lean-toolchain`. Z3 only constructs the formulas here; no SMT answer is
used. See README.md.
"""

import argparse
import concurrent.futures
import json
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

from evm_rules.isle import ISLE, Context, Rule, forms
from evm_rules.late import verify_late_file
from evm_rules.lean import THEOREM_PRELUDE, UnsupportedQuery, theorem
from evm_rules.semantics import Model, Unsupported, guarded_constants, portable_query

LEAN_PROJECT = Path(__file__).resolve().parent / "lean"
# Hand-written proof scripts, by theorem name, for obligations `bv_decide` cannot finish,
# and the lemmas they share.
MANUAL_PROOFS = LEAN_PROJECT / "proofs"
HELPERS = LEAN_PROJECT / "Helpers.lean"
DEFAULT_FILES = [
    ISLE / "mir/word.isle",
    ISLE / "mir/word_sequence.isle",
    ISLE / "mir-to-evm/stack_select.isle",
    ISLE / "evm-ir/late_word.isle",
    ISLE / "mir/egraph.isle",
]


def whole_query(rule):
    """Build the rule's complete UNSAT query as `semantics.check` does, without solving it."""
    context = Context()
    lhs, rhs = context.obligation(rule)
    model = context.model
    try:
        left, right = model.eval(lhs), model.eval(rhs)
    except Unsupported:
        constants = guarded_constants(context.assumptions)
        if not constants:
            raise
        model = Model(constants)
        left, right = model.eval(lhs), model.eval(rhs)
    solver = model.solver()
    solver.add(*context.assumptions, model.difference(left, right))
    return portable_query(solver)


def obligations(path):
    """Yield (line, query) for every word rule in an ISLE file."""
    if path.name == "late_word.isle":
        # The late lane builds stack-window queries with its own reader.
        with tempfile.TemporaryDirectory() as directory:
            report = verify_late_file(path, artifacts=Path(directory))
            for result in report["rules"]:
                for query in result.get("smt2", []):
                    yield result["line"], Path(query).read_text()
        return
    for form, line in forms(path.read_text()):
        if form[0] != "rule":
            continue
        try:
            yield line, whole_query(Rule(form, line, str(path)))
        except Unsupported as error:
            yield line, error


def prove(task):
    work_dir, name, source, timeout = task
    directory = work_dir / name
    directory.mkdir(parents=True, exist_ok=True)
    (directory / "Proof.lean").write_text(source)
    start = time.monotonic()
    try:
        process = subprocess.run(
            ["lean", str(directory / "Proof.lean")],
            cwd=LEAN_PROJECT,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )
        seconds = time.monotonic() - start
        output = (process.stdout + process.stderr).strip()
        status = (
            "proved" if process.returncode == 0 and "error" not in output else "failed"
        )
        return name, status, seconds, output[-2000:]
    except subprocess.TimeoutExpired:
        return name, "timeout", time.monotonic() - start, ""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("files", nargs="*", type=Path, default=DEFAULT_FILES)
    parser.add_argument("--jobs", type=int, default=os.cpu_count() or 4)
    parser.add_argument(
        "--timeout-s",
        type=int,
        default=120,
        help="SAT limit per rule; each rule may run 30 seconds longer in total",
    )
    parser.add_argument(
        "--tactic",
        default="bv_decide (config := {{ timeout := {timeout} }})",
        help="proof script; {timeout} becomes the SAT solver's limit in seconds",
    )
    parser.add_argument("--work-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    tactic = args.tactic.format(timeout=args.timeout_s)
    manual = {path.stem: path.read_text() for path in MANUAL_PROOFS.glob("*.lean")}
    results, tasks = [], []
    for path in args.files:
        for line, query in obligations(path):
            name = f"{path.stem}_L{line}"
            result = {"file": path.name, "line": line, "name": name}
            results.append(result)
            if isinstance(query, Exception):
                result.update(status="unsupported", reason=str(query))
                continue
            result["method"] = "manual" if name in manual else "bv_decide"
            prelude = (
                THEOREM_PRELUDE + "\n" + (HELPERS.read_text() if name in manual else "")
            )
            try:
                text = prelude + "\n" + theorem(name, query, manual.get(name, tactic))
            except UnsupportedQuery as error:
                result.update(status="unsupported", reason=str(error))
                continue
            result["query_bytes"] = len(query)
            tasks.append((args.work_dir, name, text, args.timeout_s + 30))
    by_name = {result["name"]: result for result in results}
    with concurrent.futures.ThreadPoolExecutor(args.jobs) as pool:
        for name, status, seconds, output in pool.map(prove, tasks):
            by_name[name].update(status=status, seconds=round(seconds, 2))
            if status != "proved":
                by_name[name]["output"] = output
            print(f"{status:9s} {seconds:7.2f}s {name}", flush=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps({"tactic": tactic, "rules": results}, indent=1))
    counts = {}
    for result in results:
        counts[result["status"]] = counts.get(result["status"], 0) + 1
    print(json.dumps(counts))
    return 0 if counts.get("proved", 0) == len(results) else 1


if __name__ == "__main__":
    sys.exit(main())
