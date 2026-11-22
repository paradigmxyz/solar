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
    headings = [{}, {}]
    points = [[], []]
    projections = [{"entry": 0, "nodes": []}, {"entry": 0, "nodes": []}]

    def index(old, new):
        left, right = headings
        if old in left or new in right:
            if left.get(old) != right.get(new):
                raise Unsupported("control-flow pairing changed node sharing")
            return left[old]
        position = len(left)
        left[old] = right[new] = position
        points[0].append(old)
        points[1].append(new)
        return position

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

    index(*(model["entry"] for model in models))
    jump_limit = 1
    for position, pair in enumerate(zip(*points)):
        left, right = [follow(m, pc) for m, pc in zip(models, pair, strict=True)]
        jump_limit = max(jump_limit, left[1] + 1, right[1] + 1)
        a, b = left[2], right[2]
        fields = tuple(field for field in ("next", "yes", "no") if field in a)
        if {k: v for k, v in a.items() if k not in fields} != {
            k: v for k, v in b.items() if k not in fields
        }:
            raise Unsupported("control-flow projection changed an action or terminal")
        for field in fields:
            a[field] = b[field] = 2 * index(a[field], b[field])
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
    return points, projections, jump_limit


def check(texts, models, expressions, entry, catalog, version, timeout):
    from .check import PRELUDE, ROOT, build_model, run
    from .source_identity import render

    points, normalized, jump_limit = projections(models)
    table = stack_layout.signatures(normalized[0], catalog)
    _, bounds, changed = stack_layout.certificate(*normalized, table)
    signatures = ",".join(
        f"({json.dumps(name)},({inputs},{outputs}))"
        for name, (inputs, outputs) in table.items()
    )
    code = (
        PRELUDE
        + f"""
open EVM.SourceGraph (Program)
open EVM.StackGraph EVM.ControlFlow
set_option maxRecDepth 16384
set_option maxHeartbeats 2500000
def before : Program := {expressions[0]}
def after : Program := {expressions[1]}
def old : Program := {render(normalized[0], catalog, version, "bb0", dynamic_exit=True)}
def new : Program := {render(normalized[1], catalog, version, "bb0", dynamic_exit=True)}
def sources : Array Nat := #{points[0]}
def targets : Array Nat := #{points[1]}
def bounds (i : Nat) : Nat := (#{bounds} : Array Nat)[i]?.getD 0
def signatures (name : String) : Option (Nat × Nat) :=
  ([{signatures}] : List (String × (Nat × Nat))).lookup name
theorem oldProjection : project before sources {jump_limit} = some old := by decide
theorem newProjection : project after targets {jump_limit} = some new := by decide
theorem valid : Valid old new id bounds signatures := by decide
theorem matched {{World Observation : Type}} (handlers : Handlers World Observation)
    (work : Nat) (state : State World) :
    (project before sources {jump_limit}).map
        (fun p => execute p signatures handlers work p.entry state) =
      (project after targets {jump_limit}).map
        (fun p => execute p signatures handlers work p.entry state) :=
  EVM.ControlFlow.contextual before after old new sources targets {jump_limit}
    signatures bounds oldProjection newProjection valid handlers work state
#print axioms matched
#print axioms EVM.ControlFlow.follow_execute
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
    for theorem in ("matched", "SolarTV.EVM.ControlFlow.follow_execute"):
        axioms = re.search(rf"'{theorem}' depends on axioms: \[(.*?)\]", result.stdout)
        if not (
            f"'{theorem}' does not depend on any axioms" in result.stdout
            or axioms
            and set(axioms[1].split(", "))
            <= {"propext", "Classical.choice", "Quot.sound"}
        ):
            raise Unsupported("unexpected control-flow proof axioms")
    programs = []
    for text, model in zip(texts, models, strict=True):
        labels = {f"bb{row['label']}" for row in model["nodes"]}
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
                scope="cfg-flow",
                evm_version=version,
                import_evidence={
                    "kind": "kernel-checked-jump-chain-projection",
                    "graph_sha256": hashlib.sha256(
                        json.dumps(model, sort_keys=True).encode()
                    ).hexdigest(),
                    "logical_actions": len(points[0]),
                    "source_points": len(model["nodes"]),
                    "stack_height_bounds": bounds,
                    "changed_growth_checks": changed,
                    "boundary": "compiler source graph and schema; position-independent opcode handlers; all logical steps with temporary stack growth; no physical gas claim",
                },
            )
        )
    return audit.Result(
        "verified", stdout="verified\n", program=programs[0], pair=tuple(programs)
    )
