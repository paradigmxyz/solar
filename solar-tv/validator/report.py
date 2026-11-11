"""Human-readable witnesses, canonical IR changes, and replay commands."""

import difflib
import json
import re
import shlex
import subprocess
import sys
from pathlib import Path

from .common import Program


def region(program: Program) -> str:
    if program.result_type == "EVM.Execution":
        return f"EVM IR window bb0, entry stack height {program.interface[1]}"
    return f"MIR function @{program.interface[0]}"


def difference(before: str, after: str) -> str:
    edge = re.compile(r"(fallthrough|bb\d+), stack=(\[.*\])")
    left, right = edge.fullmatch(before), edge.fullmatch(after)
    if left and right:
        if left[1] != right[1]:
            return "Outgoing edge changed"
        a, b = json.loads(left[2]), json.loads(right[2])
        if len(a) != len(b):
            return f"Outgoing stack height changed from {len(a)} to {len(b)}"
        index = next(
            (i for i, (x, y) in enumerate(zip(a, b, strict=True)) if x != y), None
        )
        if index is None:
            return "Execution outcomes differ"
        return f"Outgoing stack word {index} changed (word 0 is the top)"
    if before.startswith("revert") and after.startswith("revert"):
        return "Revert payload changed"
    if before.startswith("revert"):
        return "Revert changed to " + ("a return" if after.isdecimal() else after)
    if after.startswith("revert"):
        return "Execution changed to a revert"
    if (
        before.startswith("exception")
        or after.startswith("exception")
        or "invalid" in {before, after}
    ):
        return "Exceptional halt behavior changed"
    if left or right or before.startswith("success") or after.startswith("success"):
        return "Execution outcome changed"
    return "Returned value changed"


def ir_diff(before: str, after: str, *, verbose: bool) -> str:
    def canonical(text: str) -> list[str]:
        # Compiler dump banners contain paths, not IR. Predecessor comments are
        # derived from the code and otherwise obscure small instruction edits.
        return [
            line.split(";", 1)[0].rstrip()
            for line in text.splitlines()
            if line.strip() and not line.lstrip().startswith(";")
        ]

    diff = list(
        difflib.unified_diff(
            canonical(before),
            canonical(after),
            fromfile="before",
            tofile="after",
            n=2,
            lineterm="",
        )
    )
    if not verbose and len(diff) > 60:
        omitted = len(diff) - 60
        diff = [*diff[:60], f"... {omitted} more diff lines. Use --verbose to show all"]
    return "\n".join(diff)


def replay_command(ir: str, before: Path, after: Path, solar: str, timeout: int) -> str:
    checker = Path(__file__).resolve().parents[1] / "solar-tv"
    argv = [
        str(checker),
        ir,
        str(before.resolve()),
        str(after.resolve()),
        "--solar",
        solar,
        "--timeout",
        str(timeout),
    ]
    if sys.platform == "win32":
        argv = [sys.executable, *argv]
        return "& " + " ".join("'" + arg.replace("'", "''") + "'" for arg in argv)
    return shlex.join(argv)


def counterexample(
    program: Program,
    candidate: Program,
    values: list[int],
    before: str,
    after: str,
    replay: str,
    *,
    verbose: bool,
) -> str:
    lines = ["counterexample", f"  {region(program)}"]
    lines.extend(
        f"  {name} = {value}"
        for name, value in zip(program.inputs, values, strict=True)
    )
    lines.extend(
        [
            f"  before = {before}",
            f"  after  = {after}",
            "",
            "Difference",
            f"  {difference(before, after)}",
        ]
    )
    changes = ir_diff(program.canonical_ir, candidate.canonical_ir, verbose=verbose)
    if changes:
        lines.extend(["", "Changed IR", changes])
    lines.extend(["", "Replay", f"  {replay}"])
    return "\n".join(lines) + "\n"


def inconclusive(error: Exception, replay: str, *, verbose: bool) -> str:
    details = str(error).strip()
    if isinstance(error, subprocess.TimeoutExpired):
        command = error.cmd if isinstance(error.cmd, list) else [error.cmd]
        stage = (
            "compiler validation"
            if "-Zvalidate-ir=true" in command
            else "Lean validation"
        )
        reason = f"{stage} timed out after {error.timeout} seconds"
        hint = "Increase --timeout to allow more time"
    else:
        reason = details.splitlines()[0] if details else type(error).__name__
        hint = (
            "Use --verbose for full compiler and prover diagnostics"
            if "\n" in details
            else ""
        )
    lines = [f"inconclusive: {reason}"]
    if verbose and "\n" in details:
        lines.extend(["", "Diagnostics", details])
    elif hint:
        lines.append(f"  {hint}")
    lines.extend(["", "Replay", f"  {replay}"])
    return "\n".join(lines) + "\n"
