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
    "EVM.State.execute, EVM.State.reduceRun, EVM.State.instruction, EVM.State.available, "
    "EVM.State.next, EVM.State.stepFailed, EVM.State.stepExhausted, EVM.State.withMemory, EVM.State.account, "
    "EVM.State.putAccount, EVM.State.slotsRead, EVM.State.slotsWrite, EVM.State.warmSlot, "
    "EVM.State.warmAddress, EVM.State.memoryCharge, EVM.State.requiredWords, "
    "EVM.State.memoryCost, EVM.State.memoryWords, EVM.State.expand, EVM.State.readWord, "
    "EVM.State.readBytes, EVM.State.wordBytes, EVM.State.writeBytes, EVM.State.pureOp, "
    "EVM.State.pushValue, EVM.State.normalizedSlots, tvWorldConfig, "
    "EVM.step, Evm.add, Evm.sub, Evm.mul, Evm.eq, Evm.iszero, "
    "Storage.readPrice, Storage.writePrice, Storage.netMetered, Fork.atLeast, Fork.ordinal, "
    "Except.bind, Except.map, Except.pure, Bind.bind, Pure.pure, Functor.map, EVM.State.finalize"
)


# Typed SSA boxing introduces dependent width casts. Unfold their representation
# so address arithmetic reduces without unfolding symbolic byte memory.
MIR_UNFOLD = (
    "before, after, mirWorldConfig, MIR.State.equivalent, MIR.State.observe, "
    "MIR.State.execute, MIR.State.reduceRun, MIR.State.transition, MIR.State.next, MIR.State.stop, MIR.State.enter, MIR.State.function, MIR.State.start, "
    "MIR.State.typedWords, MIR.State.callSignature, MIR.State.branch, MIR.State.advance, "
    "MIR.State.instruction, MIR.State.operation, MIR.State.storageOperation, MIR.State.assign, "
    "MIR.State.read, MIR.State.readValue, MIR.State.Value.word, MIR.State.Value.fits, MIR.State.Value.flatten, MIR.State.ValueType.same, MIR.State.sameTypes, MIR.State.ValueType.width, MIR.State.ValueType.fields, MIR.State.assignValue, MIR.State.insertField, MIR.State.extractField, MIR.State.Scalar.word, MIR.State.Scalar.ofWord, MIR.State.wordBits, MIR.State.widenBits, MIR.State.required, MIR.State.put, MIR.State.narrow, MIR.State.value, MIR.State.effect,  "
    "BitVec.cast, BitVec.toNat_cast, BitVec.cast_eq, BitVec.cast_cast, MIR.State.finish, MIR.eval, MIR.compare, MIR.wordBinary, MIR.unary, MIR.shift, "
    "MIR.State.allocate, MIR.State.freePointer, MIR.State.memoryOperation, "
    "MIR.State.abiEncode, MIR.State.abiWriteWords, MIR.State.abiDecode, MIR.State.abiCanonical, "
    "Execution.expand, Execution.requiredWords, Execution.memoryWords, Execution.reduceClosedRead, Execution.readWord, "
    "Execution.readBytes, Execution.wordBytes, Execution.writeBytes, "
    "Execution.account, Execution.putAccount, Execution.warmSlot, Execution.warmAddress, "
    "Execution.slotsRead, Execution.slotsWrite, Execution.normalizedSlots, "
    "Execution.ordered, Execution.orderedInsert, Except.bind, Except.map, Except.pure, "
    "Bind.bind, Pure.pure, Functor.map, List.mapM.loop, List.mapM, List.foldl, List.getElem?_cons, List.find?_cons"
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
    if a.world.get("direct_mir"):
        for group in ("params", "returns"):
            left = a.world["boundary"][group]
            right = b.world["boundary"][group]
            for old, new in zip(left, right, strict=True):
                if (
                    old.get("kind") == new.get("kind") == "pointer"
                    and old["location"] != new["location"]
                ):
                    raise Unsupported("MIR boundary pointer address spaces differ")
    build = build_model(timeout)
    if build.returncode:
        raise Unsupported(build.stderr or build.stdout)
    policy = "effects" if a.gas is None else a.gas
    direct_mir = a.world.get("direct_mir", False)
    declarations = (
        PRELUDE
        + "set_option maxRecDepth 16384\nset_option maxHeartbeats 2000000\n"
        + a.world["configuration"]
        + definition("before", a)
        + definition("after", b)
    )
    arguments = " ".join(a.inputs)
    widths = a.input_widths or (256,) * len(a.inputs)
    binders = " ".join(
        f"({name} : BitVec {width})"
        for name, width in zip(a.inputs, widths, strict=True)
    )

    def relation(left, right):
        if direct_mir:
            return f"MIR.State.equivalent ({left}) ({right})"
        return f"EVM.State.equivalent tvWorldConfig tvWorldConfig .{policy} ({left}) ({right})"

    def describe(term):
        return (
            f"MIR.State.describe ({term})"
            if direct_mir
            else f"EVM.State.describe tvWorldConfig .{policy} ({term})"
        )

    statement = f"theorem pair_correct {binders} : {relation('before ' + arguments, 'after ' + arguments)} := by\n"
    # Closed executions use kernel evaluation directly. Trying definitional
    # equality first duplicates hash evaluation and is much slower for creation.
    unfold = MIR_UNFOLD if direct_mir else UNFOLD
    if not a.inputs:
        proof = "  decide +kernel\n"
    elif direct_mir:
        # Normalize each execution before comparing it. Keep byte memory abstract
        # for the range-proved read/write laws; expanding every symbolic byte
        # before reading a disjoint FMP word loses that useful structure.
        abstract = MIR_UNFOLD
        for name in (
            "before",
            "after",
            "MIR.State.equivalent",
            "MIR.State.observe",
            "Execution.readWord",
            "Execution.readBytes",
            "Execution.wordBytes",
            "Execution.writeBytes",
        ):
            abstract = abstract.replace(name + ", ", "")
        # Compare observation fields before simplifying their contents. Identical
        # account/access sets need no repeated normalization under symbolic bytes.
        # Splitting a guard can expose a continuation, so normalize again there.
        finish = f"<;> (repeat (first | split | intro _)); all_goals simp_all [{abstract}, MIR.State.equivalent, MIR.State.observe, Nat.add_comm] <;> first | rfl | omega"
        proof = (
            "  unfold before after\n"
            f"  conv => arg 1; simp [{abstract}]\n"
            f"  conv => arg 2; simp [{abstract}]\n"
            f"  simp only [MIR.State.equivalent, MIR.State.observe, MIR.State.Observation.mk.injEq, and_true, true_and, eq_self] {finish}\n"
        )
    else:
        proof = (
            "  first\n    | rfl\n    | decide +kernel\n"
            f"    | (simp [{unfold}] <;> (repeat (first | split | intro _)); all_goals simp_all <;> omega)\n"
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

        # VM evaluation only proposes a witness; it never supplies proof evidence.
        # Probe zero first even for symbolic inputs, rather than spending the full
        # budget attempting a false universal theorem. A matching sample still
        # requires the quantified proof below. Mismatches need kernel and native replay.
        concrete_zero = " ".join(f"0#{width}" for width in widths)
        probe = lean(
            f'#eval IO.println (if decide ({relation("before " + concrete_zero, "after " + concrete_zero)}) then "equivalent" else "different")\n'
        )
        lines = probe.stdout.strip().splitlines()
        if (
            probe.returncode
            or not lines
            or lines[-1] not in {"equivalent", "different"}
        ):
            raise Inconclusive(
                "invalid concrete-pair evaluation\n" + probe.stdout + probe.stderr
            )
        different = lines[-1] == "different"
        result = (
            None
            if different
            else lean(statement + proof + "#print axioms pair_correct\n")
        )
        log = (
            "VM proposes a mismatch; kernel witness required"
            if result is None
            else result.stdout + result.stderr
        )
        verified = result is not None and result.returncode == 0
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
                [
                    [value % 2**width for width in widths]
                    for value in (1, 2**255, 2**256 - 1)
                ]
            )
        for values in patterns:
            concrete = " ".join(
                f"{value}#{width}" for value, width in zip(values, widths, strict=True)
            )
            left, right = "before " + concrete, "after " + concrete
            witness = (
                ""
                if verified
                else f"example : ¬ ({relation(left, right)}) := by decide +kernel\n"
            )
            observations = lean(
                witness
                + f"#eval IO.println ({describe(left)})\n"
                + f"#eval IO.println ({describe(right)})\n"
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
