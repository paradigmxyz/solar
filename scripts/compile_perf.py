#!/usr/bin/env python3
"""Compiler-speed workflow: build, check output identity, measure, and profile.

Every command works under `target/compile-perf/`. A typical session:

    scripts/compile_perf.py corpus                      # standard JSON inputs from testdata/projects
    scripts/compile_perf.py build base --ref origin/main
    scripts/compile_perf.py build cand                  # current checkout, uncommitted changes included
    scripts/compile_perf.py identity base cand          # every output must stay byte-identical
    scripts/compile_perf.py stat base cand              # pinned `perf stat` medians
    scripts/compile_perf.py wall base cand              # parallel wall time with hyperfine
    scripts/compile_perf.py size base cand              # `.text` of each binary
    scripts/compile_perf.py profile cand -i seaport-1.6-gas
    scripts/compile_perf.py analyze PROFILE incl        # or self, children, callers, diff
    scripts/compile_perf.py analyze PROFILE callers --root 'Summaries>::new$' --caller 'run_pass$'
    scripts/compile_perf.py passes cand -i seaport-1.6-gas
    scripts/compile_perf.py clean                       # worktrees, binaries, corpus, profiles

Binaries are named builds under `target/compile-perf/bins/` or paths to any `solar` executable.
Builds use the `profiling` profile with one codegen unit by default, so partitioning noise does
not hide small changes. Release builds also use PGO and the `asm` feature; pass `--features asm`
to match the latter. Instruction counts are the stable signal; cycles move about 1% on a busy
machine, and a single run proves nothing.
"""

# /// script
# requires-python = ">=3.14"
# dependencies = []
# ///

from __future__ import annotations

import argparse
import collections
import concurrent.futures
import gzip
import json
import os
import re
import shlex
import shutil
import statistics
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
WORK = ROOT / "target" / "compile-perf"
BINS = WORK / "bins"
CORPUS = WORK / "corpus"
IDENTITY = WORK / "identity"
PROFILES = WORK / "profiles"
PROJECTS = ROOT / "testdata" / "projects"

# `runs >= 200` selects gas mode and smaller counts select size mode.
MODES = {
    "gas": {"enabled": True, "runs": 1000},
    "size": {"enabled": True, "runs": 100},
    "none": {"enabled": False, "runs": 200},
}
OUTPUTS = [
    "abi",
    "evm.bytecode.object",
    "evm.bytecode.sourceMap",
    "evm.bytecode.linkReferences",
    "evm.deployedBytecode.object",
    "evm.deployedBytecode.sourceMap",
    "evm.deployedBytecode.linkReferences",
    "evm.deployedBytecode.immutableReferences",
    "evm.methodIdentifiers",
    "metadata",
]
DEFAULT_INPUTS = [
    "seaport-1.6-gas",
    "v4-core-4.0.0-gas",
    "morpho-blue-1.0.0-gas",
    "solady-0.1.26-gas",
    "solady-0.1.26-size",
]
# A git fsmonitor daemon started under samply inherits perf events and keeps it waiting.
NO_FSMONITOR = {
    "GIT_CONFIG_COUNT": "1",
    "GIT_CONFIG_KEY_0": "core.fsmonitor",
    "GIT_CONFIG_VALUE_0": "false",
}


