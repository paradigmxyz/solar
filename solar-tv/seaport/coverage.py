#!/usr/bin/env python3
"""Measures how far solar-tv and solar-verify get on Seaport 1.6 compiled by solar.

    solar-tv/seaport/coverage.py [--seaport DIR] [--solar BIN] [--solar-verify BIN]
                                 [--out solar-tv/seaport/COVERAGE.md] [--json FILE]

Seaport is ProjectOpenSea/seaport at SEAPORT_COMMIT with its submodules (cloned into --seaport
when the directory is missing). Nothing in it is edited: crates/verify/seaport/apply_specs.py
applies the specs of crates/verify/seaport/seaport.spec to a copy, and this script checks that
solar compiles every contract of the copy to the same creation and runtime bytecode.

solar-tv: each compilation unit is compiled with the certify.sh defaults (-Ogas --evm-version
osaka) twice, with and without -Zemit-lean-stages, and the runtime bytecode must be the same.
Per contract with runtime code the table lists the exported stages, the exporter's refusal if
any, the immutables, and the loops and recursion of the first lowered MIR: a stage proof's fuel
is a block count, which bounds only a loop-free run, so a contract with loops is blocked on
loop relations from the compiler until the exporter writes them (the "compiler loop relations"
column counts the loops that have one).

solar-verify: the verifier runs on that copy of contracts/Seaport.sol, and each
function's row is its verdict, its obligations and its first diagnostic, grouped by reason. The
specs hold only function and contract specs, so a loop in a verified function was cut at an
invariant the verifier inferred, which the "loops inferred automatically" column records.
"""

import argparse
import collections
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent.parent
SPECS = ROOT / "crates" / "verify" / "seaport"
SEAPORT_URL = "https://github.com/ProjectOpenSea/seaport.git"
SEAPORT_COMMIT = "1170198"
REMAPPINGS = [
    "@rari-capital/solmate/=lib/solmate/",
    "forge-std/=lib/forge-std/src/",
    "murky/=lib/murky/src/",
    "@openzeppelin/=lib/openzeppelin-contracts/",
    "solarray/=lib/solarray/src/",
    "solady/=lib/solady/",
    "seaport-sol/=lib/seaport-sol/",
    "seaport-types/=lib/seaport-types/",
    "seaport-core/=lib/seaport-core/",
    "seaport/=contracts/",
]
OPTIONS = ["-Ogas", "--evm-version", "osaka"]
# The compilation units of the deployed contracts: Seaport, and the conduit controller, whose
# runtime code creates conduits.
UNITS = ["contracts/Seaport.sol", "lib/seaport-core/src/conduit/ConduitController.sol"]
VERIFY_UNIT = "contracts/Seaport.sol"
# The contracts Seaport deploys; the other contracts with runtime code are abstract bases whose
# runtime is only Seaport's receive guard.
DEPLOYED = ["Seaport", "ConduitController", "Conduit"]


def run(cmd, cwd=None, check=True):
    p = subprocess.run(
        [str(c) for c in cmd], cwd=cwd, capture_output=True, text=True, check=False
    )
    if check and p.returncode != 0:
        sys.exit(f"error: {' '.join(map(str, cmd))} failed:\n{p.stderr[-4000:]}")
    return p


def checkout(dest):
    if not (dest / ".git").exists():
        run(["git", "clone", "-q", SEAPORT_URL, dest])
    run(["git", "checkout", "-q", SEAPORT_COMMIT], cwd=dest)
    run(["git", "submodule", "update", "--init", "--recursive", "-q"], cwd=dest)
    head = run(["git", "rev-parse", "HEAD"], cwd=dest).stdout.strip()
    if not head.startswith(SEAPORT_COMMIT):
        sys.exit(f"error: {dest} is at {head}, not {SEAPORT_COMMIT}")
    if run(["git", "status", "--porcelain"], cwd=dest).stdout.strip():
        sys.exit(f"error: {dest} has local changes; the target is unmodified Seaport")
    return head


