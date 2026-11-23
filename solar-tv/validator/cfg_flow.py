"""Prove jump-chain contraction and block coalescing in exported EVM graphs."""

import hashlib
import json
import re
import tempfile
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


def tree(rows):
    if len(rows) <= 12:
        return f"(.leaf [{','.join(rows)}])"
    middle = len(rows) // 2
    return f"(.branch {len(rows)} {tree(rows[:middle])} {tree(rows[middle:])})"


def table(name, row_type, rows, *, lookup=True):
    definition = f"""
def {name} : EVM.ProofTable.Tree {row_type} := {tree(rows)}
"""
    if not lookup:
        return definition
    return (
        definition
        + f"""
theorem {name}Lookup : {name}.view = EVM.ProofTable.ofArray {name}.rows.toArray :=
  {name}.view_eq (by decide +kernel)
"""
    )


def projection_proof(name, module, program, nodes, points, edges, result, limit=""):
    prefix = f"{program} {points}.rows.toArray {edges}.rows.toArray {limit}"
    return f"""
theorem {name} : {module}.project {prefix} = some {result} :=
  {module}.project_checked {program} {points}.rows.toArray {edges}.rows.toArray
    {nodes}.view {points}.view {edges}.view {limit} {result}
    {nodes}Lookup {points}Lookup {edges}Lookup (by decide +kernel)
"""


def continuation_proof(name, witness, result, *, inherited=False):
    """Bind the continuation projection to the independently exported graph."""
    tables = (
        table(f"{name}SourceNodes", "EVM.ContinuationFlow.Node", witness["source"])
        + table(f"{name}Points", "EVM.ContinuationFlow.Point", witness["points"])
        + table(f"{name}Edges", "Edges", witness["edges"])
        + f"""
def {name}Source : EVM.ContinuationFlow.Program :=
  ⟨{witness["entry"]},{name}SourceNodes.rows.toArray⟩
"""
    )
    if inherited:
        return (
            tables
            + f"""
theorem {name}Provenance :
    EVM.ContinuationFlow.projectInherited {name}Source {name}Points.rows.toArray
      {name}Edges.rows.toArray callerMask = some {result} :=
  EVM.ContinuationFlow.inherited_checked {name}Source {name}Points.rows.toArray
    {name}Edges.rows.toArray callerMask {name}SourceNodes.view {name}Points.view {name}Edges.view
    {result} {name}SourceNodesLookup {name}PointsLookup {name}EdgesLookup (by decide +kernel)
"""
        )
    return tables + projection_proof(
        f"{name}Provenance",
        "EVM.ContinuationFlow",
        f"{name}Source",
        f"{name}SourceNodes",
        f"{name}Points",
        f"{name}Edges",
        result,
    )


def signatures_declaration(signature_table, catalog):
    # Keep literal operands in the graph without searching through them for arities.
    push = catalog["push1"]["stack"]
    rows = ",".join(
        f"({json.dumps(name)},({inputs},{outputs}))"
        for name, (inputs, outputs) in signature_table.items()
        if not name.startswith("push ")
    )
    return f"""
def signatures (name : String) : Option (Nat × Nat) :=
  if name.startsWith "push " then some ({push[0]},{push[1]}) else
    ([{rows}] : List (String × (Nat × Nat))).lookup name
"""


def check(
    texts,
    models,
    entry,
    catalog,
    version,
    timeout,
    *,
    continuations=None,
    interactions=False,
    from_entry=False,
    code_layout=None,
):
    from .check import PRELUDE, ROOT, build_model, run
    from .source_identity import INTERACTION_OPS, render_nodes

    points, normalized, edges, jump_limit = projections(models)
    signature_table = stack_layout.signatures(normalized[0], catalog)
    entry_bound = 0 if from_entry else 1024
    state_argument = "(payload : World)" if from_entry else "(state : State World)"
    state_value = "⟨0,payload⟩" if from_entry else "state"
    identical = normalized[0] == normalized[1]
    if identical:
        bounds, changed = [], []
    else:
        _, bounds, changed = stack_layout.certificate(
            *normalized, signature_table, entry_bound=entry_bound
        )
    program_declarations = ""
    for name, model in zip(
        ("before", "after", "old", "new"), (*models, *normalized), strict=True
    ):
        if name == "new" and identical:
            program_declarations += "def new : Program := old\n"
            continue
        root, rows = render_nodes(
            model,
            catalog,
            version,
            "bb0" if continuations or name in {"old", "new"} else entry,
            dynamic_exit=True,
            interactions=interactions,
            from_entry=from_entry,
            max_growth=4 if from_entry else 1,
        )
        program_declarations += table(f"{name}Nodes", "EVM.SourceGraph.Node", rows)
        program_declarations += (
            f"def {name} : Program := ⟨{root},{name}Nodes.rows.toArray⟩\n"
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
            "conditional_words",
            "conditional_admission",
        ):
            theorem = f"SolarTV.EVM.ContinuationFlow.{lemma}"
            provenance += f"#print axioms {theorem}\n"
            provenance_theorems.append(theorem)
        for name, (_, witness) in zip(("before", "after"), continuations, strict=True):
            theorem = f"{name}Provenance"
            provenance_theorems.append(theorem)
            provenance += continuation_proof(name, witness, name)
            provenance += f"#print axioms {theorem}\n"
        provenance_theorems.append("matchedContinuations")
        contextual = f"""
theorem matchedContinuations {{World Observation : Type}} (handlers : Handlers World Observation)
    (work : Nat) {state_argument} :
    ((EVM.ContinuationFlow.project beforeSource beforePoints.rows.toArray beforeEdges.rows.toArray).bind
        (fun p => project p sources edges {jump_limit})).map
        (fun p => execute p signatures handlers work p.entry {state_value}) =
      ((EVM.ContinuationFlow.project afterSource afterPoints.rows.toArray afterEdges.rows.toArray).bind
        (fun p => project p targets edges {jump_limit})).map
        (fun p => execute p signatures handlers work p.entry {state_value}) :=
  EVM.ControlFlow.compose beforeProvenance afterProvenance
    (fun p => project p sources edges {jump_limit}) (fun p => project p targets edges {jump_limit})
    (fun p => execute p signatures handlers work p.entry {state_value})
    (matched handlers work {"payload" if from_entry else "state"})
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
        names = ",".join(
            json.dumps(name)
            for name in sorted(INTERACTION_OPS & signature_table.keys())
        )
        trace_argument = (
            "(payload : EVM.InteractionTrace.Cursor Payload)"
            if from_entry
            else "(state : State (EVM.InteractionTrace.Cursor Payload))"
        )
        contextual += f"""
