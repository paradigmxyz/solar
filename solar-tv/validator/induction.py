"""Validate an index-to-pointer scan by binding the full typed SSA pair."""

import hashlib
import json
import tempfile
from pathlib import Path

from . import audit, builtin_lowering, dead_ssa, entry_proof, mir_state, native, world
from .common import Program, Unsupported

MODULE = "SolarTV.MIR.Transforms.CalldataInduction"


def stride_of(body):
    # A cheap eligibility filter for the cyclic fallback. The complete pair,
    # including this operand, is checked against the typed templates in Lean.
    blocks = body["blocks"]
    if len(blocks) != 9 or len(blocks[5]["phis"]) != 1:
        raise Unsupported("not a single-phi calldata scan")
    update = blocks[8]["instructions"]
    if len(update) != 1:
        raise Unsupported("not a single calldata pointer update")
    op = update[0]["operation"]
    if (
        op["kind"] != "opcode"
        or op["name"] != "add"
        or len(op["args"]) != 2
        or op["args"][1]["kind"] != "constant"
    ):
        raise Unsupported("no constant calldata stride")
    stride = int(op["args"][1]["value"])
    if not 0 <= stride < 2**256:
        raise Unsupported("calldata stride is not a word")
    return stride


def check(paths, solar, executable, version, timeout, function, *, replay, verbose):
    from .check import build_model

    texts, models, declarations = builtin_lowering.load_pair(
        paths, solar, executable, version, timeout
    )
    tables = list(map(builtin_lowering.functions, models))
    if any(
        function not in table
        or function not in decls
        or decls[function]["text"][0] != f"fn @{function}() {{"
        for table, decls in zip(tables, declarations, strict=True)
    ):
        raise Unsupported("calldata scan requires a raw function with no arguments")
    bodies = [builtin_lowering.normalize(table[function]) for table in tables]
    stride = stride_of(bodies[1])
    catalog = {row["name"]: row for row in native.catalog(executable, timeout)["evmir"]}
    for body in bodies:
        for block in body["blocks"]:
            for inst in block["instructions"]:
                op = inst["operation"]
                if (
                    op["kind"] == "opcode"
                    and not catalog[op["name"]]["forks"][version]["available"]
                ):
                    raise Unsupported("calldata scan uses an unavailable operation")
    before, after = (mir_state.function_body(body, catalog) for body in bodies)
    source = f"""import {MODULE}
open SolarTV EvmRules MIR.CalldataInduction
set_option maxRecDepth 8192
set_option maxHeartbeats 1000000
def before : Nat × MIR.State.Function := {before}
def after : Nat × MIR.State.Function := {after}
theorem captured :
    before.2 = indexBody {json.dumps(function)} ∧
    after.2 = pointerBody {json.dumps(function)} {stride} := ⟨rfl,rfl⟩
#print axioms captured
"""
    if stride == 32:
        source += """
theorem contextual (input : List Execution.Byte) (frame : MIR.State.Frame) (fuel : Nat) :
    executeScan false input frame fuel = executeScan true input frame fuel :=
  scanEquivalence input frame fuel
#print axioms contextual
#print axioms admittedScan
"""
    entry_proof.success(build_model(timeout, MODULE), MODULE)
    with tempfile.TemporaryDirectory(prefix="solar-tv-induction-") as folder:
        proof = entry_proof.lean(Path(folder), "Pair", source, timeout)
        output = entry_proof.success(proof, "calldata induction pair")
        entry_proof.check_axioms(output.splitlines(), 3 if stride == 32 else 1)
        if stride != 32:
            return replay_stride(
                Path(folder),
                texts,
                solar,
                executable,
                version,
                timeout,
                function,
                replay,
                verbose,
            )

    pair = tuple(
        Program(
            (function, "calldata-induction"),
            [],
            "",
            "",
            canonical_ir=text,
            replay_ir=text,
            selector=function,
            scope="calldata-induction",
            evm_version=version,
            import_evidence={
                "kind": "kernel-checked-calldata-induction",
                "source_sha256": hashlib.sha256(
                    dead_ssa.stable(body).encode()
                ).hexdigest(),
                "boundary": (
                    "exact typed templates and unpriced loop projection; "
                    "arbitrary calldata and scan budgets with a termination bound; "
                    "trusted parser/schema export and typed CFG projection"
                ),
            },
        )
        for text, body in zip(texts, bodies, strict=True)
    )
    return audit.Result("verified", "verified\n", program=pair[0], pair=pair)


def replay_stride(
    folder, texts, solar, executable, version, timeout, function, replay, verbose
):
    from . import world_check

    # Two elements distinguish a doubled stride from a one-word stride while
    # letting both executions terminate. Equal observations prove nothing here.
    calldata = "0x00000000" + "".join(f"{word:064x}" for word in (64, 5, 2, 4, 5))
    path = folder / "stride.json"
    path.write_text(json.dumps({"version": 1, "calldata": calldata, "fuel": 128}))
    fixture = world.read(path, solar, None, kind="mir")
    programs = [
        mir_state.translate(
            text, solar, version, fixture, timeout, function, replay=executable
        )
        for text in texts
    ]
    result = world_check.check(
        *programs,
        timeout,
        replay_command=replay,
        replay_executable=executable,
        verbose=verbose,
    )
    if result.status != "counterexample":
        raise Unsupported(
            "changed stride has no replayed counterexample for this input"
        )
    result.witness["calldata"] = calldata
    return result
