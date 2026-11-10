#!/usr/bin/env python3
"""Prove the compiler's value-range analysis facts for all inputs with Z3.

Compiles each Solidity source with `-Zprove-analyses`, which writes one SMT-LIB
query per fact the run-time checks of `-Zcheck-analyses` would test, then runs
Z3 on every query. `unsat` proves the fact for every execution; `sat` is a
counterexample to triage: an analysis bug, or a case the encoding is too coarse
to rule out (see the docs of `prove_analyses.rs`). `sat-opaque` marks a
counterexample whose goal depends on values the query leaves free (loads,
calls, earlier loop passes), which usually means a missing fact; a plain
`sat` depends only on transaction inputs and is worth replaying. `trip-count` marks a
counterexample to a loop counter's gas-derived bound, which the queries take
as an axiom.

    uv run --with z3-solver python scripts/prove-analyses.py tests/ui/codegen
"""

from __future__ import annotations

import argparse
import collections
import concurrent.futures
import hashlib
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import uuid

ROOT = pathlib.Path(__file__).resolve().parent.parent


def z3_binary(explicit: str | None) -> str:
    if explicit:
        return explicit
    candidate = pathlib.Path(sys.executable).parent / "z3"
    if candidate.exists():
        return str(candidate)
    found = shutil.which("z3")
    if not found:
        sys.exit("z3 not found: pass --z3 or run with `uv run --with z3-solver`")
    return found


def sources(paths: list[str]) -> list[pathlib.Path]:
    out: list[pathlib.Path] = []
    for path in map(pathlib.Path, paths):
        if path.is_dir():
            out.extend(sorted(path.rglob("*.sol")))
        else:
            out.append(path)
    return [p for p in out if "auxiliary" not in p.parts]


def compile_one(solar: str, source: pathlib.Path, level: str, out: pathlib.Path) -> int:
    text = source.read_text(errors="replace")
    flags: list[str] = []
    for match in re.finditer(r"^//@ compile-flags:(.*)$", text, re.MULTILINE):
        flags += [f for f in match.group(1).split() if f.startswith("--evm-version")]
    out.mkdir(parents=True, exist_ok=True)
    command = [
        solar,
        "-O",
        level,
        f"-Zprove-analyses={out}",
        "--emit=bin-runtime",
        *flags,
        str(source),
    ]
    try:
        subprocess.run(
            command, capture_output=True, timeout=300, cwd=source.parent, check=False
        )
    except subprocess.TimeoutExpired:
        return 0
    return sum(not q.name.endswith(".abstract.smt2") for q in out.glob("*.smt2"))


def header_of(query: pathlib.Path) -> dict:
    header: dict = {}
    for line in query.read_text().splitlines():
        if not line.startswith(";"):
            break
        key, _, value = line.lstrip("; ").partition(": ")
        header[key] = value
    return header


def body_of(text: str) -> str:
    """A query's declarations and assertions, without its logic, check or model commands."""
    keep = [
        line for line in text.splitlines() if line.startswith(("(declare", "(assert"))
    ]
    return "\n".join(keep)


def run_z3(z3: str, script: str, timeout: float, memory: int) -> list[str]:
    try:
        result = subprocess.run(
            [z3, "-in", "-smt2", f"-memory:{memory}"],
            input=script,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )
    except subprocess.TimeoutExpired:
        return ["timeout"]
    return result.stdout.splitlines()


