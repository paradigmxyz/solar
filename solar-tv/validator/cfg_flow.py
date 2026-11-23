"""Prove jump-chain contraction and block coalescing in exported EVM graphs."""

import hashlib
import json
import re
import tempfile
from itertools import batched
from pathlib import Path

from . import audit, regions, stack_layout
from .common import Program, Unsupported


def projections(models):
    """Propose a reachable pairing. Lean checks all paths and edges separately."""
    indices = {}
    points = []
    edges = []
    projections = [{"entry": 0, "nodes": []}, {"entry": 0, "nodes": []}]

    def index(pair):
        # Index pairs, so one source point can match several on the other side.
        if pair not in indices:
            # Two projected nodes per pair, within the 4096-node graph limit.
            if len(points) >= 2048:
                raise Unsupported("paired control-flow graph exceeds 2048 points")
            indices[pair] = len(points)
            points.append(pair)
        return indices[pair]

    def follow(model, position):
        growth = hops = 0
        while hops < len(model["nodes"]):
            node = model["nodes"][position]["node"]
            if node["kind"] != "jump":
                return growth, hops, dict(node)
            position = node["next"]
            growth = max(growth, node["growth"])
            hops += 1
        raise Unsupported("jump-only cycle has no control-flow projection")

    index(tuple(model["entry"] for model in models))
    jump_limit = 1
    for position, pair in enumerate(points):
        left, right = [follow(m, pc) for m, pc in zip(models, pair, strict=True)]
        jump_limit = max(jump_limit, left[1] + 1, right[1] + 1)
        a, b = left[2], right[2]
        fields = tuple(field for field in ("next", "yes", "no") if field in a)
        if {k: v for k, v in a.items() if k not in fields} != {
            k: v for k, v in b.items() if k not in fields
        }:
            raise Unsupported("control-flow projection changed an action or terminal")
        successors = [index((a[field], b[field])) for field in fields]
        match successors:
            case [target]:
                edges.append(f".next {target}")
            case [yes, no]:
                edges.append(f".branch {yes} {no}")
            case []:
                edges.append(".terminal")
        for field, successor in zip(fields, successors, strict=True):
            a[field] = b[field] = 2 * successor
        for model, (growth, _, node) in zip(projections, (left, right), strict=True):
            offset = 2 * position
            model["nodes"].extend(
                (
                    {
                        "id": offset,
                        "label": 0,
                        "node": {
                            "kind": "jump",
                            "growth": growth,
                            "next": offset + 1,
                        },
                    },
                    {"id": offset + 1, "label": 0, "node": node},
                )
            )
    left, right = zip(*points, strict=True)
    return (list(left), list(right)), projections, edges, jump_limit


def projection_proof(theorem, module, arguments, points, rows, result):
    """Check fixed ranges independently and compose the full projection in Lean."""
    code, ranges = [], []
    proof = ""
    start = 0
    for part, group in enumerate(batched(rows, 12)):
        interval = f"List.range' {start} {len(group)}"
        name = f"{theorem}Part{part}"
        code.append(
            f"theorem {name} : {module}.projectRows {arguments} ({interval}) =\n"
            f"    some [{','.join(group)}] := by decide +kernel\n"
        )
        ranges.append(f"({interval})")
        proof = (
            f"({module}.append_projectRows {arguments} {proof} {name})"
            if proof
            else name
        )
        start += len(group)
    return (
        "".join(code)
        + f"""
theorem {theorem} : {module}.project {arguments} = some {result} := by
  have range : List.range {points}.size = {" ++ ".join(ranges)} := by rfl
  exact {module}.project_result {arguments} 0 _ {result}
    (by rfl) (by rfl) (by rw [range]; exact {proof}) (by rfl)
"""
    )


def layout_proof(size):
    code, ranges = [], []
    proof = ""
    for part, start in enumerate(range(0, size, 16)):
        interval = f"List.range' {start} {min(16, size - start)}"
        name = f"layoutPart{part}"
        code.append(
            f"theorem {name} : ({interval}).all (checkIndex old new id bounds signatures) = true :=\n"
            "  by decide +kernel\n"
        )
        ranges.append(f"({interval})")
        proof = f"(append_checks {proof} {name})" if proof else name
    return (
        "".join(code)
        + f"""
theorem valid : Valid old new id bounds signatures := by
  have range : List.range old.nodes.size = {" ++ ".join(ranges)} := by rfl
  exact valid_checked old new id bounds signatures 1024
    (by decide +kernel) (by rfl) (by decide +kernel) (by rw [range]; exact {proof})
"""
    )