theorem traceSignatures :
    ([{names}] : List String).all (EVM.InteractionTrace.signatureFits signatures) = true :=
  by decide +kernel
theorem matchingRequests {{Payload Halt : Type}} (view : Payload → EVM.InteractionTrace.View)
    (ordinary : Handlers Payload Halt)
    (reply : EVM.InteractionTrace.Request → Payload → Except Halt Payload)
    (work : Nat) {trace_argument} :
    ((EVM.ContinuationFlow.project beforeSource beforePoints.rows.toArray beforeEdges.rows.toArray).bind
        (fun p => project p sources edges {jump_limit})).map
        (fun p => execute p signatures (EVM.InteractionTrace.handlers view ordinary reply)
          work p.entry {state_value}) =
      ((EVM.ContinuationFlow.project afterSource afterPoints.rows.toArray afterEdges.rows.toArray).bind
        (fun p => project p targets edges {jump_limit})).map
        (fun p => execute p signatures (EVM.InteractionTrace.handlers view ordinary reply)
          work p.entry {state_value}) :=
  matchedContinuations (EVM.InteractionTrace.handlers view ordinary reply) work
    {"payload" if from_entry else "state"}
#print axioms traceSignatures
#print axioms matchingRequests
#print axioms EVM.InteractionTrace.stable_request
"""
    tables = (
        table("sourcePoints", "Nat", list(map(str, points[0])))
        + table("targetPoints", "Nat", list(map(str, points[1])))
        + table("transferEdges", "Edges", edges)
    )
    projections_code = "".join(
        projection_proof(
            f"{name}Projection",
            "EVM.ControlFlow",
            source,
            f"{source}Nodes",
            point_table,
            "transferEdges",
            name,
            str(jump_limit),
        )
        for name, source, point_table in (
            ("old", "before", "sourcePoints"),
            ("new", "after", "targetPoints"),
        )
    )
    if identical:
        validity = ""
        match_proof = f"""congrArg (Option.map (fun p : Program => execute p signatures handlers work p.entry {state_value}))
    (oldProjection.trans newProjection.symm)"""
    else:
        validity = (
            table("heightBounds", "Nat", list(map(str, bounds)))
            + f"""
def bounds (i : Nat) : Nat := (heightBounds.get? i).getD 0
theorem valid : Valid old new id bounds signatures {entry_bound} :=
  valid_views old new oldNodes.view newNodes.view id bounds signatures {entry_bound}
    oldNodesLookup newNodesLookup (by decide +kernel) (by rfl) (by decide +kernel)
    (by decide +kernel)
"""
        )
        lemma = "contextual_entry" if from_entry else "contextual"
        match_proof = f"""EVM.ControlFlow.{lemma} before after old new sources targets edges {jump_limit}
    signatures bounds oldProjection newProjection valid handlers work {"payload" if from_entry else "state"}"""
    code = (
        PRELUDE
        + f"""
open EVM.SourceGraph (Program)
open EVM.StackGraph EVM.ControlFlow
noncomputable section
set_option maxRecDepth 16384
set_option maxHeartbeats 2500000
{program_declarations}
{provenance}
{tables}
def sources : Array Nat := sourcePoints.rows.toArray
def targets : Array Nat := targetPoints.rows.toArray
def edges : Array Edges := transferEdges.rows.toArray
{signatures_declaration(signature_table, catalog)}
{projections_code}
{validity}
theorem matched {{World Observation : Type}} (handlers : Handlers World Observation)
    (work : Nat) {state_argument} :
    (project before sources edges {jump_limit}).map
        (fun p => execute p signatures handlers work p.entry {state_value}) =
      (project after targets edges {jump_limit}).map
        (fun p => execute p signatures handlers work p.entry {state_value}) :=
  {match_proof}
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
        result = run(["lake", "env", "lean", "-j1", proof], ROOT, timeout)
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
                    "layout-flow"
                    if from_entry
                    else "interaction-flow"
                    if interactions
                    else "continuation-flow"
                    if witness
                    else "cfg-flow"
                ),
                evm_version=version,
                import_evidence={
                    "kind": "kernel-checked-entry-layout"
                    if from_entry
                    else "kernel-checked-interaction-trace"
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
                    **(
                        {
                            "physical_entry": True,
                            "entry_height_bound": entry_bound,
                            "block_labels": sorted(witness["block_labels"]),
                        }
                        if from_entry
                        else {}
                    ),
                    **({"code_layout": code_layout[index]} if code_layout else {}),
                },
            )
        )
    return audit.Result(
        "verified", stdout="verified\n", program=programs[0], pair=tuple(programs)
    )
