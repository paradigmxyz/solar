"""Kernel proofs and independently checked witnesses for explicit-world entries."""

import json
import re
import tempfile
from pathlib import Path

from . import audit, native, report
from .check import PRELUDE, build_model, definition, run
from .common import Inconclusive, Program, Unsupported

UNFOLD = (
    "before, after, EVM.State.equivalent, EVM.State.observe, "
    "EVM.State.execute, EVM.State.run, EVM.State.instruction, EVM.State.available, "
    "EVM.State.next, EVM.State.stepFailed, EVM.State.stepExhausted, EVM.State.withMemory, EVM.State.account, "
    "EVM.State.putAccount, EVM.State.slotsRead, EVM.State.slotsWrite, EVM.State.warmSlot, "
    "EVM.State.warmAddress, EVM.State.memoryCharge, EVM.State.requiredWords, "
    "EVM.State.memoryCost, EVM.State.memoryWords, EVM.State.expand, EVM.State.readWord, "
    "EVM.State.readBytes, EVM.State.wordBytes, EVM.State.writeBytes, EVM.State.pureOp, "
    "EVM.State.pushValue, EVM.State.normalizedSlots, tvWorldConfig, "
    "EVM.step, Evm.add, Evm.sub, Evm.mul, Evm.eq, Evm.iszero, "
    "Storage.readPrice, Storage.writePrice, Storage.netMetered, Fork.atLeast, Fork.ordinal"
)


def check(
    a: Program,
    b: Program,
    timeout: int,
    *,
    replay_command: str,
    replay_executable: str,
    verbose: bool,
) -> audit.Result:
    if a.interface != b.interface or a.world is None or b.world is None:
        raise Unsupported("explicit-world interfaces differ")
    if a.world["configuration"] != b.world["configuration"]:
        raise Unsupported("explicit-world opcode configurations differ")
    build = build_model(timeout)
    if build.returncode:
        raise Unsupported(build.stderr or build.stdout)
    policy = "effects" if a.gas is None else a.gas
    declarations = (
        PRELUDE
        + "set_option maxRecDepth 16384\nset_option maxHeartbeats 2000000\n"
        + a.world["configuration"]
        + definition("before", a)
        + definition("after", b)
    )
    arguments = " ".join(a.inputs)
    binders = " ".join(f"({name} : BitVec 256)" for name in a.inputs)

    def relation(left, right):
        return f"EVM.State.equivalent tvWorldConfig tvWorldConfig .{policy} ({left}) ({right})"

    statement = f"theorem pair_correct {binders} : {relation('before ' + arguments, 'after ' + arguments)} := by\n"
    # Closed executions use kernel evaluation directly. Trying definitional
    # equality first duplicates hash evaluation and is much slower for creation.
    reflexivity = "    | rfl\n" if a.inputs else ""
    proof = (
        "  first\n" + reflexivity + "    | decide +kernel\n"
        f"    | (simp [{UNFOLD}] <;> (repeat (first | split | intro _)); all_goals simp_all <;> omega)\n"
    )
    with tempfile.TemporaryDirectory(prefix="solar-tv-world-proof-") as tmp:
        file = Path(tmp) / "WorldPair.lean"

        def lean(text):
            file.write_text(declarations + text)
            return run(
                ["lake", "env", "lean", file],
                Path(__file__).resolve().parents[1],
                timeout,
            )

        result = lean(statement + proof + "#print axioms pair_correct\n")
        log = result.stdout + result.stderr
        verified = result.returncode == 0
        if verified:
            axioms = re.search(
                r"^'pair_correct' depends on axioms: \[(.*?)\]$", log, re.MULTILINE
            )
            if "'pair_correct' does not depend on any axioms" not in log and (
                not axioms
                or set(axioms[1].split(", "))
                - {"propext", "Quot.sound", "Classical.choice"}
            ):
                raise Unsupported("unexpected world proof axioms\n" + log)
        patterns = [[0] * len(a.inputs)]
        if not verified:
            patterns.extend(
                [[value] * len(a.inputs) for value in (1, 2**255, 2**256 - 1)]
            )
        for values in patterns:
            concrete = " ".join(f"{value}#256" for value in values)
            left, right = "before " + concrete, "after " + concrete
            witness = (
                ""
                if verified
                else f"example : ¬ ({relation(left, right)}) := by decide +kernel\n"
            )
            observations = lean(
                witness
                + f"#eval IO.println (EVM.State.describe tvWorldConfig .{policy} ({left}))\n"
                + f"#eval IO.println (EVM.State.describe tvWorldConfig .{policy} ({right}))\n"
            )
            lines = observations.stdout.strip().splitlines()
            if any(line.startswith("unsupported:") for line in lines):
                raise Unsupported(
                    "\n".join(line for line in lines if line.startswith("unsupported:"))
                )
            if observations.returncode:
                continue
            try:
                outputs = [json.loads(line) for line in lines[-2:]]
            except ValueError:
                raise Inconclusive(
                    "invalid Lean world observations\n" + observations.stdout
                ) from None
            independent = native.replay(
                a, b, values, executable=replay_executable, timeout=timeout
            )
            try:
                native_outputs = [
                    json.loads(line) for line in independent["observations"]
                ]
            except ValueError:
                raise Inconclusive("invalid native world observations") from None
            if outputs != native_outputs:
                raise Inconclusive(
                    f"Lean and native world execution disagree\nLean: {outputs}\nNative: {native_outputs}"
                )
            if verified:
                return audit.Result("verified", "verified\n", program=a, pair=(a, b))
            descriptions = [
                json.dumps(value, separators=(",", ":")) for value in outputs
            ]
            return audit.Result(
                "counterexample",
                report.counterexample(
                    a,
                    b,
                    values,
                    descriptions[0],
                    descriptions[1],
                    replay_command,
                    verbose=verbose,
                    native_checked=True,
                ),
                program=a,
                pair=(a, b),
                witness={
                    "inputs": dict(zip(a.inputs, map(str, values), strict=True)),
                    "observations": outputs,
                    "native_observations": native_outputs,
                },
            )
        raise Inconclusive(
            "Lean could not prove or replay the explicit-world pair\n"
            + log
            + "\nConcrete replay diagnostics\n"
            + observations.stdout
            + observations.stderr
        )