class Solver:
    """Runs Z3 with deterministic resource limits, as Vox does.

    A function's queries are first tried as one batch in one process, each
    under the low `--rlimit-batch` budget and in its abstract tier when it
    has one. Queries the batch does not prove are retried alone in the exact
    tier under the full `--rlimit`. Resource counts do not depend on machine
    load, so a run's verdicts are reproducible; `--timeout` is only a
    wall-clock backstop. Verdicts are cached by solver, limits and query text.
    """

    def __init__(self, args: argparse.Namespace, z3: str) -> None:
        self.z3 = z3
        self.batch_limit = args.rlimit_batch
        self.limit = args.rlimit
        self.timeout = args.timeout
        self.memory = args.memory
        version = subprocess.run(
            [z3, "--version"], capture_output=True, text=True, check=False
        ).stdout.strip()
        self.salt = f"{version}|{self.batch_limit}|{self.limit}"
        self.cache = None if args.no_cache else pathlib.Path(args.cache)
        if self.cache:
            self.cache.mkdir(parents=True, exist_ok=True)

    def key(self, query: pathlib.Path) -> str:
        digest = hashlib.sha256(self.salt.encode())
        digest.update(query.read_bytes())
        abstract = abstract_of(query)
        if abstract.exists():
            digest.update(abstract.read_bytes())
        return digest.hexdigest()

    def cached(self, key: str) -> dict | None:
        if not self.cache:
            return None
        try:
            return json.loads((self.cache / f"{key}.json").read_text())
        except OSError:
            return None
        except ValueError:
            return None

    def store(self, key: str, record: dict) -> None:
        if self.cache:
            # Write and rename, so an interrupted run leaves no partial entry.
            path = self.cache / f"{key}.json"
            partial = path.with_name(f"{path.name}.{uuid.uuid4().hex}.tmp")
            partial.write_text(json.dumps(record))
            partial.replace(path)

    def solve_group(self, queries: list[pathlib.Path], source: str) -> list[dict]:
        records: dict[pathlib.Path, dict] = {}
        keys = {query: self.key(query) for query in queries}
        pending = []
        for query in queries:
            hit = self.cached(keys[query])
            if hit is not None:
                records[query] = {
                    **hit,
                    "query": str(query),
                    "source": source,
                    "cached": True,
                }
            else:
                pending.append(query)
        for query, verdict in zip(pending, self.batch(pending)):
            if verdict == "unsat":
                tier = "abstract" if abstract_of(query).exists() else "exact"
                records[query] = self.record(
                    query, source, "unsat", tier=f"batch-{tier}"
                )
        for query in pending:
            if query not in records:
                records[query] = self.alone(query, source)
            self.store(
                keys[query],
                {
                    k: v
                    for k, v in records[query].items()
                    if k not in ("query", "source")
                },
            )
        return [records[query] for query in queries]

    def batch(self, queries: list[pathlib.Path]) -> list[str]:
        if not queries:
            return []
        script = ["(set-option :print-success false)", "(set-logic QF_UFBV)"]
        script.append(f"(set-option :rlimit {self.batch_limit})")
        for index, query in enumerate(queries):
            tier = abstract_of(query)
            text = (tier if tier.exists() else query).read_text()
            script += [
                "(push)",
                body_of(text),
                f'(echo "@{index}")',
                "(check-sat)",
                "(pop)",
            ]
        lines = run_z3(
            self.z3,
            "\n".join(script) + "\n",
            self.timeout * len(queries) + 10,
            self.memory,
        )
        verdicts = ["unknown"] * len(queries)
        current = None
        for line in lines:
            if line.startswith("@") and line[1:].isdigit():
                current = int(line[1:])
            elif current is not None and line in ("sat", "unsat", "unknown"):
                verdicts[current] = line
                current = None
        return verdicts

    def alone(self, query: pathlib.Path, source: str) -> dict:
        script = f"(set-option :rlimit {self.limit})\n" + query.read_text()
        lines = run_z3(self.z3, script, self.timeout + 10, self.memory)
        verdict = lines[0] if lines else "error"
        if verdict.startswith("(error"):
            verdict = "memout" if "memory" in verdict else "error"
        record = self.record(query, source, verdict, tier="exact")
        if verdict == "sat" and len(lines) > 1:
            record["counterexample"] = lines[1]
            if is_trip_count_bound(record):
                record["result"] = "trip-count"
            elif record.get("claim", "").split()[0] in ("cse", "load_pre"):
                # The queries do not model memory or storage contents, so a read
                # forwarded from an earlier value cannot be proved here.
                record["result"] = "memory"
            elif record.get("opaque"):
                # The model picks values for loads, calls or loop state the query
                # does not follow: it may show a missing fact rather than a bug.
                record["result"] = "sat-opaque"
        return record

    def record(self, query: pathlib.Path, source: str, verdict: str, tier: str) -> dict:
        return {
            "query": str(query),
            "source": source,
            "result": verdict,
            "tier": tier,
            **header_of(query),
        }


def abstract_of(query: pathlib.Path) -> pathlib.Path:
    return query.with_name(query.name.replace(".smt2", ".abstract.smt2"))