def main() -> None:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    commands = parser.add_subparsers(dest="command", required=True)

    build = commands.add_parser("build", help="build a named solar binary")
    build.add_argument("name")
    build.add_argument(
        "--ref", help="build a git ref in a worktree instead of the checkout"
    )
    build.add_argument("--codegen-units", type=int, default=1)
    build.add_argument("--features", help="comma-separated cargo features, e.g. asm")
    build.set_defaults(run=cmd_build)

    corpus = commands.add_parser(
        "corpus", help="write gas, size, and none standard JSON inputs"
    )
    corpus.add_argument(
        "--add",
        type=Path,
        action="append",
        default=[],
        help="extra standard JSON input (plain or .gz), e.g. a `forge build` input",
    )
    corpus.set_defaults(run=cmd_corpus)

    identity = commands.add_parser("identity", help="compare every corpus output")
    identity.add_argument("base")
    identity.add_argument("cand")
    identity.add_argument(
        "--rerun-base", action="store_true", help="recompute base outputs"
    )
    identity.add_argument("-j", "--jobs", type=int, default=os.cpu_count() or 4)
    identity.add_argument("--compiler-jobs", type=int, default=1)
    identity.add_argument(
        "--unordered-diagnostics",
        action="store_true",
        help="compare diagnostic contents without their parallel emission order",
    )
    identity.set_defaults(run=cmd_identity)

    stat = commands.add_parser(
        "stat", help="pinned `perf stat` medians, first binary as base"
    )
    stat.add_argument("bins", nargs="+")
    add_inputs(stat)
    stat.add_argument("-r", "--reps", type=int, default=5)
    stat.add_argument("--core", type=int, default=2)
    stat.add_argument("--jobs", default="1", help="solar `-j` value (default 1)")
    stat.set_defaults(run=cmd_stat)

    wall = commands.add_parser("wall", help="hyperfine wall time, parallel by default")
    wall.add_argument("bins", nargs="+")
    add_inputs(wall)
    wall.add_argument("-r", "--runs", type=int, default=7)
    wall.add_argument("--jobs", help="solar `-j` value (default: solar's own)")
    wall.set_defaults(run=cmd_wall)

    size = commands.add_parser("size", help="ELF section sizes, first binary as base")
    size.add_argument("bins", nargs="+")
    size.set_defaults(run=cmd_size)

    profile = commands.add_parser(
        "profile", help="record a presymbolicated samply profile"
    )
    profile.add_argument("bin")
    add_inputs(profile, default=["seaport-1.6-gas"])
    profile.add_argument("--rate", type=int, default=5000)
    profile.add_argument("--jobs", default="1")
    profile.set_defaults(run=cmd_profile)

    analyze = commands.add_parser("analyze", help="summarize samply profiles")
    analyze.add_argument("profile", type=Path)
    analyze.add_argument(
        "mode",
        choices=["incl", "self", "children", "callers", "diff"],
        help="incl/self: whole profile or under --root; children: direct callees of --root; "
        "callers: nearest caller of --root; diff: self-time change against --other",
    )
    analyze.add_argument("--root", help="function name regex")
    analyze.add_argument(
        "--caller",
        help="for `callers`: report the nearest ancestor matching this regex, e.g. 'run_pass$'",
    )
    analyze.add_argument("--other", type=Path, help="second profile for `diff`")
    analyze.add_argument("-n", "--top", type=int, default=30)
    analyze.set_defaults(run=cmd_analyze)

    passes = commands.add_parser(
        "passes", help="pass runs, change rates, and rounded times"
    )
    passes.add_argument("bin")
    add_inputs(passes, default=["seaport-1.6-gas"])
    passes.add_argument("-n", "--top", type=int, default=40)
    passes.set_defaults(run=cmd_passes)

    clean = commands.add_parser(
        "clean", help="remove ref worktrees and everything under target/compile-perf"
    )
    clean.set_defaults(run=cmd_clean)

    args = parser.parse_args()
    args.run(args)


def add_inputs(
    parser: argparse.ArgumentParser, default: list[str] | None = None
) -> None:
    parser.add_argument(
        "-i",
        "--input",
        dest="inputs",
        action="append",
        help=f"corpus input name (default: {' '.join(default or DEFAULT_INPUTS)})",
    )
    parser.set_defaults(default_inputs=default or DEFAULT_INPUTS)


def inputs_of(args: argparse.Namespace) -> list[Path]:
    names = args.inputs or args.default_inputs
    paths = [CORPUS / f"{name}.json" for name in names]
    missing = [path.name for path in paths if not path.exists()]
    if missing:
        sys.exit(f"missing corpus inputs {missing}; run `corpus` first")
    return paths


def binary(name: str) -> Path:
    path = BINS / name / "solar"
    if path.exists():
        return path
    path = Path(name)
    if path.exists():
        return path.resolve()
    sys.exit(f"unknown binary `{name}`: not in {BINS} and not a path")


def label(name: str) -> str:
    return Path(name).parent.name if "/" in name else name


# === build ===


