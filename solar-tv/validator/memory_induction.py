"""Check the emitted three-loop memory fill in the direct MIR interpreter."""

import hashlib
import json
import tempfile
from pathlib import Path

from . import audit, builtin_lowering, dead_ssa, entry_proof, mir_state, native, world
from .common import Program, Unsupported

MODULE = "SolarTV.MIR.Transforms.ContinuedCounter"


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
        or decls[function]["targets"]
        for table, decls in zip(tables, declarations, strict=True)
    ):
        raise Unsupported("memory fill requires a raw call-free function")
    bodies = [table[function] for table in tables]
    # The current proof handles the two copies of each loop and the split
    # pointer-handoff edge. The imported bodies are executed in full in Lean.
    if [len(body["blocks"]) for body in bodies] != [30, 31]:
        raise Unsupported("not a three-loop memory fill")
    catalog = {row["name"]: row for row in native.catalog(executable, timeout)["evmir"]}
    for body in bodies:
        for block in body["blocks"]:
            for inst in block["instructions"]:
                op = inst["operation"]
                if op["kind"] == "opcode":
                    fork = catalog[op["name"]]["forks"][version]
                    if not fork["available"]:
                        raise Unsupported("memory fill uses an unavailable operation")
    before, after = (
        mir_state.function_body({**body, "id": 0}, catalog) for body in bodies
    )
    name = json.dumps(function)
    source = f"""import {MODULE}
open SolarTV EvmRules MIR.State MIR.ContinuedCounter
set_option maxRecDepth 16384
set_option maxHeartbeats 3000000
set_option linter.unusedSimpArgs false
def before : Functions := [{before}]
def after : Functions := [{after}]
theorem beforeStores : Certificate .{version} {name} before := by
  prove_word_fill before
theorem afterStores : Certificate .{version} {name} after := by
  prove_word_fill after
theorem contextual : PairCertificate .{version} {name} before after :=
  correspondence _ _ _ _ beforeStores afterStores
#print axioms beforeStores
#print axioms afterStores
#print axioms contextual
"""
    entry_proof.success(build_model(timeout, MODULE), MODULE)
    with tempfile.TemporaryDirectory(prefix="solar-tv-memory-induction-") as folder:
        proof = entry_proof.lean(Path(folder), "Pair", source, timeout)
        if proof.returncode:
            return replay_handoff(
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
        entry_proof.check_axioms(proof.stdout.splitlines(), 3)
    pair = tuple(
        Program(
            (function, "memory-induction"),
            [],
            "",
            "",
            canonical_ir=text,
            replay_ir=text,
            selector=function,
            scope="memory-induction",
            evm_version=version,
            import_evidence={
                "kind": "kernel-checked-memory-fill",
                "source_sha256": hashlib.sha256(
                    dead_ssa.stable(body).encode()
                ).hexdigest(),
                "boundary": (
                    "direct MIR execution at an aligned heap floor at least 128, "
                    "with room for 224 bytes in the 64 KiB memory model; "
                    "exact ordered MSTORE addresses and words, full result and memory"
                ),
            },
        )
        for text, body in zip(texts, bodies, strict=True)
    )
    return audit.Result("verified", "verified\n", program=pair[0], pair=pair)


def witness_world(function, base=128, *, dirty=False):
    memory = bytearray([0xA5] * (base + 256) if dirty else bytes(96))
    memory[64:96] = base.to_bytes(32, "big")
    return {
        "version": 1,
        "fuel": 256,
        "memory": {"data": "0x" + memory.hex()},
        "mir_layout": {"functions": {function: {"heap_floor": base}}},
    }


def replay_handoff(
    folder, texts, solar, executable, version, timeout, function, replay, verbose
):
    from . import world_check

    path = folder / "handoff.json"
    fixture = witness_world(function)
    path.write_text(json.dumps(fixture))
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
        raise Unsupported("memory fill certificate did not close")
    result.witness["heap_floor"] = 128
    return result
