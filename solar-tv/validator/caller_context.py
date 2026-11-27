"""Certify the unchanged MIR context around the captured replacement bodies."""

from . import entry_proof, mir_state
from .callers import require
from .check import build_model

MODULE = "SolarTV.MIR.Transforms.CallContext"
NAME = "MorphoCallContext"


def source(tables, sites, catalog):
    left, right = [
        sorted(table.values(), key=lambda body: body["id"]) for table in tables
    ]
    require(
        [(body["id"], body["return_buffer"]) for body in left]
        == [(body["id"], body["return_buffer"]) for body in right],
        "MIR replacement function identities changed",
    )
    pairs = list(zip(left, right, strict=True))
    changed = {old["return_buffer"]: old["id"] for old, new in pairs if old != new}
    require(
        not any(site.name in changed for site in sites),
        "a Morpho caller body was replaced",
    )
    entries = {site.name: tables[0][site.name]["id"] for site in sites}
    commands = [
        f"import {MODULE}",
        "import SolarTV.MIR.Transforms.TransferFromCallers",
        entry_proof.HEADER.replace(" MIR.TransferFromEntry", ""),
        "open MIR.State MIR.CallContext MIR.TransferFromCallers",
        f"def replacements : List Nat := {list(changed.values())}",
    ]
    for label, table in zip(("before", "after"), (left, right), strict=True):
        rows = ",\n  ".join(mir_state.function_body(row, catalog) for row in table)
        commands.append(f"def {label} : Functions := [\n  {rows}]")
    proof = ".nil"
    for old, new in reversed(pairs):
        proof = f"({'.same _ _' if old == new else '.replaced _ _ _ (by decide) rfl'} {proof})"
    commands.extend(
        (
            f"theorem contextAgreement : Agreement replacements before after :=\n  {proof}",
            "def callerEntry : Caller → Nat",
            *(f"  | .{name} => {entry}" for name, entry in entries.items()),
            """def callerPrefix (functions : Functions) (caller : Caller) (fuel : Nat)
    (cfg : Config) (frame : Frame) : Except String Cut := do
  let id := callerEntry caller
  let frame ← start functions id frame.arguments frame
  runTo replacements fuel cfg functions ⟨{function := id},[],frame⟩

theorem all_caller_prefixes (caller : Caller) (fuel : Nat) (cfg : Config) (frame : Frame) :
    callerPrefix before caller fuel cfg frame = callerPrefix after caller fuel cfg frame := by
  unfold callerPrefix
  dsimp only
  rw [contextAgreement.start_eq]
  cases start after (callerEntry caller) frame.arguments frame with
  | error _ => rfl
  | ok initial => exact contextAgreement.prefix_eq fuel cfg ⟨{function := callerEntry caller},[],initial⟩

theorem all_callers_with_replacements (caller : Caller) (fuel : Nat) (cfg : Config) (frame : Frame)
    (replace : ∀ remaining reached,
      callerPrefix before caller fuel cfg frame = .ok (.inr (remaining,reached)) →
      run remaining cfg before reached = run remaining cfg after reached) :
    execute fuel cfg before (callerEntry caller) frame =
      execute fuel cfg after (callerEntry caller) frame := by
  unfold execute
  rw [contextAgreement.start_eq]
  by_cases depth : cfg.callDepth = 0
  · simp [depth]
  simp only [depth, ite_false]
  cases started : start after (callerEntry caller) frame.arguments frame with
  | error _ => rfl
  | ok initial =>
    simp only [Bind.bind, Except.bind]
    congr 1
    apply contextAgreement.compose
    intro remaining reached h
    apply replace remaining reached
    simpa only [callerPrefix, contextAgreement.start_eq, started, Bind.bind, Except.bind] using h

#print axioms all_caller_prefixes
#print axioms all_callers_with_replacements
""",
        )
    )
    return "\n".join(commands), changed


def certify(directory, tables, sites, catalog, timeout):
    built = build_model(timeout, MODULE)
    entry_proof.success(built, MODULE)
    proof, replaced = source(tables, sites, catalog)
    checked = entry_proof.lean(directory, NAME, proof, timeout, module=True)
    output = entry_proof.success(checked, NAME)
    entry_proof.check_axioms(output.splitlines(), 2)
    return {
        "proof": f"kernel/{NAME}.lean",
        "scope": "shared typed MIR execution from caller entry to the first replacement or terminal outcome",
        "replacements": replaced,
    }