def cmd_build(args: argparse.Namespace) -> None:
    if not re.fullmatch(r"[\w.-]+", args.name):
        sys.exit(
            f"invalid build name `{args.name}`: use letters, digits, `.`, `_`, or `-`"
        )
    source = ROOT
    if args.ref:
        source = WORK / "worktrees" / args.name
        if source.exists():
            run(["git", "-C", source, "checkout", "--quiet", "--detach", args.ref])
        else:
            source.parent.mkdir(parents=True, exist_ok=True)
            run(["git", "-C", ROOT, "worktree", "add", "--detach", source, args.ref])
    command = [
        "cargo",
        "build",
        "--profile",
        "profiling",
        "-p",
        "solar-compiler",
        "--bin",
        "solar",
    ]
    if args.features:
        command += ["--features", args.features]
    target = WORK / "cargo-target"
    env = os.environ | {
        "CARGO_TARGET_DIR": str(target),
        "CARGO_PROFILE_PROFILING_CODEGEN_UNITS": str(args.codegen_units),
    }
    run(command, cwd=source, env=env)
    destination = BINS / args.name / "solar"
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(target / "profiling" / "solar", destination)
    commit = capture(["git", "-C", source, "rev-parse", "--short", "HEAD"]).strip()
    dirty = capture(
        ["git", "-C", source, "status", "--porcelain", "--untracked-files=no"]
    )
    note = f"{commit}{' + uncommitted changes' if dirty.strip() else ''}"
    if args.features:
        note += f", features {args.features}"
    (destination.parent / "source.txt").write_text(note + "\n")
    print(f"{destination} ({note})")


# === corpus ===


def cmd_corpus(args: argparse.Namespace) -> None:
    CORPUS.mkdir(parents=True, exist_ok=True)
    sources = sorted(PROJECTS.glob("*.json.gz")) + args.add
    for source in sources:
        opener = gzip.open if source.suffix == ".gz" else open
        with opener(source, "rt") as file:
            data = json.load(file)
        name = source.name.removesuffix(".gz").removesuffix(".json")
        for mode, optimizer in MODES.items():
            settings = data.setdefault("settings", {})
            settings["optimizer"] = optimizer
            settings["outputSelection"] = {"*": {"*": OUTPUTS}}
            (CORPUS / f"{name}-{mode}.json").write_text(json.dumps(data))
    print(f"wrote {len(sources) * len(MODES)} inputs to {CORPUS}")


# === identity ===


def cmd_identity(args: argparse.Namespace) -> None:
    inputs = sorted(CORPUS.glob("*.json"))
    if not inputs:
        sys.exit("empty corpus; run `corpus` first")
    base = binary(args.base)
    base_dir = IDENTITY / label(args.base)
    stamp = base_dir / "stamp.json"
    if (
        args.rerun_base
        or not stamp.exists()
        or stamp.read_text() != identity_stamp(base, inputs, args.compiler_jobs)
    ):
        compile_all(base, inputs, base_dir, args.jobs, args.compiler_jobs)
    cand_dir = IDENTITY / label(args.cand)
    compile_all(binary(args.cand), inputs, cand_dir, args.jobs, args.compiler_jobs)
    different = [
        path.stem
        for path in inputs
        if not same_file(base_dir / f"{path.stem}.code", cand_dir / f"{path.stem}.code")
        or not same_output(
            base_dir / f"{path.stem}.out",
            cand_dir / f"{path.stem}.out",
            args.unordered_diagnostics,
        )
    ]
    if different:
        for name in different:
            print(f"DIFF {name}: {base_dir / name}.out vs {cand_dir / name}.out")
        sys.exit(
            f"{len(different)} of {len(inputs)} outputs differ from `{label(args.base)}`"
        )
    qualification = " (ignoring diagnostic order)" if args.unordered_diagnostics else ""
    print(
        f"identical: all {len(inputs)} outputs match `{label(args.base)}`{qualification}"
    )


def compile_all(
    solar: Path, inputs: list[Path], out: Path, jobs: int, compiler_jobs: int
) -> None:
    out.mkdir(parents=True, exist_ok=True)

    def one(path: Path) -> None:
        with path.open("rb") as stdin:
            result = subprocess.run(
                [solar, "--standard-json", f"-j{compiler_jobs}"],
                stdin=stdin,
                capture_output=True,
                check=False,
            )
        (out / f"{path.stem}.out").write_bytes(result.stdout)
        (out / f"{path.stem}.err").write_bytes(result.stderr)
        (out / f"{path.stem}.code").write_text(f"{result.returncode}\n")

    with concurrent.futures.ThreadPoolExecutor(jobs) as pool:
        list(pool.map(one, inputs))
    (out / "stamp.json").write_text(identity_stamp(solar, inputs, compiler_jobs))


