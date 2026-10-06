"""Verify ISLE rule files in Lean, one theorem per rule.

Readers turn each rule into words and preconditions; `lean.py` states them as a
theorem over the EVM semantics in `lean/EvmRules`, and `prover.py` checks it in a
reusable worker process. `evm_auto` proves it over natural numbers with `evm_arith`, by
bit-blasting with `evm_decide`, bit by bit with `evm_bits` or as a ring identity with
`evm_ring`; a hand-written proof replaces them. A physical stack
rule has one theorem per distinct shape of its per-depth variants: each variant is
that theorem with its variables renamed.

Every run proves every selected theorem and checks its applicability again: no
earlier verdict is reused, so a change to the readers, the result validation or the
applicability check applies to every rule at once.
"""

import concurrent.futures
import hashlib
import re
import time
from contextlib import ExitStack
from queue import SimpleQueue

from .expr import Expr, Unsupported
from .isle import ISLE, Context, Rule, forms, rule_sources
from .late import late_obligation, late_rules
from .lean import canonical, simplify
from .prover import MANUAL_PROOFS, Checker, job, prove
from .stack import stack_rules, stack_variants

DEFAULT_FILES = [
    ISLE / "mir/word",
    ISLE / "mir/word_sequence",
    ISLE / "mir-to-evm/stack_select.isle",
    ISLE / "evm-ir/stack_peephole.isle",
    ISLE / "evm-ir/late_word.isle",
    ISLE / "mir/egraph",
]
FAILURES = (Unsupported, ValueError, TypeError, IndexError)
# A hand-written proof is named after its rule's file and digest, which edits elsewhere
# in the file leave unchanged; a rule with several theorems adds the theorem's index.
PROOF_NAME = re.compile(r"(?P<stem>.+)_(?P<digest>[0-9a-f]{16})(?:_(?P<index>\d+))?")


def proof_name(path, entry, index):
    """The hand-written proof file name for one theorem of a rule."""
    suffix = f"_{index}" if len(entry.get("theorems", [])) > 1 else ""
    return f"{path.stem}_{entry['sha256'][:16]}{suffix}"


def obligations(path):
    """Yield one entry per rule, with its theorems or the reason it is unsupported.

    A theorem is a (name, lhs, rhs, preconditions) tuple.
    """
    if path.name == "stack_peephole.isle":
        _, rules = stack_rules(path)
        for rule in rules:
            entry = {
                "line": rule.line,
                "name": f"{path.stem}_L{rule.line}",
                "sha256": rule.digest,
            }
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
            entry = {
                "line": rule.line,
                "name": f"{path.stem}_L{rule.line}",
                "sha256": rule.digest,
            }
            try:
                details, lhs, rhs = late_obligation(rule)
                entry.update(details=details, theorems=[(entry["name"], lhs, rhs, [])])
            except FAILURES as error:
                entry["error"] = str(error)
            yield entry
        return
    for module, text in rule_sources(path):
        # Line numbers repeat across the modules of a rule-set directory.
        prefix = f"{path.name}_{module.stem}" if path.is_dir() else path.stem
        for form, line in forms(text):
            if form[0] != "rule":
                continue
            rule = Rule(form, line, str(module))
            entry = {
                "source": str(module),
                "line": line,
                "name": f"{prefix}_L{line}",
                "sha256": rule.digest,
            }
            context = Context()
            try:
                lhs, rhs = context.obligation(rule)
                assumptions = simplify(context.assumptions)
                entry["theorems"] = [(entry["name"], lhs, rhs, assumptions)]
            except Unsupported as error:
                entry["error"] = str(error)
            entry["contracts"] = sorted(context.contracts)
            yield entry


def verify_files(
    paths,
    work_dir,
    lean_path,
    *,
    jobs,
    timeout_s,
    progress=print,
    checker=None,
    isolated=False,
):
    """Check every rule of `paths`; return per-file results and status counts.

    Workers import the model once, but check every theorem in a fresh environment.
    `isolated` starts a separate `lean` process per theorem. An explicit `checker`
    answers all obligations sequentially, as in discovery tests.
    """
    tactic = f"evm_auto {timeout_s}"
    manual = {path.stem: path.read_text() for path in MANUAL_PROOFS.glob("*.lean")}
    files, tasks, proofs = [], [], {}
    # A proof for a selected file must name one of its current rules.
    stems = {path.stem for path in paths}
    unused = {
        name
        for name in manual
        if (match := PROOF_NAME.fullmatch(name)) and match["stem"] in stems
    }
    for path in paths:
        source = "".join(text for _, text in rule_sources(path))
        rules = list(obligations(path))
        if not rules:
            raise ValueError(f"no rules in {path}")
        files.append(
            {
                "source": str(path),
                "source_sha256": hashlib.sha256(source.encode()).hexdigest(),
                "rules": rules,
            }
        )
        for entry in rules:
            for index, (name, lhs, rhs, assumptions) in enumerate(
                entry.get("theorems", [])
            ):
                key = proof_name(path, entry, index)
                unused.discard(key)
                proof = manual.get(key)
                if checker is not None:
                    start = time.monotonic()
                    try:
                        result = checker.check(
                            lhs, rhs, assumptions, timeout_s * 1000, proof
                        )
                    except Unsupported as error:
                        entry["error"] = str(error)
                        continue
                    result["seconds"] = round(time.monotonic() - start, 2)
                    result["method"] = "manual" if proof else result.pop("tactic", "")
                    proofs[name] = result
                    continue
                try:
                    task = job(
                        work_dir,
                        name,
                        lhs,
                        rhs,
                        assumptions,
                        proof or tactic,
                        timeout_s,
                        lean_path,
                    )
                except Unsupported as error:
                    entry["error"] = str(error)
                    continue
                tasks.append((task, proof is not None))
    if unused:
        raise ValueError(f"hand-written proofs without a rule: {sorted(unused)}")
    workers = SimpleQueue()

    def run(task):
        if isolated:
            return prove(task)
        worker = workers.get()
        try:
            return prove(task, worker)
        finally:
            workers.put(worker)

    with ExitStack() as stack, concurrent.futures.ThreadPoolExecutor(jobs) as pool:
        if not isolated:
            for _ in range(min(jobs, len(tasks))):
                workers.put(stack.enter_context(Checker(lean_path)))
        for (_, manual_proof), (name, result) in zip(
            tasks, pool.map(run, [task for task, _ in tasks])
        ):
            result["method"] = "manual" if manual_proof else result.pop("tactic", "")
            proofs[name] = result
            if progress is not None:
                progress(f"{result['status']:12s} {result['seconds']:7.2f}s {name}")
    counts, methods = {}, {}
    for file in files:
        for entry in file["rules"]:
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
                for proof in theorems:
                    if proof.get("status") == "proved":
                        method = proof.get("method") or "unrecorded"
                        methods[method] = methods.get(method, 0) + 1
            counts[entry["status"]] = counts.get(entry["status"], 0) + 1
    return {"tactic": tactic, "files": files, "counts": counts, "methods": methods}