def runtime_bytecodes(solar, seaport, unit, extra=(), emit="bin-runtime"):
    """Every contract's runtime bytecode (or the outputs emit names), solar's exit status and
    its stderr."""
    with tempfile.TemporaryDirectory() as out:
        p = run(
            [
                solar,
                *REMAPPINGS,
                *OPTIONS,
                *extra,
                "--emit",
                emit,
                "--out-dir",
                out,
                unit,
            ],
            cwd=seaport,
            check=False,
        )
        codes = {}
        combined = pathlib.Path(out) / "combined.json"
        if combined.exists():
            for name, c in (
                json.loads(combined.read_text()).get("contracts", {}).items()
            ):
                codes[name.rsplit(":", 1)[-1]] = (
                    c.get("bin-runtime", "")
                    if emit == "bin-runtime"
                    else (c.get("bin"), c.get("bin-runtime"))
                )
    return codes, p.returncode, p.stderr


# The first lowered MIR, read from its Lean term: a "-- @name" comment per function and a
# "-- bbN" comment per block.
FUNCTION = re.compile(r"^    -- @(.*)$", re.MULTILINE)
BLOCK = re.compile(r"^      -- bb(\d+)$", re.MULTILINE)


def mir_functions(text):
    """Per function, in index order: its name, each block's successors and its callees."""
    starts = [(m.start(), m.group(1)) for m in FUNCTION.finditer(text)]
    functions = []
    for i, (start, name) in enumerate(starts):
        end = starts[i + 1][0] if i + 1 < len(starts) else len(text)
        body = text[start:end]
        marks = list(BLOCK.finditer(body))
        succs = {}
        for j, m in enumerate(marks):
            block = body[
                m.end() : marks[j + 1].start() if j + 1 < len(marks) else len(body)
            ]
            term = block[block.rfind("],") :]
            out = []
            if b := re.search(r"\.branch \(.*?\) (\d+) (\d+)", term):
                out = [int(b.group(1)), int(b.group(2))]
            elif jump := re.search(r"\.jump (\d+)", term):
                out = [int(jump.group(1))]
            elif s := re.search(r"\.switch \(.*?\) (\d+) \[(.*)\]", term):
                out = [int(s.group(1))] + [
                    int(t) for t in re.findall(r", (\d+)\)", s.group(2))
                ]
            succs[int(m.group(1))] = out
        callees = {int(c) for c in re.findall(r"\.(?:call \S+|tailCall) (\d+) ", body)}
        functions.append({"name": name, "succs": succs, "callees": callees})
    return functions


def back_edges(succs):
    """The edges a depth-first search from bb0 finds into a block on its stack: one per loop."""
    count, state, stack = 0, {0: 1}, [(0, iter(succs.get(0, [])))]
    while stack:
        node, it = stack[-1]
        nxt = next(it, None)
        if nxt is None:
            state[node] = 2
            stack.pop()
        elif state.get(nxt) == 1:
            count += 1
        elif nxt not in state:
            state[nxt] = 1
            stack.append((nxt, iter(succs.get(nxt, []))))
    return count


def reachable(functions, roots):
    seen, todo = set(), list(roots)
    while todo:
        f = todo.pop()
        if f in seen or f >= len(functions):
            continue
        seen.add(f)
        todo.extend(functions[f]["callees"])
    return seen


def tv_contract(gen, name):
    manifest = json.loads((gen / "manifest.json").read_text())
    stages = [s for s in manifest["stages"] if s["kind"] != "bytecode"]
    stage0 = (gen / "Stage0.lean").read_text()
    entry = int(re.search(r"entry := (\d+)", stage0).group(1))
    functions = mir_functions(stage0)
    reach = reachable(functions, [entry])
    rec = sorted(
        functions[f]["name"]
        for f in reach
        if f in reachable(functions, functions[f]["callees"])
    )
    looping = {functions[f]["name"]: back_edges(functions[f]["succs"]) for f in reach}
    looping = {f: n for f, n in looping.items() if n}
    immutables = manifest.get("immutables", [])
    return {
        "contract": name,
        "runtime_bytes": len(manifest["bytecode"]) // 2,
        "mir_stages": sum(s["kind"] == "mir" for s in stages),
        "evm_stages": sum(s["kind"] in ("sched", "evm") for s in stages),
        "immutables": len({i["id"] for i in immutables}),
        "placeholders": dict(collections.Counter(i["bytes"] for i in immutables)),
        "functions": len(reach),
        "loops": sum(looping.values()),
        "looping_functions": sorted(looping),
        "recursive": rec,
        # The exporter writes no loop relations yet.
        "loop_relations": int(manifest.get("loop_relations", 0)),
    }


