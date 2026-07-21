#!/usr/bin/env -S uv run
# /// script
# requires-python = ">=3.11"
# dependencies = ["tabulate>=0.9"]
# ///
"""Capture and analyze EVM IR peephole tracing output.

Examples:
    ./scripts/analyze_evm_ir_peepholes.py capture /tmp/peepholes.log -- \
        cargo nextest run --workspace
    ./scripts/analyze_evm_ir_peepholes.py capture --append /tmp/peepholes.log -- \
        cargo bench -p solar-bench --bench criterion -- codegen
    ./scripts/analyze_evm_ir_peepholes.py analyze /tmp/peepholes.log
"""

import argparse
import os
import re
import subprocess
import sys
from collections import Counter
from pathlib import Path

from tabulate import tabulate


DEFAULT_RUST_LOG = (
    "solar::codegen::evm_ir::peephole=trace,"
    "solar::codegen::mir::inst_simplify=trace,"
    "solar_codegen=debug"
)
ANSI_RE = re.compile(r"\x1b\[[0-9;]*m")
STACK_OP_RE = re.compile(r"^(?:DUP|SWAP)\d+$|^POP$", re.IGNORECASE)
EXACT_PUSH_RE = re.compile(r"^PUSH (0x[0-9a-f]+|[0-9]+)$", re.IGNORECASE)


def field(line: str, name: str) -> str | None:
    match = re.search(rf"(?:^|\s){re.escape(name)}=(?:\"([^\"]*)\"|(\S*))", line)
    if match is None:
        return None
    return match.group(1) if match.group(1) is not None else match.group(2)


def capture(args: argparse.Namespace) -> int:
    command = args.command
    if command and command[0] == "--":
        command = command[1:]
    if not command:
        raise SystemExit("capture requires a command after `--`")

    path = Path(args.log)
    path.parent.mkdir(parents=True, exist_ok=True)
    mode = "a" if args.append else "w"
    with path.open(mode) as output:
        marker = f"# command: {' '.join(command)}\n"
        output.write(marker)
    print(marker, end="", file=sys.stderr)
    env = {
        **os.environ,
        "NO_COLOR": "1",
        "RUST_LOG": args.rust_log,
        "SOLAR_LOG_FILE": str(path.resolve()),
    }
    return subprocess.run(command, cwd=args.cwd, env=env).returncode


def markdown_table(headers: list[str], rows: list[list[object]]) -> None:
    print(tabulate(rows, headers=headers, tablefmt="pipe", colalign=("left",) + ("right",) * (len(headers) - 1)))
    print()


def normalize_instruction(instruction: str) -> str:
    match = EXACT_PUSH_RE.match(instruction)
    if match is None:
        return instruction
    value = int(match.group(1), 0)
    if value < 2:
        return f"PUSH {value}"
    return "PUSH"


