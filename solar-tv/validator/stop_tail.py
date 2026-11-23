"""Prove a deleted POP suffix cannot underflow before an unchanged STOP."""

import hashlib
import json
import tempfile
from collections import deque
from pathlib import Path

from . import (
    audit,
    cfg_flow,
    continuation_flow,
    correspondence,
    entry_proof,
    regions,
    stack_layout,
)
from .common import Program, Unsupported


def lower_bounds(projected, signatures):
    """Propose a conservative bound for every reachable provenance point."""
    bounds = [1025] * len(projected["nodes"])
    bounds[0] = 0
    pending = deque([0])
    updates = 0
    while pending:
        pc = pending.popleft()
        floor = bounds[pc]
        node = projected["nodes"][pc]["node"]
        match node["kind"]:
            case "operation":
                inputs, outputs = signatures[node["operation"]]
                edges = [(node["next"], max(floor, inputs) - inputs + outputs)]
            case "jump":
                edges = [(node["next"], floor)]
            case "branch":
                edges = [(node[edge], max(floor, 1) - 1) for edge in ("yes", "no")]
            case _:
                edges = []
        for target, incoming in edges:
            if incoming < bounds[target]:
                bounds[target] = incoming
                pending.append(target)
                updates += 1
                if updates > 65536:
                    raise Unsupported("stack floor analysis exceeds 65536 updates")
    return bounds


def source_data(text, block, count, replay, catalog, version, timeout):
    source = correspondence.export(text, None, version, replay, timeout, labels=True)
    local = [row for row in source["nodes"] if row["label"] == int(block[2:])]
    if len(local) <= count:
        raise Unsupported("terminal suffix is missing from the physical source graph")
    cut = local[-count - 1]["id"]
    projected, witness = continuation_flow.project(
        source, catalog, version, interactions=True, from_entry=True
    )
    signatures = stack_layout.signatures(projected, catalog)
    floors = lower_bounds(projected, signatures)
    incoming = [
        floor
        for (pc, _), floor in zip(witness["point_data"], floors, strict=True)
        if pc == cut
    ]
    if not incoming or min(incoming) < count or min(incoming) > 1024:
        raise Unsupported(
            "caller paths do not establish enough stack words for the deleted POPs"
        )
    return witness, signatures, floors, cut, min(incoming)


def proof_stages(witness, signatures, floors, cut, count, catalog):
    shapes = {}
    points = []
    for pc, shape in witness["point_data"]:
        key = shapes.setdefault(shape, len(shapes))
        points.append(f"({pc},{key})")
    tables = (
        cfg_flow.table(
            "rootNodes", "EVM.ContinuationFlow.Node", witness["source"], lookup=False
        )
        + cfg_flow.table("rootPoints", "(Nat × Nat)", points, lookup=False)
        + cfg_flow.table(
            "rootShapes",
            "EVM.ContinuationFlow.Shape",
            [continuation_flow.shape_term(shape) for shape in shapes],
            lookup=False,
        )
        + cfg_flow.table(
            "rootEdges", "EVM.ControlFlow.Edges", witness["edges"], lookup=False
        )
        + cfg_flow.table("floorRows", "Nat", list(map(str, floors)), lookup=False)
        + cfg_flow.signatures_declaration(signatures, catalog)
        + f"""
def points := EVM.ContinuationFlow.internedPoints rootPoints.view rootShapes.view
def graph := EVM.StackReachability.project rootNodes.view points rootEdges.view
def floor (i : Nat) := (floorRows.get? i).getD 0
def certificateAt (i : Nat) :=
  EVM.StackReachability.floorAt graph signatures floor i &&
    EVM.StopTail.floorAtCut points floor {cut} {count} i
"""
    )
    checks, certificates = [], []
    # Release Lean's state between groups for large physical graphs. The
    # linked range proof still covers every row of the original projection.
    for start in range(0, len(points), 2048):
        name = f"floorRange{start}"
        checks.append(
            cfg_flow.check_rows(
                name, "certificateAt", min(2048, len(points) - start), start=start
            )
        )
        certificates.append(name)
    joined = certificates[0]
    for part in certificates[1:]:
        joined = f"(EVM.StackReachability.joinRanges {joined} {part})"
    theorem = f"""
open EVM.StackReachability
theorem certificate : CheckedRange certificateAt 0 {len(points)} := {joined}
theorem validated (ctx : EVM.Context) (stack : EVM.Stack)
    {{pc : Nat}} {{point : EVM.ContinuationFlow.Point}}
    (reaches : Reaches graph signatures pc stack.length)
    (pointEq : points.get? pc = some point) (atCut : point.pc = {cut}) :
    EVM.execute ctx (List.replicate {count} .pop) .stop stack =
      EVM.execute ctx [] .stop stack := by
  refine EVM.StopTail.from_entry (source := {witness["entry"]}) (floors := floor) (by decide +kernel)
    (by decide +kernel) (by decide +kernel) ?_ ?_ (by decide +kernel)
    ctx stack reaches pointEq atCut
  · intro i h
    have both := certificate i (Nat.zero_le i) h
    rw [certificateAt, Bool.and_eq_true] at both
    exact both.1
  · intro i h
    have both := certificate i (Nat.zero_le i) h
    rw [certificateAt, Bool.and_eq_true] at both
    exact both.2
#print axioms validated
"""
    return [tables, *checks, theorem]