def is_trip_count_bound(record: dict) -> bool:
    """Whether a counterexample is to the bound check-elim derives from the gas limit.

    A counter with a constant start and step cannot travel more than
    `step << 64` from its start, since every iteration burns gas. The queries
    do not model gas, so these claims are axioms the run-time checks test.
    """
    if record.get("definition") != "phi" or not record.get("claim", "").startswith(
        ("check-elim", "check_elim")
    ):
        return False
    bounds = re.findall(r"0x[0-9a-f]+", record["claim"])
    if len(bounds) == 1:
        # A recorded bound `phi ule start + step << 64`.
        bound = int(bounds[0], 16)
        return bound >= 1 << 64 and bound % (1 << 64) < 1 << 32
    if len(bounds) < 2:
        return False
    span = int(bounds[1], 16) - int(bounds[0], 16)
    return span > 0 and span % (1 << 64) == 0


def main() -> None:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("paths", nargs="+", help="Solidity files or directories")
    parser.add_argument("--solar", default=str(ROOT / "target/release/solar"))
    parser.add_argument("--z3")
    parser.add_argument(
        "--levels", default="gas,size", help="comma-separated -O levels"
    )
    parser.add_argument(
        "--timeout", type=int, default=60, help="wall-clock backstop per query, seconds"
    )
    parser.add_argument(
        "--rlimit-batch",
        type=int,
        default=1_000_000,
        help="Z3 resource limit per query in a function's batch",
    )
    parser.add_argument(
        "--rlimit",
        type=int,
        default=40_000_000,
        help="Z3 resource limit per query proved alone",
    )
    parser.add_argument(
        "--memory", type=int, default=1500, help="Z3 memory limit per process, MB"
    )
    parser.add_argument("--jobs", type=int, default=os.cpu_count() or 1)
    parser.add_argument("--out", help="keep queries and write results.jsonl here")
    parser.add_argument(
        "--cache",
        default=str(pathlib.Path.home() / ".cache/solar-prove-analyses"),
        help="verdict cache directory",
    )
    parser.add_argument("--no-cache", action="store_true")
    args = parser.parse_args()
    solver = Solver(args, z3_binary(args.z3))
    out = pathlib.Path(args.out or tempfile.mkdtemp(prefix="prove-analyses-"))
    files = sources(args.paths)
    jobs = []
    for file in files:
        digest = hashlib.sha256(str(file.resolve()).encode()).hexdigest()[:12]
        for level in args.levels.split(","):
            jobs.append((file.resolve(), level, out / f"{digest}-{level}"))
    with concurrent.futures.ThreadPoolExecutor(args.jobs) as pool:
        counts = list(pool.map(lambda job: compile_one(args.solar, *job), jobs))
    origin = {str(directory): str(file) for file, _, directory in jobs}
    groups: dict[tuple[str, str], list[pathlib.Path]] = collections.defaultdict(list)
    for query in sorted(out.rglob("*.smt2")):
        if not query.name.endswith(".abstract.smt2"):
            function = "-".join(query.name.split("-")[:2])
            groups[(str(query.parent), function)].append(query)
    print(
        f"{len(files)} sources, {len(jobs)} compiles, {sum(counts)} queries",
        file=sys.stderr,
    )
    records = []
    with (
        concurrent.futures.ThreadPoolExecutor(args.jobs) as pool,
        open(out / "results.jsonl", "w") as results,
    ):
        futures = [
            pool.submit(solver.solve_group, queries, origin.get(directory, ""))
            for (directory, _), queries in groups.items()
        ]
        for future in concurrent.futures.as_completed(futures):
            for record in future.result():
                records.append(record)
                results.write(json.dumps(record) + "\n")
            results.flush()
    table: collections.Counter[tuple[str, str]] = collections.Counter()
    tiers: collections.Counter[str] = collections.Counter()
    axioms: collections.Counter[str] = collections.Counter()
    for record in records:
        analysis = record.get("claim", "? ").split()[0]
        table[(analysis, record["result"])] += 1
        if record["result"] == "unsat":
            tiers[record.get("tier", "?")] += 1
        for axiom in record.get("axioms", "").split():
            axioms[axiom] += 1
    for (analysis, verdict), count in sorted(table.items()):
        print(f"{analysis:14} {verdict:12} {count}")
    print("proved by tier: " + ", ".join(f"{t} {n}" for t, n in sorted(tiers.items())))
    print(
        "trusted: trip-count bounds (gas), "
        + ", ".join(f"{a} in {n} queries" for a, n in sorted(axioms.items()))
    )
    for record in records:
        if record["result"] == "sat":
            print(
                f"COUNTEREXAMPLE {record['source']}\n  {record['query']}\n"
                f"  {record.get('function')}: {record.get('claim')}\n"
                f"  {record.get('counterexample')}"
            )
    print(f"results: {out / 'results.jsonl'}", file=sys.stderr)


if __name__ == "__main__":
    main()