def identity_stamp(solar: Path, inputs: list[Path], compiler_jobs: int) -> str:
    """Identifies the binary and corpus behind saved outputs, so stale outputs are recomputed."""
    info = solar.stat()
    files = {path.name: path.stat().st_mtime_ns for path in inputs}
    return json.dumps(
        {
            "binary": [str(solar), info.st_size, info.st_mtime_ns],
            "inputs": files,
            "compiler_jobs": compiler_jobs,
        }
    )


def same_file(a: Path, b: Path) -> bool:
    return a.exists() and b.exists() and a.read_bytes() == b.read_bytes()


def same_output(a: Path, b: Path, unordered_diagnostics: bool) -> bool:
    if same_file(a, b):
        return True
    if not unordered_diagnostics:
        return False
    try:
        outputs = [json.loads(path.read_bytes()) for path in (a, b)]
    except ValueError, UnicodeDecodeError:
        return False
    for output in outputs:
        if not isinstance(output, dict):
            return False
        if "errors" in output:
            output["errors"].sort(key=lambda error: json.dumps(error, sort_keys=True))
    return outputs[0] == outputs[1]


# === stat, wall, size ===


def cmd_stat(args: argparse.Namespace) -> None:
    bins = [binary(name) for name in args.bins]
    for path in inputs_of(args):
        samples: dict[int, dict[str, list[float]]] = {
            index: collections.defaultdict(list) for index in range(len(bins))
        }
        for _ in range(args.reps):
            for index, solar in enumerate(bins):
                for event, value in perf_stat(
                    solar, path, args.core, args.jobs
                ).items():
                    samples[index][event].append(value)
        rows = []
        base: dict[str, float] = {}
        for index, name in enumerate(args.bins):
            median = {
                event: statistics.median(values)
                for event, values in samples[index].items()
            }
            base = base or median
            rows.append(
                [
                    label(name),
                    f"{median['instructions'] / 1e9:.3f} G",
                    change(median["instructions"], base["instructions"]),
                    f"{median['cycles'] / 1e9:.3f} G",
                    change(median["cycles"], base["cycles"]),
                    f"{median['task-clock']:.0f} ms",
                ]
            )
        print(f"\n{path.stem} (median of {args.reps}, core {args.core}, -j{args.jobs})")
        table(["binary", "instructions", "", "cycles", "", "time"], rows)


def perf_stat(solar: Path, path: Path, core: int, jobs: str) -> dict[str, float]:
    command: list[Any] = ["taskset", "-c", str(core), "perf", "stat", "-x,"]
    command += ["-e", "instructions:u,cycles:u,task-clock"]
    command += ["--", solar, "--standard-json", f"-j{jobs}"]
    with path.open("rb") as stdin:
        result = subprocess.run(
            [str(arg) for arg in command],
            stdin=stdin,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
            # Keep `.` as the decimal point; locales that use `,` break the `-x,` fields.
            env=os.environ | {"LC_ALL": "C"},
        )
    values = {}
    for line in result.stderr.splitlines():
        fields = line.split(",")
        if len(fields) > 2 and fields[0].replace(".", "", 1).isdigit():
            event = fields[2].split(":")[0]
            values[event] = float(fields[0])
    return values


def cmd_wall(args: argparse.Namespace) -> None:
    bins = [binary(name) for name in args.bins]
    jobs = [f"-j{args.jobs}"] if args.jobs else []
    for path in inputs_of(args):
        with tempfile.NamedTemporaryFile(suffix=".json") as export:
            command: list[Any] = [
                "hyperfine",
                "-N",
                "-w",
                "1",
                "-r",
                str(args.runs),
                "--style",
                "none",
            ]
            command += [
                "--input",
                path,
                "--output",
                "null",
                "--export-json",
                export.name,
            ]
            for name, solar in zip(args.bins, bins, strict=True):
                command += [
                    "-n",
                    label(name),
                    shlex.join([str(solar), "--standard-json", *jobs]),
                ]
            run(command)
            results = json.loads(Path(export.name).read_text())["results"]
        base = results[0]["mean"]
        rows = [
            [
                result["command"],
                f"{result['mean'] * 1e3:.1f} ms",
                f"± {result['stddev'] * 1e3:.1f}",
                change(result["mean"], base),
            ]
            for result in results
        ]
        print(
            f"\n{path.stem} ({args.runs} runs, {jobs[0] if jobs else 'default jobs'})"
        )
        table(["binary", "mean", "stddev", ""], rows)


