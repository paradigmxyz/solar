#!/usr/bin/env python3
"""Counts what -Zemit-lean-stages exports and refuses across the runtime tests.

Compiles every Solidity UI test with a run-call directive, once per revision, and every source
of the tests/foundry projects, with -Zcheck-analyses=false -Ogas --evm-version osaka and the
test's own compile flags (which replace those defaults when they set the same option). Each
compilation runs twice, without and with -Zemit-lean-stages, and must give the same bytecode
for every contract: any difference, or a manifest whose bytecode is not the runtime code
compiled without exporting, is reported and makes the script exit with status 1, as does an
export that times out. Exports that
fail are refused, and their messages, without the contract, function, block and numbers they
name, are counted by reason. Compilations run on one thread unless the test sets its own, so a
compilation with several refused contracts reports the same refusals on every run.

    solar-tv/tools/export_sweep.py [--solar target/release/solar] [--jobs N] [--json FILE]

Run it from anywhere after cargo build --release; MIR and EVM IR inputs are left out, since
the exporter runs only on the Solidity pipeline.
"""

import argparse
import collections
import concurrent.futures
import json
import os
import pathlib
import re
import subprocess
import sys
import tempfile
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[2]
EVM_VERSION = "osaka"
EMIT = ["bin", "bin-runtime"]
PREFIX = "error: -Zemit-lean-stages: "
TIMEOUT = 300
KINDS = (
    "exported",
    "no code",
    "refused",
    "does not compile",
    "export timed out",
    "bytecode changed",
)

# The codegen matrix's standard revisions and their flags (tools/tester/src/codegen_matrix.rs).
MATRIX_EMIT = "--emit=abi,bin,bin-runtime,srcmap,srcmap-runtime"
MATRIX = {
    "mir": ["-O", "none", MATRIX_EMIT, "-Zdump=mir"],
    "gas": ["-O", "gas", MATRIX_EMIT],
    "size": ["-O", "size", MATRIX_EMIT],
}


def directives(text):
    """The (revisions, name, arguments) of every //@ directive of a test."""
    out = []
    for line in text.splitlines():
        m = re.match(r"\s*//@(?:\[([^\]]*)\])?\s*([\w-]+)\s*:?\s*(.*)$", line)
        if m:
            revisions = (
                [r.strip() for r in m.group(1).split(",")] if m.group(1) else None
            )
            out.append((revisions, m.group(2), m.group(3).strip()))
    return out


def ui_cases(path):
    """The compilations of one UI test: one per revision, with its flags."""
    items = directives(path.read_text(errors="replace"))
    revisions = []
    for revs, name, args in items:
        if revs is None and name == "revisions":
            revisions += args.split()
        if revs is None and name == "codegen-matrix":
            extra = args.split()[1:]
            revisions = list(MATRIX) + [r for r in revisions + extra if r not in MATRIX]
    matrix = any(name == "codegen-matrix" and revs is None for revs, name, _ in items)
    cases = []
    for revision in revisions or [None]:
        flags = list(MATRIX.get(revision, [])) if matrix else []
        for revs, name, args in items:
            if name == "compile-flags" and (revs is None or revision in revs):
                flags += args.split()
        cases.append((str(path.relative_to(ROOT)), revision, ROOT, [str(path)], flags))
    return cases


def foundry_cases():
    """One compilation per source of each tests/foundry project, outside its lib/."""
    cases = []
    for toml in sorted((ROOT / "tests/foundry").glob("*/foundry.toml")):
        project = toml.parent
        remappings = project / "remappings.txt"
        flags = remappings.read_text().split() if remappings.exists() else []
        tables = [tomllib.loads(toml.read_text())]
        while tables:
            table = tables.pop()
            flags += [r for r in table.get("remappings", []) if r not in flags]
            tables += [v for v in table.values() if isinstance(v, dict)]
        for source in sorted(project.rglob("*.sol")):
            relative = source.relative_to(project)
            if relative.parts[0] in ("lib", "out", "cache"):
                continue
            name = str(source.relative_to(ROOT))
            cases.append((name, None, project, [str(relative)], list(flags)))
    return cases


def options(flags):
    """The flags of a compilation: the defaults the test does not set, then the test's own,
    with the outputs the sweep reads added and without the test's own stage directory."""
    out, emit, i = [], list(EMIT), 0
    while i < len(flags):
        flag = flags[i]
        if flag == "--emit" and i + 1 < len(flags):
            emit += flags[i + 1].split(",")
            i += 1
        elif flag.startswith("--emit="):
            emit += flag.removeprefix("--emit=").split(",")
        elif not flag.startswith("-Zemit-lean-stages"):
            out.append(flag)
        i += 1

    def sets(name):
        return any(f == name or f.startswith(name + "=") for f in out)

    head = []
    if not any(f.startswith("-Zcheck-analyses") for f in out):
        head.append("-Zcheck-analyses=false")
    if not any(f.startswith("-O") for f in out) and not sets("--optimize"):
        head.append("-Ogas")
    if not sets("--evm-version"):
        head += ["--evm-version", EVM_VERSION]
    # One thread, so that a compilation whose contracts are refused in parallel reports the
    # same refusals on every run.
    if (
        not any(f.startswith("-j") for f in out)
        and not sets("--threads")
        and not sets("--jobs")
    ):
        head.append("-j1")
    return head + out + ["--emit=" + ",".join(dict.fromkeys(emit))]