def solar_tv(solar, seaport):
    rows, units = [], []
    for unit in UNITS:
        plain, code, err = runtime_bytecodes(solar, seaport, unit)
        if code != 0:
            sys.exit(f"error: solar failed on {unit}:\n{err[-4000:]}")
        gen = pathlib.Path(tempfile.mkdtemp(prefix="seaport-stages-"))
        try:
            exported, code, err = runtime_bytecodes(
                solar, seaport, unit, [f"-Zemit-lean-stages={gen}"]
            )
            marker = "-Zemit-lean-stages: "
            refusals = [l.split(marker, 1)[1] for l in err.splitlines() if marker in l]
            changed = sorted(
                c for c in plain if c in exported and exported[c] != plain[c]
            )
            units.append(
                {
                    "unit": unit,
                    "exit": code,
                    "refusals": refusals,
                    "bytecode_changed": changed,
                }
            )
            for name in sorted(plain):
                if name not in DEPLOYED:
                    continue
                d = gen / name
                if not (d / "manifest.json").exists():
                    why = next((r for r in refusals if f"`{name}`" in r), "no manifest")
                    rows.append(
                        {
                            "contract": name,
                            "unit": unit,
                            "runtime_bytes": len(plain[name]) // 2,
                            "refused": why,
                        }
                    )
                    continue
                row = tv_contract(d, name)
                row.update(unit=unit, refused=None)
                if (
                    json.loads((d / "manifest.json").read_text())["bytecode"]
                    != plain[name]
                ):
                    row["refused"] = (
                        "the manifest's bytecode is not the compiled runtime code"
                    )
                rows.append(row)
        finally:
            shutil.rmtree(gen)
    return rows, units


# Refusal reasons, by a phrase of the diagnostic, and the workstream that owns each.
REASONS = [
    ("`for` loops", "Yul for loop", "solar-verify loops and symbolic memory"),
    (
        "symbolic",
        "memory at a symbolic offset",
        "solar-verify loops and symbolic memory",
    ),
    (
        "outside the scratch space",
        "memory outside the scratch space",
        "solar-verify loops and symbolic memory",
    ),
    ("builtin `return`", "Yul return", "solar-verify loops and symbolic memory"),
    (".slot", ".slot/.offset/.length", "solar-verify functions"),
    ("malloc", "free function", "solar-verify functions"),
    (
        "unsupported call",
        "call through a function-pointer cast",
        "solar-verify functions",
    ),
    ("assembly assignment", "assembly assignment to an enum", "solar-verify functions"),
]


def verify_reason(msg):
    m = msg.split(":", 1)[-1].strip()
    for key, label, owner in REASONS:
        if key in m:
            return label, owner
    return m[:80], "solar-verify functions"


def solidity_loops(source):
    """Per function name, the loops written in its own body (Solidity and Yul)."""
    source = re.sub(r"//[^\n]*|/\*.*?\*/", "", source, flags=re.DOTALL)
    out = {}
    for m in re.finditer(r"\bfunction\s+(\w+)\s*\(", source):
        i, semi = source.find("{", m.end()), source.find(";", m.end())
        if i < 0 or 0 <= semi < i:
            continue
        depth, j = 0, i
        while j < len(source):
            depth += {"{": 1, "}": -1}.get(source[j], 0)
            if depth == 0:
                break
            j += 1
        n = len(re.findall(r"\bfor\s*[({]|\bwhile\s*\(|\bdo\s*\{", source[i:j]))
        out[m.group(1)] = out.get(m.group(1), 0) + n
    return out


def apply_specs(solar, seaport, out, spec):
    """The spec-applied copy of Seaport, checked to compile to the same bytecode."""
    cmd = [sys.executable, SPECS / "apply_specs.py", "--seaport", seaport, "--out", out]
    if spec:
        cmd += ["--spec", spec]
    run(cmd)
    for unit in UNITS:
        a, ca, ea = runtime_bytecodes(solar, seaport, unit, emit="bin,bin-runtime")
        b, cb, eb = runtime_bytecodes(solar, out, unit, emit="bin,bin-runtime")
        if ca or cb:
            sys.exit(f"error: solar failed on {unit}:\n{(ea or eb)[-3000:]}")
        changed = sorted(c for c in a.keys() | b.keys() if a.get(c) != b.get(c))
        if changed:
            sys.exit(
                f"error: the specs change the bytecode of {', '.join(changed)} in {unit}"
            )


