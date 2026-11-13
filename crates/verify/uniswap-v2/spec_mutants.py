#!/usr/bin/env python3
"""Check that every spec in a spec-applied tree is killed by some mutant.

    spec_mutants.py --tree OUT_DIR --verifier SOLAR_VERIFY --file F [--file F]...
        [--also F=FILE:C.fn,...] [--per-fn N] [--jobs N] [--timeout MS] [--json REPORT]

For each Solidity file given (relative to the tree), it asks solar-verify for its
mutants (solar-verify mutate --list), applies each one to a copy of the tree,
verifies the functions the mutant can reach (plus any named with --also), and
records which specs then fail with a counterexample. A spec is a @custom:ensures,
@custom:invariant or @custom:reentrancy-guard tag of the tree; it is killed when
at least one mutant makes it fail. Unknown results (timeouts) never count as
kills. The report lists every spec with the mutants that kill it, and the specs
no mutant kills.
"""

import argparse
import concurrent.futures as cf
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

SPEC_TAG = re.compile(r"@custom:(ensures|invariant|reentrancy-guard)\s+(.*)$")
MSG = re.compile(r"^(\d+):(.*)$")


def specs_of(tree, files):
    out = {}
    for f in files:
        for n, line in enumerate((tree / f).read_text().splitlines(), 1):
            m = SPEC_TAG.search(line)
            if m:
                out[(f, n)] = f"{m.group(1)} {m.group(2).strip()}"
    return out


def list_mutants(verifier, tree, f, per_fn):
    res = subprocess.run(
        [verifier, "mutate", "--list", "--json", "--per-fn", str(per_fn), f],
        cwd=tree,
        capture_output=True,
        text=True,
        check=False,
    )
    line = next((x for x in res.stdout.splitlines() if x.startswith("JSON ")), None)
    if line is None:
        sys.exit(f"spec_mutants: no mutants listed for {f}:\n{res.stderr}")
    return json.loads(line[5:])


def apply(src, m):
    lines = src.split("\n")
    lo = sum(len(x) + 1 for x in lines[: m["line"] - 1])
    at = src.find(m["orig"], lo)
    if at < 0:
        return None
    return src[:at] + m["rep"] + src[at + len(m["orig"]) :]


def run_one(args, i, f, m, targets):
    with tempfile.TemporaryDirectory() as d:
        work = pathlib.Path(d) / "t"
        shutil.copytree(args.tree, work, symlinks=True)
        path = work / f
        new = apply(path.read_text(), m)
        if new is None:
            return i, "stillborn", []
        path.write_text(new)
        kills, unknown = [], False
        for target_file, fn in targets:
            cmd = [
                args.verifier,
                "--json",
                "--timeout",
                str(args.timeout),
                "--only",
                fn,
            ]
            res = subprocess.run(
                [*cmd, target_file],
                cwd=work,
                capture_output=True,
                text=True,
                check=False,
            )
            if res.returncode == 2:
                return i, "stillborn", []
            line = next(
                (x for x in res.stdout.splitlines() if x.startswith("JSON ")), None
            )
            for row in json.loads(line[5:]) if line else []:
                for msg in row.get("msgs", []):
                    mm = MSG.match(msg)
                    if not mm:
                        continue
                    if "solver returned" in msg:
                        unknown = True
                        continue
                    kills.append((target_file, int(mm.group(1)), mm.group(2)))
        state = "killed" if kills else ("undecided" if unknown else "survived")
        return i, state, kills


def describe(m):
    return (
        f"{m['fn']} {m['op']} line {m['line']}: '{m['orig'][:60]}' -> '{m['rep'][:60]}'"
    )


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--tree", required=True, type=pathlib.Path)
    ap.add_argument("--verifier", required=True)
    ap.add_argument("--file", action="append", required=True)
    ap.add_argument(
        "--also",
        action="append",
        default=[],
        metavar="F=FILE:C.fn,...",
        help="for mutants of file F, also verify these functions (in FILE)",
    )
    ap.add_argument("--per-fn", type=int, default=12)
    ap.add_argument("--jobs", type=int, default=os.cpu_count() or 1)
    ap.add_argument("--timeout", type=int, default=30000)
    ap.add_argument("--json", type=pathlib.Path)
    args = ap.parse_args()
    args.tree = args.tree.resolve()
    also = {}
    for a in args.also:
        f, _, rest = a.partition("=")
        also[f] = [tuple(x.split(":", 1)) for x in rest.split(",") if x]
    files = sorted(
        {p.relative_to(args.tree).as_posix() for p in args.tree.rglob("*.sol")}
    )
    specs = specs_of(args.tree, files)
    plan = []
    for f in args.file:
        for m in list_mutants(args.verifier, args.tree, f, args.per_fn):
            targets = [(f, fn) for fn in m["affected"]] + also.get(f, [])
            plan.append((f, m, targets))
    print(f"{len(plan)} mutants, {len(specs)} specs", file=sys.stderr)
    results = [None] * len(plan)
    with cf.ThreadPoolExecutor(args.jobs) as ex:
        futs = [
            ex.submit(run_one, args, i, f, m, t) for i, (f, m, t) in enumerate(plan)
        ]
        for done, fut in enumerate(cf.as_completed(futs), 1):
            i, state, kills = fut.result()
            results[i] = (state, kills)
            print(
                f"[{done}/{len(plan)}] {state:9} {describe(plan[i][1])}",
                file=sys.stderr,
            )
    killed_by = {k: [] for k in specs}
    for (_, m, _), (_, kills) in zip(plan, results, strict=True):
        for tf, line, msg in kills:
            # A message names its spec by line, in whichever file declares it (an
            # inherited spec lives in another file), and quotes its text.
            hits = [k for k, t in specs.items() if t.split(" ", 1)[1] in msg]
            exact = [k for k in hits if k == (tf, line)] or [
                k for k in hits if k[1] == line
            ]
            for k in exact or hits:
                if describe(m) not in killed_by[k]:
                    killed_by[k].append(describe(m))
    report = [
        {"file": f, "line": line, "spec": text, "killed_by": killed_by[(f, line)]}
        for (f, line), text in sorted(specs.items())
    ]
    counts = {}
    for state, _ in results:
        counts[state] = counts.get(state, 0) + 1
    if args.json:
        args.json.write_text(
            json.dumps({"mutants": counts, "specs": report}, indent=1) + "\n"
        )
    alive = [r for r in report if not r["killed_by"]]
    for r in report:
        mark = "killed" if r["killed_by"] else "NOT KILLED"
        first = r["killed_by"][0] if r["killed_by"] else ""
        print(f"{mark:10} {r['file']}:{r['line']} {r['spec'][:110]}  {first[:120]}")
    print(f"mutants: {counts}; specs killed: {len(report) - len(alive)}/{len(report)}")
    sys.exit(1 if alive else 0)


if __name__ == "__main__":
    main()