def analyze(args: argparse.Namespace) -> int:
    rewrites: Counter[tuple[str, str]] = Counter()
    rewrite_modules: dict[tuple[str, str], Counter[str]] = {}
    ngrams: Counter[tuple[str, ...]] = Counter()
    mir_round_runs: Counter[int] = Counter()
    mir_round_changed: Counter[int] = Counter()
    mir_round_simplified: Counter[int] = Counter()
    blocks = 0

    with open(args.log) as log:
        for raw_line in log:
            line = ANSI_RE.sub("", raw_line)
            if "solar::codegen::evm_ir::peephole: rewrite" in line:
                module_match = re.search(r"evm_codegen\{module=([^}]+)\}", line)
                if module_match is None:
                    module_match = re.search(r"evm_ir_pipeline\{program=([^}]+)\}", line)
                module = module_match.group(1) if module_match else "unknown"
                input_sequence = field(line, "input") or ""
                output_sequence = field(line, "output") or ""
                rewrite = (input_sequence, output_sequence)
                rewrites[rewrite] += 1
                rewrite_modules.setdefault(rewrite, Counter())[module] += 1
            elif "solar::codegen::evm_ir::peephole: input" in line:
                blocks += 1
                instructions = (field(line, "instructions") or "").split(",")
                instructions = [instruction for instruction in instructions if instruction]
                for size in range(2, args.max_pattern + 1):
                    ngrams.update(zip(*(instructions[offset:] for offset in range(size))))
            elif "mir_inst_simplify_round" in line:
                round_number = int(field(line, "round") or 0)
                simplified = int(field(line, "simplified") or 0)
                mir_round_runs[round_number] += 1
                mir_round_changed[round_number] += simplified != 0
                mir_round_simplified[round_number] += simplified

    if not rewrites and blocks == 0:
        print("No EVM IR peephole trace events found", file=sys.stderr)
        return 1

    total_rewrites = sum(rewrites.values())
    print(f"Analyzed {blocks:,} eligible input blocks and {total_rewrites:,} rewrites.\n")
    markdown_table(
        ["Input → Output", "Hits", "Share", "Top modules"],
        [
            [
                f"{input_sequence or '∅'} → {output_sequence or '∅'}",
                count,
                f"{count / total_rewrites:.1%}" if total_rewrites else "-",
                ", ".join(
                    f"{module} ({count})"
                    for module, count in rewrite_modules.get(
                        (input_sequence, output_sequence), Counter()
                    ).most_common(3)
                )
                or "-",
            ]
            for (input_sequence, output_sequence), count in rewrites.most_common(args.top)
        ],
    )

    if mir_round_runs:
        print("MIR instruction simplifier fixpoint rounds:\n")
        markdown_table(
            ["Round", "Runs", "Changed runs", "Simplifications"],
            [
                [round_number, runs, mir_round_changed[round_number], mir_round_simplified[round_number]]
                for round_number, runs in sorted(mir_round_runs.items())
            ],
        )

    rewritten_inputs = {
        tuple(normalize_instruction(instruction) for instruction in input_sequence.split(","))
        for input_sequence, _ in rewrites
    }

    def overlaps_rewrite(pattern: tuple[str, ...]) -> bool:
        def contains(sequence: tuple[str, ...], subsequence: tuple[str, ...]) -> bool:
            return any(
                sequence[offset : offset + len(subsequence)] == subsequence
                for offset in range(len(sequence) - len(subsequence) + 1)
            )

        return any(
            contains(sequence, pattern) or contains(pattern, sequence)
            for sequence in rewritten_inputs
        )

    candidates = [
        (pattern, count)
        for pattern, count in ngrams.most_common()
        if count >= args.min_count
        and any(STACK_OP_RE.match(instruction) for instruction in pattern)
        and not overlaps_rewrite(pattern)
    ][: args.top]
    print("Frequent stack-instruction patterns not exactly matched by a recorded rewrite:\n")
    markdown_table(
        ["Pattern", "Occurrences"],
        [[",".join(pattern), count] for pattern, count in candidates],
    )
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="subcommand", required=True)

    capture_parser = subparsers.add_parser("capture", help="run a command and capture its trace")
    capture_parser.add_argument("--append", action="store_true")
    capture_parser.add_argument("--cwd", default=str(Path(__file__).resolve().parent.parent))
    capture_parser.add_argument("--rust-log", default=DEFAULT_RUST_LOG)
    capture_parser.add_argument("log")
    capture_parser.add_argument("command", nargs=argparse.REMAINDER)
    capture_parser.set_defaults(func=capture)

    analyze_parser = subparsers.add_parser("analyze", help="print Markdown hit and pattern tables")
    analyze_parser.add_argument("--max-pattern", type=int, choices=range(2, 7), default=4)
    analyze_parser.add_argument("--min-count", type=int, default=2)
    analyze_parser.add_argument("--top", type=int, default=30)
    analyze_parser.add_argument("log")
    analyze_parser.set_defaults(func=analyze)
    return parser.parse_args()


if __name__ == "__main__":
    parsed_args = parse_args()
    sys.exit(parsed_args.func(parsed_args))
