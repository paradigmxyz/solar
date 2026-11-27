"""Validate the full indexed and pointer variants of the variable word fill."""

import hashlib
import json
import tempfile
from itertools import zip_longest
from pathlib import Path

from . import (
    audit,
    builtin_lowering,
    dead_ssa,
    entry_proof,
    memory_induction,
    mir_state,
    native,
    report,
    world,
)
from .common import Program, Unsupported

MODULE = "SolarTV.MIR.Transforms.BoundedFill"


def check(paths, solar, executable, version, timeout, function, *, replay, verbose):
    from .check import build_model

    texts, models, declarations = builtin_lowering.load_pair(
        paths, solar, executable, version, timeout
    )
    if any(
        function not in decls
        or decls[function]["text"][0] != f"fn @{function}() {{"
        or decls[function]["targets"]
        for decls in declarations
    ):
        raise Unsupported("variable word fill requires a raw call-free body")
    try:
        bodies = [
            builtin_lowering.normalize(builtin_lowering.functions(model)[function])
            for model in models
        ]
    except KeyError as error:
        raise Unsupported("variable word fill needs an exported body") from error
    if any(len(body["blocks"]) != 19 for body in bodies):
        raise Unsupported("no checked-allocation word-fill shape in supplied pair")
    catalog = {row["name"]: row for row in native.catalog(executable, timeout)["evmir"]}
    for body in bodies:
        for block in body["blocks"]:
            for inst in block["instructions"]:
                op = inst["operation"]
                if (
                    op["kind"] == "opcode"
                    and not catalog[op["name"]]["forks"][version]["available"]
                ):
                    raise Unsupported(
                        "variable word fill uses an unavailable operation"
                    )
    before, after = (mir_state.function_body(body, catalog) for body in bodies)
    name = json.dumps(function)
    source = f"""import {MODULE}
open SolarTV EvmRules MIR.BoundedFill
set_option maxRecDepth 8192
def before : Nat × MIR.State.Function := {before}
def after : Nat × MIR.State.Function := {after}
theorem certificate :
    before.2 = indexedBody {name} ∧ after.2 = pointerBody {name} ∧
    ∀ (State Event Failure Result : Type) (handlers : Handlers State Event Failure Result)
      (size base length count : Word) (fuel : Nat) (state : State),
      (execute false size base length count handlers fuel state =
        execute true size base length count handlers fuel state) ∧
      (execute false size base length count handlers
        (min length.toNat count.toNat+1) state).2.isSome = true :=
  ⟨rfl,rfl,fun _ _ _ _ handlers size base length count fuel state =>
    ⟨correspondence size base length count handlers fuel state,
      completes size base length count handlers state⟩⟩
#print axioms certificate
"""
    entry_proof.success(build_model(timeout, MODULE), MODULE)
    with tempfile.TemporaryDirectory(prefix="solar-tv-bounded-fill-") as folder:
        folder = Path(folder)
        proof = entry_proof.lean(folder, "Pair", source, timeout)
        if proof.returncode:
            return replay_bad_stride(
                folder,
                texts,
                solar,
                executable,
                version,
                timeout,
                function,
                replay,
                verbose,
            )
        entry_proof.check_axioms(proof.stdout.splitlines(), 1)
    pair = tuple(
        Program(
            (function, "bounded-fill"),
            [],
            "",
            "",
            canonical_ir=text,
            replay_ir=text,
            selector=function,
            scope="bounded-fill",
            evm_version=version,
            import_evidence={
                "kind": "kernel-checked-runtime-bounded-loop",
                "source_sha256": hashlib.sha256(
                    dead_ssa.stable(body).encode()
                ).hexdigest(),
                "boundary": (
                    "Unpriced typed CFG projection from ABI entry through publication. "
                    "Equal heap placement and arbitrary shared state. All logical iteration "
                    "prefixes, ordered memory effects and terminal results."
                ),
            },
        )
        for text, body in zip(texts, bodies, strict=True)
    )
    return audit.Result("verified", "verified\n", program=pair[0], pair=pair)


def witness_world(
    folder, solar, *, function="fillWords", length=3, count=5, base=128, calldata=None
):
    if calldata is None:
        calldata = "0xa534f1f0" + f"{length:064x}{count:064x}"
    path = folder / "word-fill-world.json"
    fixture = memory_induction.witness_world(function, base)
    path.write_text(json.dumps({**fixture, "fuel": 1024, "calldata": calldata}))
    return world.read(path, solar, None, kind="mir")


def replay_bad_stride(
    folder, texts, solar, executable, version, timeout, function, replay, verbose
):
    from . import traced_replay

    fixture = witness_world(folder, solar, function=function)
    pair, observations, native_observations, traces = traced_replay.trace_pair(
        folder,
        texts,
        fixture,
        solar,
        executable,
        version,
        timeout,
        function,
        stores=True,
    )
    if observations[0] == observations[1]:
        raise Unsupported(
            "variable word fill does not match the typed loop certificate"
        )
    first = next(
        (
            (index, old, new)
            for index, (old, new) in enumerate(zip_longest(*traces))
            if old != new
        ),
        None,
    )
    if first is None:
        raise Unsupported("variable word fill has no independently divergent store")
    index, before, after = first
    return audit.Result(
        "counterexample",
        report.counterexample(
            *pair,
            [],
            *(json.dumps(value) for value in observations),
            replay,
            verbose=verbose,
            native_checked=True,
        )
        + f"First differing MSTORE (#{index + 1}): {before} vs {after}\n",
        program=pair[0],
        pair=pair,
        witness={
            "inputs": {},
            "calldata": "0x" + bytes(fixture["calldata"]).hex(),
            "observations": observations,
            "native_observations": native_observations,
            "first_different_write": {"index": index, "before": before, "after": after},
        },
    )
