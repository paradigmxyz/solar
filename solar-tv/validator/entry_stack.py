"""Certify an EVM stack envelope independently on both sides of a region."""

import os
from dataclasses import dataclass

from . import cfg_flow, continuation_flow, correspondence, stack_layout
from .common import Unsupported


@dataclass
class Caller:
    witness: dict
    signatures: dict
    bounds: list[int]
    cut: int

    def mask(self):
        masks = {
            tuple(tag is not None for tag in shape)
            for pc, shape in self.witness["point_data"]
            if pc == self.cut
        }
        if len(masks) != 1:
            raise Unsupported("region entry has different caller return-slot layouts")
        return masks.pop()


def propose(text, entry, replay, catalog, version, timeout):
    source = correspondence.export(text, None, version, replay, timeout, labels=True)
    graph, witness = continuation_flow.project(
        source, catalog, version, interactions=True, from_entry=True
    )
    cut = next(
        (row["id"] for row in source["nodes"] if row["label"] == int(entry[2:])), None
    )
    if cut is None or not any(pc == cut for pc, _ in witness["point_data"]):
        raise Unsupported(f"{entry} is outside the physical entry projection")
    signatures = stack_layout.signatures(graph, catalog)
    _, bounds, _ = stack_layout.certificate(graph, graph, signatures, entry_bound=0)
    return Caller(witness, signatures, bounds, cut)


def ranges(start, count, check="check"):
    if count <= 256:
        return f"(checkedRange {check} {start} {count} (by decide +kernel))"
    middle = count // 2
    return f"(joinRanges {ranges(start, middle, check)} {ranges(start + middle, count - middle, check)})"


def certificate(namespace, proposal, catalog, limit):
    witness, signatures, bounds = proposal.witness, proposal.signatures, proposal.bounds
    shapes = {}
    points = []
    for pc, shape in witness["point_data"]:
        index = shapes.setdefault(shape, len(shapes))
        points.append(f"({pc},{index})")
    return (
        f"\nnamespace {namespace}\n"
        + cfg_flow.table(
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
        + cfg_flow.table("boundRows", "Nat", list(map(str, bounds)), lookup=False)
        + cfg_flow.signatures_declaration(signatures, catalog)
        + f"""
open EVM.StackReachability
def points := EVM.ContinuationFlow.internedPoints rootPoints.view rootShapes.view
def graph := project rootNodes.view points rootEdges.view
def ceilingAt (i : Nat) : Nat := (boundRows.get? i).getD 0
def check := closedAt graph signatures ceilingAt {limit}
theorem trees :
    rootNodes.checked && rootPoints.checked && rootShapes.checked &&
      rootEdges.checked && boundRows.checked = true :=
  by decide +kernel
theorem checked : CheckedRange check 0 rootPoints.size :=
  {ranges(0, len(bounds))}
theorem entryFits {{pc height : Nat}}
    (reaches : Reaches graph signatures pc height) : height ≤ {limit} :=
  projected_cap (source := {witness["entry"]}) (by decide +kernel) (by decide +kernel)
    (fun point inside => checked point (Nat.zero_le _) inside) reaches
end {namespace}
"""
    )


def evidence(proposal):
    return {**proposal.witness["evidence"], "max_height": max(proposal.bounds)}


def mask_proof(namespace, caller):
    return f"""
namespace {namespace}
open EVM.StackReachability EVM.ContinuationFlow
def checkMask := callerMaskAt points {caller.cut} callerMask
theorem checkedMask : CheckedRange checkMask 0 points.size :=
  {ranges(0, len(caller.bounds), "checkMask")}
theorem callerLayout (i : Nat) (inside : i < points.size) (shape : Shape)
    (selected : points.get? i = some ⟨{caller.cut},shape⟩) :
    shape.map Option.isSome = callerMask :=
  caller_mask (checkedMask i (Nat.zero_le _) inside) selected
end {namespace}
"""


def compile_callers(directory, callers, catalog, bound, prelude, timeout):
    """Compile each physical-entry proof before importing either theorem."""
    from .check import ROOT, run

    env = os.environ.copy()
    env["LEAN_PATH"] = os.pathsep.join(
        [str(directory), *filter(None, [env.get("LEAN_PATH")])]
    )
    imports = []
    for name, caller in zip(("BeforeFrame", "AfterFrame"), callers, strict=True):
        path = directory / f"{name}.lean"
        path.write_text(prelude + certificate(name, caller, catalog, bound))
        checked = run(
            [
                "lake",
                "env",
                "lean",
                "-j1",
                f"--root={directory}",
                "-o",
                path.with_suffix(".olean"),
                path,
            ],
            ROOT,
            timeout,
            env=env,
        )
        if checked.returncode:
            raise Unsupported(
                f"{name} stack certificate did not close\n"
                + checked.stdout
                + checked.stderr
            )
        imports.append(f"import {name}\n")
    return "".join(imports), env
