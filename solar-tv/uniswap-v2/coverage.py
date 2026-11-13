#!/usr/bin/env python3
"""Measures how far solar-tv and solar-verify get on the Uniswap V2 port compiled by solar.

    solar-tv/uniswap-v2/coverage.py [--solar BIN] [--solar-verify BIN] [--z3 Z3]
                                    [--out solar-tv/uniswap-v2/COVERAGE.md] [--json FILE]

The target is the ^0.8 port of Uniswap v2-core this directory's README describes, which
port.py writes to --src (default: target/uniswap-v2/src).

solar-tv: each unit (the factory, which embeds the pair's creation code, and the pair) is
compiled with the certify.sh defaults (-Ogas --evm-version osaka), with and without
-Zemit-lean-stages, and every contract's runtime bytecode must be the same both times. Per
contract the table lists the exported stages, the exporter's refusal if any, and the
immutables. Per function of the first lowered MIR that the runtime entry reaches, it lists the
blocks, loops (back edges from the function's entry), internal calls and whether it is
recursive: a stage proof's fuel is a block count, which bounds only a loop-free run, so a
function with loops needs loop relations from the compiler before its contract's stages can be
proved.

solar-verify: each unit is verified per function. When crates/verify/uniswap-v2/apply_specs.py
exists, it first applies that directory's specs to a copy of the port (apply_specs.py --src SRC
--out OUT), and solar must compile the copy to the same creation and runtime bytecode. The
table lists each function's verdict, obligations, loops, and first diagnostic.
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

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent.parent
SRC = ROOT / "target" / "uniswap-v2" / "src"
SPECS = ROOT / "crates" / "verify" / "uniswap-v2"
OPTIONS = ["-Ogas", "--evm-version", "osaka"]
UNITS = ["UniswapV2Factory.sol", "UniswapV2Pair.sol"]
# solar-verify verifies a file's own contracts with what they inherit: the token on its own too.
VERIFY_UNITS = [*UNITS, "UniswapV2ERC20.sol"]
# The contracts that deploy: the factory and the pairs it creates.
DEPLOYED = ["UniswapV2Factory", "UniswapV2Pair"]


def run(cmd, cwd=None, check=True, env=None):
    p = subprocess.run(
        [str(c) for c in cmd],
        cwd=cwd,
        capture_output=True,
        text=True,
        check=False,
        env=env,
    )
    if check and p.returncode != 0:
        sys.exit(f"error: {' '.join(map(str, cmd))} failed:\n{p.stderr[-4000:]}")
    return p


def compile_unit(solar, src, unit, extra=(), emit="bin-runtime"):
    """Every contract's outputs (the runtime bytecode, or (creation, runtime) with emit
    "bin,bin-runtime"), solar's exit status and its stderr."""
    with tempfile.TemporaryDirectory() as out:
        p = run(
            [solar, *OPTIONS, *extra, "--emit", emit, "--out-dir", out, unit],
            cwd=src,
            check=False,
        )
        codes = {}
        combined = pathlib.Path(out) / "combined.json"
        if combined.exists():
            for name, c in (
                json.loads(combined.read_text()).get("contracts", {}).items()
            ):
                key = name.rsplit(":", 1)[-1]
                codes[key] = (
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
        callees = {
            int(c)
            for c in re.findall(
                r"\.(?:call (?:none|\(some \d+\))|tailCall) (\d+) ", body
            )
        }
        name = name.split(":")[0].strip()
        functions.append(
            {"name": name, "blocks": len(marks), "succs": succs, "callees": callees}
        )
    return functions


def back_edges(succs):
    """The edges a depth-first search from bb0 finds into a block on its stack: one per loop."""
    if 0 not in succs:
        return 0
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
    rows = []
    for f in sorted(reach):
        fn = functions[f]
        rows.append(
            {
                "function": fn["name"],
                "blocks": fn["blocks"],
                "loops": back_edges(fn["succs"]),
                "calls": len(fn["callees"]),
                "recursive": f in reachable(functions, fn["callees"]),
            }
        )
    immutables = manifest.get("immutables", [])
    return {
        "contract": name,
        "runtime_bytes": len(manifest["bytecode"]) // 2,
        "mir_stages": sum(s["kind"] == "mir" for s in stages),
        "evm_stages": sum(s["kind"] in ("sched", "evm") for s in stages),
        "immutables": len({i["id"] for i in immutables}),
        "placeholders": dict(collections.Counter(str(i["bytes"]) for i in immutables)),
        "functions": rows,
        "loops": sum(r["loops"] for r in rows),
        # The exporter writes no loop relations yet.
        "loop_relations": int(manifest.get("loop_relations", 0)),
    }


def solar_tv(solar, src):
    contracts, units = {}, []
    for unit in UNITS:
        plain, code, err = compile_unit(solar, src, unit)
        if code != 0:
            sys.exit(f"error: solar failed on {unit}:\n{err[-4000:]}")
        gen = pathlib.Path(tempfile.mkdtemp(prefix="uniswap-v2-stages-"))
        try:
            exported, code, err = compile_unit(
                solar, src, unit, [f"-Zemit-lean-stages={gen}"]
            )
            marker = "-Zemit-lean-stages: "
            refusals = [
                line.split(marker, 1)[1] for line in err.splitlines() if marker in line
            ]
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
            for name in DEPLOYED:
                if name not in plain or name in contracts:
                    continue
                d = gen / name
                if not (d / "manifest.json").exists():
                    why = next(
                        (r for r in refusals if f"\x60{name}\x60" in r),
                        refusals[0] if refusals else "no manifest",
                    )
                    contracts[name] = {
                        "contract": name,
                        "runtime_bytes": len(plain[name]) // 2,
                        "refused": why,
                    }
                    continue
                row = tv_contract(d, name)
                row["refused"] = None
                if (
                    json.loads((d / "manifest.json").read_text())["bytecode"]
                    != plain[name]
                ):
                    row["refused"] = (
                        "the manifest's bytecode is not the compiled runtime code"
                    )
                contracts[name] = row
        finally:
            shutil.rmtree(gen)
    return [contracts[n] for n in DEPLOYED if n in contracts], units


def solidity_loops(source):
    """Per function name, the loops written in its own body."""
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


def specced_copy(solar, src, out):
    """The port with the specs applied when crates/verify/uniswap-v2 has them, checked to
    compile to the same bytecode; else the port itself."""
    applier = SPECS / "apply_specs.py"
    if not applier.exists():
        return src, False
    run([sys.executable, applier, "--src", src, "--out", out])
    for unit in UNITS:
        a, ca, ea = compile_unit(solar, src, unit, emit="bin,bin-runtime")
        b, cb, eb = compile_unit(solar, out, unit, emit="bin,bin-runtime")
        if ca or cb:
            sys.exit(f"error: solar failed on {unit}:\n{(ea or eb)[-3000:]}")
        changed = sorted(c for c in a.keys() | b.keys() if a.get(c) != b.get(c))
        if changed:
            sys.exit(
                f"error: the specs change the bytecode of {', '.join(changed)} in {unit}"
            )
    return out, True


def solar_verify(verifier, z3, src, timeout, jobs):
    env = dict(os.environ)
    if z3:
        env["SOLAR_VERIFY_Z3"] = z3
    loops = solidity_loops("".join(f.read_text() for f in sorted(src.rglob("*.sol"))))
    rows, seen, errors = [], set(), ""
    for unit in VERIFY_UNITS:
        p = run(
            [verifier, "--json", "--jobs", jobs, "--timeout", timeout, unit],
            cwd=src,
            check=False,
            env=env,
        )
        if "JSON " not in p.stdout:
            errors += p.stderr[-2000:]
            continue
        for r in json.loads(p.stdout[p.stdout.index("JSON ") + 5 :].splitlines()[0]):
            if r["fn"] in seen:
                continue
            seen.add(r["fn"])
            r["loops"] = loops.get(r["fn"].split(".", 1)[1], 0)
            if r["status"] == "verified" and r["checks"] == 0:
                r["reason"] = "vacuous: no spec reaches it"
            elif r["status"] == "verified":
                r["reason"] = ""
            else:
                r["reason"] = (r["msgs"][0] if r["msgs"] else "")[:120]
            rows.append(r)
    return rows, errors


def markdown(meta, tv_rows, tv_units, verify_rows, verify_err):
    L = ["# Uniswap V2 coverage", ""]
    L.append(
        "Generated by [coverage.py](coverage.py); do not edit. The ^0.8 port of Uniswap v2-core that "
        f"[port.py](port.py) writes, compiled by solar {meta['solar']} with \x60{' '.join(OPTIONS)}\x60; solar-verify ran "
        + (
            "with the specs of crates/verify/uniswap-v2 applied, which leave every contract's bytecode unchanged."
            if meta["specs"]
            else "on it without specs."
        )
    )
    L += ["", "## solar-tv: contract × stage", ""]
    for u in tv_units:
        changed = ", ".join(u["bytecode_changed"]) or "none"
        L.append(
            f"- \x60{u['unit']}\x60: exporter exit status {u['exit']}, {len(u['refusals'])} refusals; "
            f"contracts whose runtime bytecode \x60-Zemit-lean-stages\x60 changes: {changed}."
        )
    L += [
        "",
        "| Contract | Runtime bytes | Export | MIR stages | EVM IR stages | Immutables (placeholders) | MIR loops | Compiler loop relations | Stage proofs |",
        "| --- | ---: | --- | ---: | ---: | --- | ---: | ---: | --- |",
    ]
    for r in tv_rows:
        if "mir_stages" not in r:
            L.append(
                f"| {r['contract']} | {r['runtime_bytes']} | refused: {r['refused']} | | | | | | blocked: export |"
            )
            continue
        widths = (
            ", ".join(
                f"{n} × {w} B"
                for w, n in sorted(r["placeholders"].items(), key=lambda x: int(x[0]))
            )
            or "none"
        )
        recursive = any(f["recursive"] for f in r["functions"])
        if r["loops"] > r["loop_relations"]:
            proofs = (
                f"blocked: {r['loops'] - r['loop_relations']} loops without relations"
            )
        elif recursive:
            proofs = "blocked: recursion"
        else:
            proofs = "certify.sh"
        L.append(
            f"| {r['contract']} | {r['runtime_bytes']} | {r['refused'] or 'ok'} | {r['mir_stages']} | {r['evm_stages']} "
            f"| {r['immutables']} ({widths}) | {r['loops']} | {r['loop_relations']} | {proofs} |"
        )
    L += ["", "## solar-tv: function × first lowered MIR", ""]
    L += [
        "| Contract | Function | Blocks | Loops | Internal callees | Recursive |",
        "| --- | --- | ---: | ---: | ---: | --- |",
    ]
    for r in tv_rows:
        for f in r.get("functions", []):
            L.append(
                f"| {r['contract']} | \x60{f['function']}\x60 | {f['blocks']} | {f['loops']} | {f['calls']} | {'yes' if f['recursive'] else 'no'} |"
            )
    L += [
        "",
        "The functions are those of the first lowered MIR that the runtime entry reaches, after inlining.",
        "",
    ]
    L += ["## solar-verify: function × status", ""]
    if not verify_rows:
        L += [
            "solar-verify verified nothing; its diagnostics end:",
            "",
            "\x60\x60\x60text",
            verify_err.rstrip(),
            "\x60\x60\x60",
            "",
        ]
        return "\n".join(L) + "\n"
    c = collections.Counter(r["status"] for r in verify_rows)
    proved = sum(r["status"] == "verified" and r["checks"] > 0 for r in verify_rows)
    L.append(
        f"{len(verify_rows)} functions: {proved} verified against specs, {c['verified'] - proved} verified "
        f"vacuously (no spec reaches them), {c['unsupported']} refused, {c['failed']} failed, {c['unknown']} unknown."
    )
    L += [
        "",
        "| Function | Status | Obligations | Loops in body | Loops inferred automatically | Reason |",
        "| --- | --- | ---: | ---: | --- | --- |",
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
            f"| \x60{r['fn']}\x60 | {r['status']} | {r['checks']} | {r['loops']} | {inferred} | {reason} |"
        )
    return "\n".join(L) + "\n"


def main():
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    target = ROOT / "target" / "release"
    ap.add_argument("--solar", default=os.environ.get("SOLAR", target / "solar"))
    ap.add_argument(
        "--solar-verify",
        default=os.environ.get("SOLAR_VERIFY", target / "solar-verify"),
    )
    ap.add_argument("--z3", default=os.environ.get("SOLAR_VERIFY_Z3"))
    ap.add_argument(
        "--src", type=pathlib.Path, default=SRC, help="where port.py writes the port"
    )
    ap.add_argument("--v2-core", type=pathlib.Path, help="port.py's --v2-core")
    ap.add_argument("--timeout", default="20000")
    ap.add_argument("--jobs", default="3")
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

    src = args.src.resolve()
    cmd = [sys.executable, HERE / "port.py", "--out", src]
    if args.v2_core:
        cmd += ["--v2-core", args.v2_core]
    run(cmd)
    version = run([args.solar, "--version"]).stdout.strip().splitlines()
    tv_rows, tv_units = solar_tv(args.solar, src)
    verify_rows, verify_err, specs = [], "", False
    if not args.skip_verify:
        with tempfile.TemporaryDirectory() as tmp:
            specced, specs = specced_copy(args.solar, src, pathlib.Path(tmp) / "src")
            verify_rows, verify_err = solar_verify(
                args.solar_verify, args.z3, specced, args.timeout, args.jobs
            )
    meta = {"solar": version[0].split(" ", 1)[-1] if version else "?", "specs": specs}
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
