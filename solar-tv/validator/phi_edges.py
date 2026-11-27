"""Kernel-check pure edge splits against independently resolved MIR bodies."""

import re
import tempfile
from pathlib import Path

from . import mir_state


def targets(term):
    match term["kind"]:
        case "jump":
            return [term["target"]]
        case "branch":
            return [term["then"], term["else"]]
        case "switch":
            return [term["default"], *(block for _, block in term["cases"])]
        case _:
            return []


def splits(before, after):
    retained = len(before["blocks"])
    edges = []
    for shim, block in enumerate(after["blocks"][retained:], retained):
        term = block["terminator"]
        if (
            block["instructions"]
            or block["phis"]
            or term["kind"] != "jump"
            or not 0 <= term["target"] < retained
        ):
            return []
        predecessors = [
            pred
            for pred, body in enumerate(after["blocks"][:retained])
            if shim in targets(body["terminator"])
        ]
        if len(predecessors) != 1:
            return []
        edges.append((predecessors[0], term["target"]))
    return edges


def check(before, after, definitions, timeout):
    from .check import PRELUDE, ROOT, build_model, run

    edges = splits(before, after)
    if not edges:
        return []
    source = PRELUDE + "set_option maxRecDepth 16384\n"
    source += f"def old := ({mir_state.function_body(before, definitions)} : Nat × MIR.State.Function).2\n"
    source += f"def new := ({mir_state.function_body(after, definitions)} : Nat × MIR.State.Function).2\n"
    stages = [f"({pred},{succ})" for pred, succ in edges]
    source += f"def edits : List (Nat × Nat) := [{','.join(stages)}]\n"
    source += "theorem shape : new = edits.foldl MIR.PhiEdges.split old := by rfl\n"
    source += "#print axioms shape\n#print axioms MIR.PhiEdges.redirect_destination\n"
    for i, (pred, succ) in enumerate(edges):
        source += f"""
def stage{i} := (edits.take {i}).foldl MIR.PhiEdges.split old
def phis{i} := (stage{i}.blocks[{succ}]'(by decide)).phis
theorem real_edge{i} : MIR.PhiEdges.admissible stage{i} ({pred},{succ}) = true := by decide
theorem entry{i} (frame : MIR.State.Frame) (predecessor : Nat)
    (valid : predecessor ≠ stage{i}.blocks.length) :
    MIR.State.enter (some (MIR.PhiEdges.replace {pred} stage{i}.blocks.length predecessor))
      (phis{i}.map (MIR.PhiEdges.rekey {pred} stage{i}.blocks.length)) frame =
      MIR.State.enter (some predecessor) phis{i} frame :=
  MIR.PhiEdges.enter_rekey {pred} _ predecessor frame phis{i} (by decide) valid
theorem bypass{i} (frame : MIR.State.Frame) :
    (do
      let frame ← MIR.State.enter (some {pred}) [] frame
      MIR.State.enter (some stage{i}.blocks.length)
        (phis{i}.map (MIR.PhiEdges.rekey {pred} stage{i}.blocks.length)) frame) =
      MIR.State.enter (some {pred}) phis{i} frame :=
  MIR.PhiEdges.forward {pred} _ frame phis{i} (by decide) (by decide)
#print axioms real_edge{i}
#print axioms entry{i}
#print axioms bypass{i}
"""
    built = build_model(timeout)
    if built.returncode:
        return []
    with tempfile.TemporaryDirectory(prefix="solar-tv-phi-splits-") as folder:
        path = Path(folder) / "Pair.lean"
        path.write_text(source)
        result = run(["lake", "env", "lean", path], ROOT, timeout)
    assumptions = re.findall("depends on axioms: \\[(.*?)\\]", result.stdout)
    if result.returncode or any(
        set(value.split(", ")) - {"propext", "Quot.sound", "Classical.choice"}
        for value in assumptions
    ):
        return []
    return edges