def solar_verify(verifier, z3, specced, timeout, jobs):
    env = dict(os.environ)
    if z3:
        env["SOLAR_VERIFY_Z3"] = z3
    p = subprocess.run(
        [
            verifier,
            *REMAPPINGS,
            "--json",
            "--jobs",
            str(jobs),
            "--timeout",
            str(timeout),
            VERIFY_UNIT,
        ],
        cwd=specced,
        capture_output=True,
        text=True,
        env=env,
        check=False,
    )
    if "JSON " not in p.stdout:
        return [], p.stderr
    rows = json.loads(p.stdout[p.stdout.index("JSON ") + 5 :].splitlines()[0])
    sources = "".join(
        f.read_text(errors="replace")
        for d in ("contracts", "lib/seaport-core/src", "lib/seaport-types/src")
        for f in sorted((specced / d).rglob("*.sol"))
        if "/test/" not in str(f)
    )
    loops = solidity_loops(sources)
    for r in rows:
        r["loops"] = loops.get(r["fn"].split(".", 1)[1], 0)
        if r["status"] == "unsupported" and r["msgs"]:
            r["reason"], r["owner"] = verify_reason(r["msgs"][0])
        elif r["status"] == "verified" and r["checks"] == 0:
            r["reason"], r["owner"] = "vacuous: no spec reaches it", "Seaport target"
        elif r["status"] == "verified":
            r["reason"], r["owner"] = "", ""
        else:
            r["reason"], r["owner"] = (
                (r["msgs"][0] if r["msgs"] else "")[:100],
                "Seaport target",
            )
    return rows, p.stderr


def markdown(meta, tv_rows, tv_units, verify_rows, verify_err):
    L = ["# Seaport coverage", ""]
    L.append(
        "Generated by [coverage.py](coverage.py); do not edit. Seaport 1.6 "
        f"(ProjectOpenSea/seaport at {meta['seaport']}, with submodules, unmodified) compiled by "
        f"solar {meta['solar']} with `{' '.join(OPTIONS)}`; solar-verify ran on it with the specs "
        "of [seaport.spec](../../crates/verify/seaport/seaport.spec) applied by "
        "[apply_specs.py](../../crates/verify/seaport/apply_specs.py), which leave every contract's "
        "bytecode unchanged."
    )
    L += ["", "## solar-tv: contract × stage", ""]
    for u in tv_units:
        changed = ", ".join(u["bytecode_changed"]) or "none"
        L.append(
            f"- `{u['unit']}`: exporter exit status {u['exit']}, {len(u['refusals'])} refusals; "
            f"contracts whose runtime bytecode `-Zemit-lean-stages` changes: {changed}."
        )
    L += [
        "",
        (
            "| Contract | Runtime bytes | Export | MIR stages | EVM IR stages | Immutables (placeholders) "
            "| MIR loops | Compiler loop relations | Recursive functions | Stage proofs |"
        ),
        "| --- | ---: | --- | ---: | ---: | --- | ---: | ---: | --- | --- |",
    ]
    for r in tv_rows:
        if "mir_stages" not in r:
            L.append(
                f"| {r['contract']} | {r['runtime_bytes']} | refused: {r['refused']} | | | | | | | blocked: export |"
            )
            continue
        widths = (
            ", ".join(
                f"{n} × {w} B"
                for w, n in sorted(r["placeholders"].items(), key=lambda x: int(x[0]))
            )
            or "none"
        )
        if r["loops"] > r["loop_relations"]:
            proofs = (
                f"blocked: {r['loops'] - r['loop_relations']} loops without relations"
            )
        elif r["recursive"]:
            proofs = "blocked: recursion"
        else:
            proofs = "certify.sh"
        L.append(
            f"| {r['contract']} | {r['runtime_bytes']} | {r['refused'] or 'ok'} | {r['mir_stages']} "
            f"| {r['evm_stages']} | {r['immutables']} ({widths}) | {r['loops']} | {r['loop_relations']} "
            f"| {', '.join(r['recursive']) or 'none'} | {proofs} |"
        )
    L += [
        "",
        (
            "The rows are the contracts Seaport deploys; every other contract of the units, an "
            "abstract base or a library, exports too. A loop is a back edge, from each function's "
            "entry, of the first lowered MIR's functions that the runtime entry reaches."
        ),
        "",
    ]
    for r in tv_rows:
        if r.get("looping_functions"):
            L.append(
                f"- {r['contract']} loops in "
                + ", ".join(f"`{f}`" for f in r["looping_functions"])
                + "."
            )
    L += ["", "## solar-verify: function × status", ""]
    if not verify_rows:
        L += [
            "solar-verify verified nothing; its diagnostics end:",
            "",
            "```text",
            verify_err[-3000:].rstrip(),
            "```",
            "",
        ]
        return "\n".join(L) + "\n"
    c = collections.Counter(r["status"] for r in verify_rows)
    proved = sum(r["status"] == "verified" and r["checks"] > 0 for r in verify_rows)
    L.append(
        f"{len(verify_rows)} functions: {proved} verified against specs, {c['verified'] - proved} "
        f"verified vacuously (no spec reaches them, no obligation), {c['unsupported']} refused, "
        f"{c['failed']} failed, {c['unknown']} unknown."
    )
    L += [
        "",
        "| Function | Status | Obligations | Loops in body | Loops inferred automatically | Reason | Owner |",
        "| --- | --- | ---: | ---: | --- | --- | --- |",
    ]
    order = {"verified": 0, "failed": 1, "unknown": 2, "unsupported": 3}
    for r in sorted(
        verify_rows,
        key=lambda r: (order.get(r["status"], 4), r["checks"] == 0, r["fn"]),
    ):
        inferred = (
            "-"
            if not r["loops"]
            else ("yes" if r["status"] == "verified" and r["checks"] else "open")
        )
        reason = r["reason"].replace("|", "\\|")
        L.append(
            f"| `{r['fn']}` | {r['status']} | {r['checks']} | {r['loops']} | {inferred} | {reason} | {r['owner']} |"
        )
    L += ["", "Refusals by reason:", ""]
    reasons = collections.Counter(
        r["reason"] for r in verify_rows if r["status"] == "unsupported"
    )
    L += [f"- {n}: {reason}" for reason, n in reasons.most_common()]
    return "\n".join(L) + "\n"


