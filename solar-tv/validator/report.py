"""Human-readable witnesses, canonical IR changes, and replay commands."""

import difflib
import json
import re
import shlex
import subprocess
import sys
from pathlib import Path

from .common import Program, ReplayTimeout


def region(program: Program) -> str:
    if program.interface[0] == "evm-window":
        region = f"EVM IR window {program.selector or 'bb0'}, entry stack height {program.interface[1]}"
        if program.gas is not None:
            region += f" (gas={program.gas}, evm={program.evm_version})"
        return region
    return f"MIR function @{program.interface[0]}"


def difference(before: str, after: str) -> str:
    if before.startswith("out-of-gas") or after.startswith("out-of-gas"):
        return "Out-of-gas behavior changed"
    remaining = re.compile(r", gas=(\d+)$")
    left_gas, right_gas = remaining.search(before), remaining.search(after)
    if left_gas and right_gas:
        if before[: left_gas.start()] == after[: right_gas.start()]:
            return f"Remaining gas changed from {left_gas[1]} to {right_gas[1]}"
        before, after = before[: left_gas.start()], after[: right_gas.start()]
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
    size = re.compile(r"\bmsize=(\d+)")
    left_size, right_size = size.search(before), size.search(after)
    if left_size and right_size and left_size[1] != right_size[1]:
        return (
            f"Active memory size changed from {left_size[1]} to {right_size[1]} bytes"
        )
    returned = re.compile(r"return=(\d+),")
    left_value, right_value = returned.match(before), returned.match(after)
    if left_value and right_value and left_value[1] != right_value[1]:
        return "Returned value changed"
    contents = re.compile(r"memory=(\[.*\])")
    left_memory, right_memory = contents.search(before), contents.search(after)
    if left_memory and right_memory and left_memory[1] != right_memory[1]:
        a, b = json.loads(left_memory[1]), json.loads(right_memory[1])
        index = next(i for i, (x, y) in enumerate(zip(a, b, strict=True)) if x != y)
        return f"Memory contents changed in word at byte offset {index * 32}"
    if before.startswith("success(data=") or after.startswith("success(data="):
        return "Returned byte payload changed"
    edge = re.compile(r"(fallthrough|bb\d+), stack=(\[[^]]*\])")
    left, right = edge.match(before), edge.match(after)
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


def replay_command(
    ir: str,
    before: Path,
    after: Path,
    solar: str,
    timeout: int,
    *,
    gas: str | None = None,
    evm_version: str | None = None,
    replay: str | None = None,
    function: str | None = None,
    window: str | None = None,
    stack_height: int | None = None,
) -> str:
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
    if gas is not None:
        argv.extend(["--gas", gas])
    if evm_version is not None:
        argv.extend(["--evm-version", evm_version])
    if replay is not None:
        argv.extend(["--replay", replay])
    if function is not None:
        argv.extend(["--function", function])
    if window is not None:
        argv.extend(["--window", window])
        if stack_height is not None:
            argv.extend(["--stack-height", str(stack_height)])
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
    native_checked: bool = False,
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
        ]
    )
    if native_checked:
        lines.extend(
            [
                "Replay checks",
                "  Lean kernel: confirmed",
                "  Rust interpreter: matched",
                "",
            ]
        )
    lines.extend(["Difference", f"  {difference(before, after)}"])
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
            "Rust interpreter replay"
            if isinstance(error, ReplayTimeout)
            else (
                "compiler validation"
                if "-Zvalidate-ir=true" in command
                else "Lean validation"
            )
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