def cmd_size(args: argparse.Namespace) -> None:
    sections = [elf_sections(binary(name)) for name in args.bins]
    rows = []
    for name, section in zip(args.bins, sections, strict=True):
        rows.append(
            [
                label(name),
                f"{section.get('.text', 0):,}",
                change(section.get(".text", 0), sections[0].get(".text", 0)),
                f"{section.get('.rodata', 0):,}",
                f"{binary(name).stat().st_size:,}",
            ]
        )
    table(["binary", ".text", "", ".rodata", "file"], rows)


def elf_sections(path: Path) -> dict[str, int]:
    sizes = {}
    for line in capture(["size", "-A", path]).splitlines():
        fields = line.split()
        if len(fields) >= 2 and fields[0].startswith(".") and fields[1].isdigit():
            sizes[fields[0]] = int(fields[1])
    return sizes


# === profile, analyze ===


def cmd_profile(args: argparse.Namespace) -> None:
    solar = binary(args.bin)
    PROFILES.mkdir(parents=True, exist_ok=True)
    for path in inputs_of(args):
        out = PROFILES / f"{label(args.bin)}-{path.stem}.json.gz"
        command: list[Any] = [
            "samply",
            "record",
            "-r",
            str(args.rate),
            "--presymbolicate",
            "--save-only",
        ]
        command += ["-o", out, "--", solar, "--standard-json", f"-j{args.jobs}"]
        with path.open("rb") as stdin:
            subprocess.run(
                [str(arg) for arg in command],
                stdin=stdin,
                stdout=subprocess.DEVNULL,
                env=os.environ | NO_FSMONITOR,
                check=True,
            )
        print(f"samply load {out}")


class Profile:
    """Samples of a samply profile as root-to-leaf function-name chains."""

    def __init__(self, path: Path) -> None:
        with gzip.open(path) as file:
            data = json.load(file)
        shared = data["shared"]
        strings = shared["stringArray"]
        names = [
            re.sub(r"::\{closure#\d+\}", "{c}", strings[index])
            for index in shared["funcTable"]["name"]
        ]
        frame_func = shared["frameTable"]["func"]
        stacks = shared["stackTable"]
        frames = stacks["frame"]
        prefixes = [
            index - offset if offset else None
            for index, offset in enumerate(stacks["prefixOffset"])
        ]
        chains: dict[int, list[str]] = {}

        def chain(stack: int) -> list[str]:
            if stack not in chains:
                out = []
                current: int | None = stack
                while current is not None:
                    out.append(names[frame_func[frames[current]]])
                    current = prefixes[current]
                out.reverse()
                chains[stack] = out
            return chains[stack]

        self.samples: list[tuple[list[str], int]] = []
        for thread in data["threads"]:
            if thread["name"] == "samply":
                continue
            samples = thread["samples"]
            weights = samples.get("weight") or [1] * len(samples["stack"])
            for stack, weight in zip(samples["stack"], weights, strict=True):
                if stack is not None:
                    self.samples.append((chain(stack), weight or 1))
        self.total = sum(weight for _, weight in self.samples)

    def counts(
        self,
        mode: str,
        root: re.Pattern[str] | None,
        caller: re.Pattern[str] | None = None,
    ) -> collections.Counter[str]:
        counts: collections.Counter[str] = collections.Counter()
        for chain, weight in self.samples:
            start = 0
            if root:
                found = next(
                    (i for i, name in enumerate(chain) if root.search(name)), None
                )
                if found is None:
                    continue
                start = found
            if mode == "incl":
                counts.update(dict.fromkeys(set(chain[start:]), weight))
            elif mode == "self":
                counts[chain[-1]] += weight
            elif mode == "children":
                counts[chain[start + 1] if start + 1 < len(chain) else "<self>"] += (
                    weight
                )
            elif mode == "callers":
                counts[
                    next(
                        (
                            name
                            for name in reversed(chain[:start])
                            if (caller.search(name) if caller else not is_wrapper(name))
                        ),
                        "<root>",
                    )
                ] += weight
        return counts


def is_wrapper(name: str) -> bool:
    """Closures and standard library adapters, skipped when looking for a caller."""
    return "{c}" in name or name.startswith(
        ("core::", "<core::", "alloc::", "<alloc::", "std::")
    )


