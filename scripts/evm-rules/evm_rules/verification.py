"""Verify ISLE rule files in Lean, one theorem per rule.

Readers turn each rule into words and preconditions; `lean.py` states them as a
theorem over the EVM semantics in `lean/EvmRules`, and `prover.py` checks it in its
own `lean` process. A physical stack rule has one theorem per distinct shape of its
per-depth variants: each variant is that theorem with its variables renamed.

An optional cache keeps proved results, keyed by the exact theorem file and a digest
of the Lean library and toolchain. Failures, timeouts and unknown results are never
cached, and a changed rule, proof, lemma or toolchain misses.
"""

import concurrent.futures
import hashlib
import json
import time

from .expr import Expr, Unsupported
from .isle import ISLE, Context, Rule, forms
from .late import late_obligation, late_rules
from .lean import canonical, simplify
from .prover import MANUAL_PROOFS, job, library_digest, prove
from .stack import stack_rules, stack_variants

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

    A theorem is a (name, lhs, rhs, preconditions) tuple.
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
        rule = Rule(form, line, str(path))
        entry = {"line": line, "name": f"{path.stem}_L{line}", "sha256": rule.digest}
        context = Context()
        try:
            lhs, rhs = context.obligation(rule)
            assumptions = simplify(context.assumptions)
            entry["theorems"] = [(entry["name"], lhs, rhs, assumptions)]
        except Unsupported as error:
            entry["error"] = str(error)
        entry["contracts"] = sorted(context.contracts)
        yield entry


def cache_key(digest, text):
    return hashlib.sha256(f"{digest}\0{text}".encode()).hexdigest()


def verify_files(
    paths,
    work_dir,
    lean_path,
    *,
    jobs,
    timeout_s,
    cache_dir=None,
    progress=print,
    checker=None,
):
    """Check every rule of `paths`; return per-file results and status counts.

    With a `checker`, every theorem is answered by that one process instead of a
    `lean` process per theorem: faster for small files, with the same statuses.
    """
    tactic = f"evm_decide {timeout_s}"
    manual = {path.stem: path.read_text() for path in MANUAL_PROOFS.glob("*.lean")}
    digest = library_digest() if cache_dir is not None else None
    files, tasks, proofs = [], [], {}
    # A proof for a selected file must name one of its current rules.
    stems = {path.stem for path in paths}
    unused = {name for name in manual if name.rsplit("_L", 1)[0] in stems}
    for path in paths:
        source = path.read_text()
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
            for name, lhs, rhs, assumptions in entry.get("theorems", []):
                unused.discard(name)
                if checker is not None:
                    start = time.monotonic()
                    try:
                        result = checker.check(
                            lhs, rhs, assumptions, timeout_s * 1000, manual.get(name)
                        )
                    except Unsupported as error:
                        entry["error"] = str(error)
                        continue
                    result["seconds"] = round(time.monotonic() - start, 2)
                    result["method"] = "manual" if name in manual else "evm_decide"
                    proofs[name] = result
                    continue
                try:
                    task = job(
                        work_dir,
                        name,
                        lhs,
                        rhs,
                        assumptions,
                        manual.get(name, tactic),
                        timeout_s,
                        lean_path,
                    )
                except Unsupported as error:
                    entry["error"] = str(error)
                    continue
                cached = None
                if cache_dir is not None:
                    cached = cache_dir / f"{cache_key(digest, task.text)}.json"
                    if cached.exists():
                        proofs[name] = json.loads(cached.read_text()) | {"cached": True}
                        continue
                tasks.append((task, cached))
    if unused:
        raise ValueError(f"hand-written proofs without a rule: {sorted(unused)}")
    with concurrent.futures.ThreadPoolExecutor(jobs) as pool:
        for (_, cached), (name, result) in zip(
            tasks, pool.map(prove, [task for task, _ in tasks])
        ):
            result["method"] = "manual" if name in manual else "evm_decide"
            proofs[name] = result
            if cached is not None and result["status"] == "proved":
                cached.parent.mkdir(parents=True, exist_ok=True)
                cached.write_text(json.dumps(result))
            if progress is not None:
                progress(f"{result['status']:12s} {result['seconds']:7.2f}s {name}")
    counts = {}
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
            counts[entry["status"]] = counts.get(entry["status"], 0) + 1
    return {"tactic": tactic, "files": files, "counts": counts}