def check(
    texts,
    models,
    expressions,
    entry,
    catalog,
    version,
    timeout,
    *,
    continuations=None,
    interactions=False,
):
    from .check import PRELUDE, ROOT, build_model, run
    from .source_identity import render_nodes

    points, normalized, edges, jump_limit = projections(models)
    table = stack_layout.signatures(normalized[0], catalog)
    _, bounds, changed = stack_layout.certificate(*normalized, table)
    signatures = ",".join(
        f"({json.dumps(name)},({inputs},{outputs}))"
        for name, (inputs, outputs) in table.items()
    )
    provenance = ""
    provenance_theorems = []
    contextual = ""
    if continuations is not None:
        for lemma in (
            "transfer_tags",
            "checked_operands",
            "swap_encoding",
            "return_word",
        ):
            theorem = f"SolarTV.EVM.ContinuationFlow.{lemma}"
            provenance += f"#print axioms {theorem}\n"
            provenance_theorems.append(theorem)
        for name, model, (_, witness) in zip(
            ("before", "after"), models, continuations, strict=True
        ):
            theorem = f"{name}Provenance"
            provenance_theorems.append(theorem)
            provenance += f"""
def {name}Source : EVM.ContinuationFlow.Program := {witness["source"]}
def {name}Points : Array EVM.ContinuationFlow.Point := {witness["points"]}
def {name}Edges : Array Edges := {witness["edges"]}
"""
            _, rows = render_nodes(
                model, catalog, version, "bb0", interactions=interactions
            )
            provenance += projection_proof(
                theorem,
                "EVM.ContinuationFlow",
                f"{name}Source {name}Points {name}Edges",
                f"{name}Points",
                rows,
                name,
            )
            provenance += f"#print axioms {theorem}\n"
        provenance_theorems.append("matchedContinuations")
        contextual = f"""
theorem matchedContinuations {{World Observation : Type}} (handlers : Handlers World Observation)
    (work : Nat) (state : State World) :
    ((EVM.ContinuationFlow.project beforeSource beforePoints beforeEdges).bind
        (fun p => project p sources edges {jump_limit})).map
        (fun p => execute p signatures handlers work p.entry state) =
      ((EVM.ContinuationFlow.project afterSource afterPoints afterEdges).bind
        (fun p => project p targets edges {jump_limit})).map
        (fun p => execute p signatures handlers work p.entry state) :=
  EVM.ControlFlow.compose beforeProvenance afterProvenance
    (fun p => project p sources edges {jump_limit}) (fun p => project p targets edges {jump_limit})
    (fun p => execute p signatures handlers work p.entry state) (matched handlers work state)
#print axioms matchedContinuations
"""
    if interactions:
        provenance_theorems.extend(
            (
                "traceSignatures",
                "matchingRequests",
                "SolarTV.EVM.InteractionTrace.stable_request",
            )
        )
        names = ",".join(json.dumps(name) for name in table)
        contextual += f"""
theorem traceSignatures :
    ([{names}] : List String).all (EVM.InteractionTrace.signatureFits signatures) = true :=
  by decide +kernel
theorem matchingRequests {{Payload Halt : Type}} (view : Payload → EVM.InteractionTrace.View)
    (ordinary : Handlers Payload Halt)
    (reply : EVM.InteractionTrace.Request → Payload → Except Halt Payload)
    (work : Nat) (state : State (EVM.InteractionTrace.Cursor Payload)) :
    ((EVM.ContinuationFlow.project beforeSource beforePoints beforeEdges).bind
        (fun p => project p sources edges {jump_limit})).map
        (fun p => execute p signatures (EVM.InteractionTrace.handlers view ordinary reply)
          work p.entry state) =
      ((EVM.ContinuationFlow.project afterSource afterPoints afterEdges).bind
        (fun p => project p targets edges {jump_limit})).map
        (fun p => execute p signatures (EVM.InteractionTrace.handlers view ordinary reply)
          work p.entry state) :=
  matchedContinuations (EVM.InteractionTrace.handlers view ordinary reply) work state
#print axioms traceSignatures
#print axioms matchingRequests
#print axioms EVM.InteractionTrace.stable_request
"""
    projections_code = []
    normalized_terms = []
    for name, source, point_array, model in zip(
        ("old", "new"),
        ("before", "after"),
        ("sources", "targets"),
        normalized,
        strict=True,
    ):
        root, rows = render_nodes(
            model, catalog, version, "bb0", dynamic_exit=True, interactions=interactions
        )
        normalized_terms.append(f"def {name} : Program := ⟨{root},#[{','.join(rows)}]⟩")
        projections_code.append(
            projection_proof(
                f"{name}Projection",
                "EVM.ControlFlow",
                f"{source} {point_array} edges {jump_limit}",
                point_array,
                [f"[{left},{right}]" for left, right in batched(rows, 2)],
                name,
            )
        )
    normalized_declarations = "\n".join(normalized_terms)
    code = (
        PRELUDE
        + f"""
open EVM.SourceGraph (Program)
open EVM.StackGraph EVM.ControlFlow
set_option maxRecDepth 16384
set_option maxHeartbeats 2500000
def before : Program := {expressions[0]}
def after : Program := {expressions[1]}
{provenance}
{normalized_declarations}
def sources : Array Nat := #{points[0]}
def targets : Array Nat := #{points[1]}
def edges : Array Edges := #[{",".join(edges)}]
def bounds (i : Nat) : Nat := (#{bounds} : Array Nat)[i]?.getD 0
def signatures (name : String) : Option (Nat × Nat) :=
  ([{signatures}] : List (String × (Nat × Nat))).lookup name
{"".join(projections_code)}
{layout_proof(len(normalized[0]["nodes"]))}
theorem matched {{World Observation : Type}} (handlers : Handlers World Observation)
    (work : Nat) (state : State World) :
    (project before sources edges {jump_limit}).map
        (fun p => execute p signatures handlers work p.entry state) =
      (project after targets edges {jump_limit}).map
        (fun p => execute p signatures handlers work p.entry state) :=
  EVM.ControlFlow.contextual before after old new sources targets edges {jump_limit}
    signatures bounds oldProjection newProjection valid handlers work state
#print axioms matched
#print axioms EVM.ControlFlow.follow_execute
#print axioms EVM.ControlFlow.resolve_edge
{contextual}
"""
    )
    built = build_model(timeout)
    if built.returncode:
        raise Unsupported(built.stdout + built.stderr)
    with tempfile.TemporaryDirectory(prefix="solar-tv-cfg-flow-") as folder:
        proof = Path(folder) / "Pair.lean"
        proof.write_text(code)
        result = run(["lake", "env", "lean", proof], ROOT, timeout)
    if result.returncode:
        raise Unsupported(
            "control-flow path certificate did not close\n"
            + result.stdout
            + result.stderr
        )
    for theorem in (
        "matched",
        "SolarTV.EVM.ControlFlow.follow_execute",
        "SolarTV.EVM.ControlFlow.resolve_edge",
        *provenance_theorems,
    ):
        axioms = re.search(rf"'{theorem}' depends on axioms: \[(.*?)\]", result.stdout)
        if not (
            f"'{theorem}' does not depend on any axioms" in result.stdout
            or axioms
            and set(axioms[1].split(", "))
            <= {"propext", "Classical.choice", "Quot.sound"}
        ):
            raise Unsupported("unexpected control-flow proof axioms")
    programs = []
    for index, (text, model, paired) in enumerate(
        zip(texts, models, points, strict=True)
    ):
        source, witness = continuations[index] if continuations else (model, None)
        labels = {f"bb{row['label']}" for row in source["nodes"]}
        projection = (
            "@module ControlFlow\n"
            + "\n".join(
                f"{label}:\n" + "\n".join(body)
                for label, body in regions.blocks(text).items()
                if label in labels
            )
            + "\n"
        )
        programs.append(
            Program(
                ("evm-cfg-flow", entry),
                [],
                "",
                "",
                canonical_ir=projection,
                replay_ir=text,
                selector=entry,
                scope=(
                    "interaction-flow"
                    if interactions
                    else "continuation-flow"
                    if witness
                    else "cfg-flow"
                ),
                evm_version=version,
                import_evidence={
                    "kind": "kernel-checked-interaction-trace"
                    if interactions
                    else "kernel-checked-continuation-flow"
                    if witness
                    else "kernel-checked-jump-chain-projection",
                    "graph_sha256": hashlib.sha256(
                        json.dumps(model, sort_keys=True).encode()
                    ).hexdigest(),
                    "logical_actions": len(points[0]),
                    "source_points": len(model["nodes"]),
                    "reused_points": len(paired) - len(set(paired)),
                    "stack_height_bounds": bounds,
                    "changed_growth_checks": changed,
                    "boundary": "compiler source graph and schema; position-independent opcode handlers; all logical steps with temporary stack growth; no physical gas claim",
                    **(witness["evidence"] if witness else {}),
                },
            )
        )
    return audit.Result(
        "verified", stdout="verified\n", program=programs[0], pair=tuple(programs)
    )
