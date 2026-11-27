"""Derive exact Morpho gas from ABI entry through callbacks and early exits."""

import json

from . import entry_proof, world
from .callers import SELECTORS, require
from .check import build_model

MODULE = "SolarTV.EVMIR.State.GasTrace"
NAME = "MorphoCallerGas"


def source(helper, config):
    selectors = list(SELECTORS.values())
    return f"""import EntryImage
import {MODULE}
import SolarTV.EVMIR.State.Observe
import SolarTV.MIR.Transforms.TransferFromDispatch
{entry_proof.HEADER}
namespace MorphoCallerGas
open SolarTV.Execution EVM.State.GasTrace
{config}
def selectors : List Nat := {selectors}
def entry (env : World.Environment) (gas : Nat) : EVM.State.Frame :=
  ⟨env,code.bytes,0,[],[],[],gas⟩

def check (block : Block) (work : Nat) (journal : Journal)
    (env : World.Environment) (gas : Nat) : Except String Report :=
  if selectors.contains (MIR.TransferFromDispatch.selector env.input).toNat then
    toBoundary {{tvWorldConfig with block}} {helper} work journal (entry env gas)
  else .error "not a Morpho transferFrom caller"

theorem abiGas (block : Block) (work : Nat) (journal : Journal)
    (env : World.Environment) (gas : Nat) (report : Report)
    (accepted : check block work journal env gas = .ok report) :
    (report.exit.at {helper} ∧
      (report.debits.map Debit.net).sum + (report.exit.gas : Int) = (gas : Int)) ∧
    report.exit.resume {{tvWorldConfig with block}} =
      EVM.State.run work {{tvWorldConfig with block}} journal (entry env gas) := by
  unfold check at accepted
  split at accepted
  · refine ⟨until_accounted _ _ _ _ _ _ accepted,?_⟩
    have resumed := resume_until {{tvWorldConfig with block}} {helper} work journal (entry env gas)
    rw [accepted] at resumed
    exact resumed
  · contradiction

theorem helperEntry : {helper} ∈ code.jumps 0 := by decide +kernel

def word (x : Word) : Lean.Json := .str (toString x.toNat)
def byte (x : Byte) : Lean.Json := Lean.toJson x.toNat
def message (row : Debit) (gas : MessageDebit) : Lean.Json := .mkObj [
  ("pc",Lean.toJson row.pc),("before",Lean.toJson row.before),("after",Lean.toJson row.after),
  ("prepared",Lean.toJson gas.prepared),("requested",Lean.toJson gas.requested),
  ("charged",Lean.toJson gas.charged),("supplied",Lean.toJson gas.supplied),
  ("returned",Lean.toJson gas.returned)]

def describe (block : Block) (journal : Journal) (checked : Except String Report) : String :=
  match checked with
  | .error reason => "unsupported: " ++ reason
  | .ok report =>
    let result := match report.exit with
    | .reached _ _ frame _ => Lean.Json.mkObj [
      ("pc",Lean.toJson frame.pc),("gas",Lean.toJson frame.gas),
      ("stack",Lean.toJson (frame.stack.map word)),("memory",Lean.toJson (frame.memory.map byte)),
      ("returndata",Lean.toJson (frame.returnData.map byte))]
    | .halted receipt =>
      let final := if receipt.stop = .success then
        {{receipt with «world» := EVM.State.finalize receipt.world}}
      else {{receipt with «world» := journal, gas :=
        if receipt.stop = .exception || receipt.stop = .outOfGas then 0 else receipt.gas}}
      (Lean.Json.parse (EVM.State.describe {{tvWorldConfig with block}} .exact (.ok final))).toOption.getD .null
    (Lean.Json.mkObj [
      ("result",result),("net",Lean.toJson (report.debits.map Debit.net).sum),
      ("pcs",Lean.toJson (report.debits.map (·.pc))),
      ("calls",Lean.toJson (report.debits.filterMap fun row => row.message.map (message row))) ]).compress

#print axioms abiGas
#print axioms helperEntry
#print axioms returnedGas
end MorphoCallerGas
"""


def certify(kernel, helper, config, timeout):
    entry_proof.success(build_model(timeout, MODULE), MODULE)
    result = entry_proof.lean(
        kernel, NAME, source(helper, config), timeout, module=True
    )
    output = entry_proof.success(result, NAME)
    entry_proof.check_axioms(output.splitlines(), 3)
    return {"proof": f"kernel/{NAME}.lean", "callers": list(SELECTORS)}


def state_declarations(index, state):
    root = state["address"]
    seed = {
        **state,
        "accounts": [
            {**account, "code": []} if account["address"] == root else account
            for account in state["accounts"]
        ],
    }
    warm = ", ".join(world.warm_addresses(state, "cancun"))
    slots = ", ".join(f"({owner},{key})" for owner, key in state["warm_slots"])
    return f"""
def accounts{index} : List Execution.Account :=
  ({world.accounts_expression(seed)} : List Execution.Account).map fun account =>
    if account.address = {root} then {{account with code := code.bytes}} else account
def journal{index} : Execution.Journal := ⟨accounts{index},accounts{index},[{warm}],[{slots}],[],0⟩
def env{index} : World.Environment := {world.environment_expression(state)}
def block{index} : Execution.Block := {world.context_expression(state)}
"""


def state_source(index, state):
    return (
        state_declarations(index, state)
        + f"""
#eval IO.println (describe block{index} journal{index}
  (check block{index} 2048 journal{index} env{index} {state["gas"]}))
"""
    )


def evaluate(kernel, states, timeout):
    source = (
        f"import {NAME}\n"
        + "open SolarTV EvmRules MorphoCallerGas\n"
        + "set_option maxRecDepth 16384\n"
        + "".join(state_source(i, state) for i, state in enumerate(states))
    )
    output = entry_proof.success(
        entry_proof.lean(kernel, "ReplayMorphoCallerGas", source, timeout),
        "Morpho caller gas evaluation",
    )
    try:
        reports = [json.loads(line) for line in output.splitlines()]
    except ValueError:
        require(False, f"unsupported Morpho caller gas evaluation\n{output}")
    require(len(reports) == len(states), "missing Morpho caller gas observations")
    return reports


def check_reached(report, state, native):
    cut = report["result"]
    names = ("pc", "gas", "stack", "memory", "returndata")
    require(
        isinstance(cut, dict)
        and all(cut[key] == native[key] for key in names)
        and report["pcs"] == native["pcs"]
        and report["net"] == state["gas"] - native["gas"]
        and [call["before"] for call in report["calls"]] == native["budgets"],
        "ABI-to-helper gas disagrees with independent bytecode replay",
    )