def compile_once(solar, cwd, inputs, flags, out_dir, stages):
    """Runs one compilation; returns its status, stderr and per-contract bytecode."""
    command = [solar, *flags, *inputs, "--out-dir", str(out_dir)]
    if stages:
        command.append(f"-Zemit-lean-stages={stages}")
    try:
        run = subprocess.run(
            command,
            cwd=cwd,
            capture_output=True,
            text=True,
            timeout=TIMEOUT,
            check=False,
        )
    except subprocess.TimeoutExpired:
        return None, "timeout", {}
    combined = out_dir / "combined.json"
    contracts = {}
    if run.returncode == 0 and combined.exists():
        for name, artifact in (
            json.loads(combined.read_text()).get("contracts", {}).items()
        ):
            contracts[name] = (artifact.get("bin"), artifact.get("bin-runtime"))
    return run.returncode, run.stderr, contracts


def reason(message):
    """A refusal without the contract, pass, file, function and numbers it names."""
    message = re.sub(
        r"^(after (MIR|EVM IR) pass `[^`]*` on|scheduled runtime EVM IR of) `[^`]*`: ",
        "",
        message,
    )
    message = re.sub(r"^in `[^`]*` of `[^`]*`: ", "", message)
    message = re.sub(r"in `@[^`]*`: ", "", message)
    message = re.sub(r"^bb\d+( \(block \d+\))?: ", "", message)
    message = re.sub(r"`@[^`]*`", "`@f`", message)
    message = re.sub(r"\bv\d+\b", "vN", message)
    message = re.sub(r"(two contracts are named) `[^`]*`", r"\1 `C`", message)
    return re.sub(r"\b\d+\b", "N", message)


def sweep(case, solar):
    """Compiles one case without and with exporting and classifies the result."""
    name, revision, cwd, inputs, flags = case
    flags = options(flags)
    label = name + (f" [{revision}]" if revision else "")
    with tempfile.TemporaryDirectory(prefix="solar-export-sweep-") as tmp:
        tmp = pathlib.Path(tmp)
        (tmp / "plain").mkdir()
        (tmp / "export").mkdir()
        status, stderr, plain = compile_once(
            solar, cwd, inputs, flags, tmp / "plain", None
        )
        if status != 0:
            return (
                label,
                "does not compile",
                stderr.strip().splitlines()[-1:] if stderr else [],
            )
        stages = tmp / "stages"
        status, stderr, exported = compile_once(
            solar, cwd, inputs, flags, tmp / "export", stages
        )
        refusals = [
            reason(line[len(PREFIX) :])
            for line in stderr.splitlines()
            if line.startswith(PREFIX)
        ]
        if status is None:
            return label, "export timed out", []
        if status != 0:
            return label, "refused", refusals or [stderr.strip().splitlines()[-1]]
        changed = [c for c in plain if plain[c] != exported.get(c)]
        changed += [c for c in exported if c not in plain]
        for manifest in stages.glob("*/manifest.json") if stages.exists() else []:
            data = json.loads(manifest.read_text())
            contract = data["contract"]
            runtimes = [r for c, (_, r) in plain.items() if c.endswith(f":{contract}")]
            if not any(
                r is not None and r.startswith(data["bytecode"]) for r in runtimes
            ):
                changed.append(f"manifest of {contract}")
        if changed:
            return label, "bytecode changed", changed
        count = len(list(stages.glob("*/manifest.json"))) if stages.exists() else 0
        return label, "exported" if count else "no code", [count]


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--solar", default=str(ROOT / "target/release/solar"))
    parser.add_argument("--jobs", type=int, default=os.cpu_count())
    parser.add_argument("--json", help="write every case's result to this file")
    args = parser.parse_args()
    solar = str(pathlib.Path(args.solar).resolve())

    ui = [
        p
        for p in sorted((ROOT / "tests/ui").rglob("*.sol"))
        if re.search(
            r"^\s*//@(\[[^\]]*\])?\s*run-call",
            p.read_text(errors="replace"),
            re.MULTILINE,
        )
    ]
    cases = [c for p in ui for c in ui_cases(p)] + foundry_cases()
    with concurrent.futures.ThreadPoolExecutor(args.jobs) as pool:
        results = list(pool.map(lambda case: sweep(case, solar), cases))

    outcomes = collections.Counter(kind for _, kind, _ in results)
    reasons, files = collections.Counter(), collections.defaultdict(set)
    for label, kind, detail in results:
        for message in dict.fromkeys(detail) if kind == "refused" else []:
            reasons[message] += 1
            files[message].add(label.split(" [")[0])
    contracts = sum(d[0] for _, kind, d in results if kind == "exported")
    sources = len(cases) - sum(len(ui_cases(p)) for p in ui)
    print(
        f"{len(ui)} run-call files and {sources} foundry sources: {len(cases)} compilations"
    )
    for kind in KINDS:
        print(f"  {outcomes[kind]:5}  {kind}")
    print(f"  {contracts:5}  contracts exported")
    print(
        "refused compilations and files by reason (a compilation counts once per reason):"
    )
    for message, count in reasons.most_common():
        print(f"  {count:5} {len(files[message]):5}  {message}")
    for label, kind, detail in results:
        if kind in ("bytecode changed", "export timed out"):
            print(f"{kind}: {label}: {detail}")
    if args.json:
        pathlib.Path(args.json).write_text(json.dumps(results, indent=1) + "\n")
    return 1 if outcomes["bytecode changed"] or outcomes["export timed out"] else 0


if __name__ == "__main__":
    sys.exit(main())
