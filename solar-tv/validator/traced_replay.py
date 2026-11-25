"""Ordered MIR effects independently replayed and checked in the kernel."""

import json
from dataclasses import replace

from . import entry_proof, mir_state, native
from .common import Unsupported


def trace_pair(
    folder, texts, fixture, solar, executable, version, timeout, function, *, stores
):
    """Certify the native trace against the corresponding Lean interpreter."""
    from .check import build_model, definition

    if stores:
        module, execute, event, key = (
            "StoreTrace",
            "executeStores",
            "Store",
            "store_trace",
        )
    else:
        module, execute, event, key = (
            "StorageTrace",
            "executeStorage",
            "StorageAccess",
            "storage_trace",
        )
    module = f"SolarTV.MIR.State.{module}"
    entry_proof.success(build_model(timeout, module), module)
    programs = tuple(
        mir_state.translate(
            text, solar, version, fixture, timeout, function, replay=executable
        )
        for text in texts
    )
    result = native.replay(
        *programs,
        [],
        executable=executable,
        timeout=timeout,
        trace_storage=not stores,
        trace_stores=stores,
    )
    observations = [json.loads(item) for item in result["observations"]]
    traces = [item.pop(key) for item in observations]
    source = f"""import {module}
import SolarTV.MIR.State.Observe
open SolarTV EvmRules
set_option maxRecDepth 16384
set_option maxHeartbeats 4000000
{programs[0].world["configuration"]}
"""
    # Check each payload with its trace. Reducing two full executions inside
    # one observation comparison can retain both kernel memory graphs.
    payload_differs = observations[0]["data"] != observations[1]["data"]
    for name, program, trace, observation in zip(
        ("before", "after"), programs, traces, observations, strict=True
    ):
        traced = replace(
            program,
            body=program.body.replace("MIR.State.execute ", f"MIR.State.{execute} "),
            result_type=f"Except String (List MIR.State.{event} × MIR.State.Result)",
        )
        events = ",".join(
            "(" + ",".join(str(int(word)) for word in row) + ")" for row in trace
        )
        source += definition(name, traced)
        projection = "Prod.fst"
        expected = f"([{events}] : List MIR.State.{event})"
        if payload_differs:
            projection = "(fun pair => (pair.1,(MIR.State.observe pair.2).data))"
            expected = f"({expected},({observation['data']} : List Execution.Byte))"
        source += f"""theorem {name}Trace :
    ({name}).toOption.map {projection} = some {expected} := by
  decide +kernel
#print axioms {name}Trace
#eval IO.println (MIR.State.describe (({name}).map Prod.snd))
"""
    needs_difference_proof = not payload_differs and observations[0] != observations[1]
    if needs_difference_proof:
        source += """theorem different :
    ¬ MIR.State.equivalent (before.map Prod.snd) (after.map Prod.snd) := by
  decide +kernel
#print axioms different
"""
    proof = entry_proof.lean(folder, "Replay", source, timeout)
    lines = entry_proof.success(proof, "MIR trace replay").splitlines()
    entry_proof.check_axioms(lines[::2], 3 if needs_difference_proof else 2)
    lean_observations = [json.loads(item) for item in lines[1::2]]
    if lean_observations != observations:
        raise Unsupported("native and Lean MIR replay observations differ")
    return programs, lean_observations, observations, traces