def main():
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    target = ROOT / "target" / "release"
    ap.add_argument(
        "--seaport",
        type=pathlib.Path,
        default=pathlib.Path(
            os.environ.get("SEAPORT_DIR", ROOT / "target" / "seaport")
        ),
    )
    ap.add_argument("--solar", default=os.environ.get("SOLAR", target / "solar"))
    ap.add_argument(
        "--solar-verify",
        default=os.environ.get("SOLAR_VERIFY", target / "solar-verify"),
    )
    ap.add_argument("--z3", default=os.environ.get("SOLAR_VERIFY_Z3"))
    ap.add_argument(
        "--spec", type=pathlib.Path, help="default: crates/verify/seaport/seaport.spec"
    )
    ap.add_argument("--timeout", type=int, default=20000)
    ap.add_argument("--jobs", type=int, default=3)
    ap.add_argument("--out", type=pathlib.Path, default=HERE / "COVERAGE.md")
    ap.add_argument("--json", type=pathlib.Path)
    ap.add_argument("--skip-verify", action="store_true")
    args = ap.parse_args()
    for tool in ("solar", "solar_verify"):
        path = str(getattr(args, tool))
        setattr(
            args,
            tool,
            str(pathlib.Path(path).resolve()) if "/" in path else shutil.which(path),
        )

    head = checkout(args.seaport.resolve())
    version = run([args.solar, "--version"]).stdout.strip().splitlines()
    tv_rows, tv_units = solar_tv(args.solar, args.seaport.resolve())
    verify_rows, verify_err = [], ""
    if not args.skip_verify:
        with tempfile.TemporaryDirectory() as tmp:
            specced = pathlib.Path(tmp) / "seaport"
            apply_specs(args.solar, args.seaport.resolve(), specced, args.spec)
            verify_rows, verify_err = solar_verify(
                args.solar_verify, args.z3, specced, args.timeout, args.jobs
            )
    meta = {
        "seaport": head[:7],
        "solar": version[0].split(" ", 1)[-1] if version else "?",
    }
    args.out.write_text(markdown(meta, tv_rows, tv_units, verify_rows, verify_err))
    if args.json:
        args.json.write_text(
            json.dumps(
                {
                    "meta": meta,
                    "tv": tv_rows,
                    "tv_units": tv_units,
                    "verify": verify_rows,
                },
                indent=1,
            )
            + "\n"
        )
    print(f"wrote {args.out}")


if __name__ == "__main__":
    main()