def try_check(texts, models, entry, catalog, version, replay, timeout):
    if any(model["terminal"] != "Op(0)" for model in models):
        return None
    left, right = [model["instructions"] for model in models]
    count = len(left) - len(right)
    if (
        count <= 0
        or left[: len(right)] != right
        or left[len(right) :] != ["pop"] * count
    ):
        return None
    before, after = map(regions.blocks, texts)
    if any(
        before.get(name) != after.get(name)
        for name in before.keys() | after.keys()
        if name != entry
    ):
        return None

    from .check import PRELUDE, build_model

    witness, signatures, floors, cut, lower = source_data(
        texts[0], entry, count, replay, catalog, version, timeout
    )
    prelude = (
        PRELUDE
        + """
noncomputable section
set_option maxRecDepth 65536
set_option maxHeartbeats 4000000
set_option Elab.async false
"""
    )
    built = build_model(timeout)
    if built.returncode:
        raise Unsupported(built.stdout + built.stderr)
    with tempfile.TemporaryDirectory(prefix="solar-tv-stop-tail-") as folder:
        result = cfg_flow.check_stages(
            Path(folder),
            prelude,
            proof_stages(witness, signatures, floors, cut, count, catalog),
            timeout,
        )
    if result.returncode:
        raise Unsupported(
            "terminal suffix certificate did not close\n"
            + result.stdout
            + result.stderr
        )
    entry_proof.check_axioms(
        [line for line in result.stdout.splitlines() if line.startswith("'validated'")],
        1,
    )
    pair = tuple(
        Program(
            ("evm-stop-suffix", entry),
            [],
            "",
            "",
            canonical_ir=text,
            replay_ir=text,
            selector=entry,
            scope="stack-terminal-stop",
            evm_version=version,
            import_evidence={
                "kind": "independent-terminal-suffix",
                "removed_pops": count,
                "incoming_minimum": lower,
                "source_cut": cut,
                "caller_source_sha256": witness["evidence"]["source_sha256"],
                "window_sha256": hashlib.sha256(
                    json.dumps(model, sort_keys=True).encode()
                ).hexdigest(),
                "boundary": "unpriced STOP suffix; imported EVM control and opcode signatures; unchanged code before the cut under shared context",
            },
        )
        for text, model in zip(texts, models, strict=True)
    )
    return audit.Result("verified", "verified\n", program=pair[0], pair=pair)