def cmd_analyze(args: argparse.Namespace) -> None:
    if args.mode in ("children", "callers") and not args.root:
        sys.exit(f"`{args.mode}` needs --root")
    if args.mode == "diff" and not args.other:
        sys.exit("`diff` needs --other")
    for path in (args.profile, args.other):
        if path and not path.exists():
            sys.exit(f"no profile at {path}")
    profile = Profile(args.profile)
    root = re.compile(str(args.root)) if args.root else None
    if args.mode == "diff":
        other = Profile(args.other)
        before, after = profile.counts("self", root), other.counts("self", root)
        deltas = {
            name: after[name] / other.total - before[name] / profile.total
            for name in set(before) | set(after)
        }
        print(
            f"samples {profile.total} -> {other.total} ({change(other.total, profile.total)})"
        )
        ranked = sorted(deltas.items(), key=lambda item: abs(item[1]), reverse=True)[
            : args.top
        ]
        for name, delta in ranked:
            print(
                f"{delta * 100:+6.2f}  {before[name] / profile.total * 100:5.2f} -> "
                f"{after[name] / other.total * 100:5.2f}  {name[:150]}"
            )
        return
    caller = re.compile(str(args.caller)) if args.caller else None
    counts = profile.counts(args.mode, root, caller)
    matched = sum(
        weight
        for chain, weight in profile.samples
        if not root or any(root.search(n) for n in chain)
    )
    print(f"{matched / profile.total * 100:.2f}% of {profile.total} samples")
    for name, weight in counts.most_common(args.top):
        print(f"{weight / profile.total * 100:6.2f}%  {name[:170]}")


# === passes ===


def cmd_passes(args: argparse.Namespace) -> None:
    solar = binary(args.bin)
    for path in inputs_of(args):
        with path.open("rb") as stdin:
            result = subprocess.run(
                [solar, "--standard-json", "-j1", "-Ztime-passes"],
                stdin=stdin,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.PIPE,
                text=True,
                check=False,
            )
        stats: dict[str, list[float]] = collections.defaultdict(lambda: [0, 0, 0.0])
        # time:   0.001	MIR Contract pass changed=true
        # time:   0.001	EVM IR Contract_runtime pass changed=false
        pattern = re.compile(
            r"time:\s+([\d.]+)\t(MIR|EVM IR) \S+ (\S+) changed=(true|false)"
        )
        for line in result.stderr.splitlines():
            if match := pattern.match(line):
                entry = stats[f"{match[2]} {match[3]}"]
                entry[0] += 1
                entry[1] += match[4] == "true"
                entry[2] += float(match[1])
        rows = [
            [
                name,
                f"{runs:.0f}",
                f"{changed / runs * 100:.1f}%",
                f"{seconds * 1e3:.0f}",
            ]
            for name, (runs, changed, seconds) in sorted(
                stats.items(), key=lambda item: item[1][2], reverse=True
            )[: args.top]
        ]
        print(f"\n{path.stem} (times round to 1 ms per run; take time from profiles)")
        table(["pass", "runs", "changed", "ms"], rows)


# === clean ===


def cmd_clean(_args: argparse.Namespace) -> None:
    for worktree in sorted((WORK / "worktrees").glob("*")):
        run(["git", "-C", ROOT, "worktree", "remove", "--force", worktree])
    if WORK.exists():
        shutil.rmtree(WORK)
    print(f"removed {WORK}")


# === helpers ===


def change(value: float, base: float) -> str:
    return f"{(value / base - 1) * 100:+.2f}%" if base else ""


def table(header: list[str], rows: list[list[str]]) -> None:
    widths = [
        max(len(str(row[i])) for row in [header, *rows]) for i in range(len(header))
    ]
    for row in [header, *rows]:
        print(
            "  ".join(
                str(cell).ljust(width) for cell, width in zip(row, widths, strict=True)
            )
        )


def run(
    command: list[Any], cwd: Path | None = None, env: dict[str, str] | None = None
) -> None:
    subprocess.run([str(arg) for arg in command], cwd=cwd, env=env, check=True)


def capture(command: list[Any]) -> str:
    return subprocess.run(
        [str(arg) for arg in command], capture_output=True, text=True, check=True
    ).stdout


if __name__ == "__main__":
    main()
