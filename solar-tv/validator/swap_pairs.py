"""Propose cancellation of two SWAP1s across a static transfer."""

from . import stack_layout
from .common import Unsupported


def propose(model, catalog):
    nodes = [row["node"] for row in model["nodes"]]
    windows = {}
    for pc, first in enumerate(nodes):
        if first.get("operation") != "swap 1":
            continue
        transfer = nodes[first["next"]]
        if transfer["kind"] != "jump":
            continue
        second = nodes[transfer["next"]]
        if second.get("operation") == "swap 1":
            windows[pc] = {
                "kind": "jump",
                "growth": transfer["growth"],
                "next": second["next"],
            }
    if not windows:
        return model, None

    signatures = stack_layout.signatures(model, catalog)
    if signatures.get("swap 1") != (2, 2):
        raise Unsupported("return swap certificate requires the SWAP1 signature")
    floors = stack_layout.lower_bounds(model, signatures)
    windows = {pc: jump for pc, jump in windows.items() if floors[pc] >= 2}
    if not windows:
        return model, None
    return {
        **model,
        "nodes": [
            {**row, "node": windows[row["id"]]} if row["id"] in windows else row
            for row in model["nodes"]
        ],
    }, (signatures, floors, windows)


def certificate(name, proposal, catalog, *, chunked):
    from .cfg_flow import check_rows, signatures_declaration, table

    signatures, floors, _ = proposal
    graph = f"{name}Nodes.view"
    floors_at = f"{name}SwapFloors"
    arities = f"{name}SwapSignatures"
    rewritten = f"{name}SwapsNodes.view"
    rows = table(f"{name}SwapFloorRows", "Nat", list(map(str, floors)), lookup=False)
    rows += signatures_declaration(signatures, catalog, name=arities)
    rows += f"""
def {floors_at} (i : Nat) : Nat := ({name}SwapFloorRows.get? i).getD 0
theorem {name}SwapArity : {arities} "swap 1" = some (2,2) := by decide +kernel
def {name}SwapFold := EVM.ProofTable.checkedRow {rewritten}
    (EVM.SwapPairs.replace {graph} {floors_at})
def {name}SwapFloorCheck := EVM.StackReachability.floorAt {graph} {arities} {floors_at}
"""
    for suffix, check in (("Fold", "SwapFold"), ("Floors", "SwapFloorCheck")):
        checked = f"{name}CheckedSwap{suffix}"
        if chunked:
            rows += check_rows(checked, f"{name}{check}", len(floors))
        else:
            rows += f"""
theorem {checked} : EVM.StackReachability.CheckedRange {name}{check} 0 {len(floors)} :=
  EVM.StackReachability.checkedRange {name}{check} 0 {len(floors)} (by decide +kernel)
"""
    return (
        rows
        + f"""
theorem {name}SwapNormalization : EVM.SwapPairs.normalize {name} {floors_at} = some {name}Swaps :=
  EVM.SwapPairs.normalize_checked {name} {name}Swaps {floors_at} {graph} {rewritten}
    {name}NodesLookup {name}SwapsNodesLookup (by rfl) (by rfl)
    (fun i inside => {name}CheckedSwapFold i (Nat.zero_le _) inside)
theorem {name}SwapAdmission {{pc height : Nat}}
    (reached : EVM.StackReachability.Reaches {graph} {arities} pc height)
    (selected : 2 ≤ {floors_at} pc) : 2 ≤ height :=
  EVM.SwapPairs.reachable_window (by decide +kernel) (by rfl)
    (fun i inside => {name}CheckedSwapFloors i (Nat.zero_le _) inside) reached selected
theorem {name}SwapExecution {{World Observation : Type}}
    (handlers : Handlers (EVM.Stack × World) Observation) (pc fuel : Nat)
    (window : EVM.SwapPairs.Window) (state : State (EVM.Stack × World))
    (found : EVM.SwapPairs.read {graph} pc = some window)
    (reached : EVM.StackReachability.Reaches {graph} {arities} pc state.height)
    (selected : 2 ≤ {floors_at} pc) (fits : state.height ≤ 1024) :
    execute {name} {arities} (EVM.SwapPairs.withSwap handlers) (fuel+3) pc state =
      if state.height + window.growth > 1024 then some .overflow
      else execute {name} {arities} (EVM.SwapPairs.withSwap handlers) fuel window.destination state :=
  EVM.SwapPairs.execute_window {name} {arities} handlers pc fuel window
    ({name}NodesLookup ▸ found) {name}SwapArity state ({name}SwapAdmission reached selected) fits
#print axioms {name}SwapArity
#print axioms {name}SwapAdmission
#print axioms {name}SwapNormalization
#print axioms {name}SwapExecution
"""
    )
