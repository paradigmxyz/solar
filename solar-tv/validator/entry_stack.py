"""Certify an EVM stack envelope independently on both sides of a region."""

from . import cfg_flow, continuation_flow, correspondence, stack_layout
from .common import Unsupported


def propose(text, entry, replay, catalog, version, timeout):
    source = correspondence.export(text, None, version, replay, timeout, labels=True)
    graph, witness = continuation_flow.project(
        source, catalog, version, interactions=True, from_entry=True
    )
    if int(entry[2:]) not in witness["block_labels"]:
        raise Unsupported(f"{entry} is outside the physical entry projection")
    signatures = stack_layout.signatures(graph, catalog)
    _, bounds, _ = stack_layout.certificate(graph, graph, signatures, entry_bound=0)
    return witness, signatures, bounds


def ranges(start, count):
    if count <= 256:
        return f"(checkedRange check {start} {count} (by decide +kernel))"
    middle = count // 2
    return (
        f"(joinRanges {ranges(start, middle)} {ranges(start + middle, count - middle)})"
    )


def certificate(namespace, proposal, catalog, limit):
    witness, signatures, bounds = proposal
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
    witness, _, bounds = proposal
    return {**witness["evidence"], "max_height": max(bounds)}
