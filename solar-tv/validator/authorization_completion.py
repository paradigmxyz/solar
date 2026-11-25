"""Resume the checked nonce store through the two nested signing evaluators."""

import json
from pathlib import Path

from . import authorization_entry, caller_gas, entry_proof
from .callers import require

NAME = "MorphoAuthorizationCompletion"


def source(entry, recovery):
    return f"""import {entry}
import {recovery}
{entry_proof.HEADER}
namespace MorphoAuthorizationCompletion
open SolarTV.Execution
open AuthorizationEntry hiding tvWorldConfig

def resumeNonce (lowered : Bool) (env : World.Environment) (journal : Journal)
    (gas work : Nat) : EVM.State.Run :=
  MIR.StorageBoundary.resumeWith tvWorldConfig (nonceWarmed env journal)
    {{nonceStoreFrame env journal with gas}} (nonceKey env) (nonceNext env journal)
    (nonceStoreRest env journal) (EVM.State.Reentry.runWith tvWorldConfig (route lowered) work)

theorem routedNonce (lowered : Bool) (env : World.Environment) (journal : Journal)
    (gas work : Nat) :
    EVM.State.Reentry.runWith tvWorldConfig (route lowered) (work+1)
      (nonceWarmed env journal) {{nonceStoreFrame env journal with gas}} =
        resumeNonce lowered env journal gas work := by
  have fits : ¬ 1024 < (nonceStoreRest env journal).length := by simp [nonceStoreRest]
  have pc : (nonceStoreFrame env journal).pc = noncePC := rfl
  have image : (nonceStoreFrame env journal).code = code.bytes := rfl
  have stack : (nonceStoreFrame env journal).stack =
      nonceKey env :: nonceNext env journal :: nonceStoreRest env journal := rfl
  rw [EVM.State.Reentry.runWith]
  simp only [route,EVM.State.Reentry.choose,routeStore,pc,image,stack,
    true_and,true_or,and_true,ite_true,Option.orElse_some,fits,ite_false,resumeNonce]

theorem nonceToExit (lowered : Bool) (env : World.Environment) (journal : Journal)
    (gas work : Nat) :
    resumeNonce lowered env journal gas work =
    EVM.State.run (work+1) tvWorldConfig (nonceWarmed env journal)
      {{nonceStoreFrame env journal with gas}} := by
  rw [← routedNonce, EVM.State.Reentry.correspondence _ _ (routeSound lowered)]

theorem authorizationToExit {authorization_entry.BINDINGS}
    (noValue : env.value = 0)
    (selected : MIR.TransferFromDispatch.selector env.input = {authorization_entry.SELECTOR})
    (live : expiry env = 0#256)
    (noOverflow : nonceNext env journal ≠ 0#256)
    (lowered : Bool) (work balance : Nat) (paid : noncePrefixCharge env journal ≤ balance) :
    EVM.State.run (work+1+noncePrefixSteps) tvWorldConfig journal (dispatchFrame env balance 0 []) =
      (do
        let result ← resumeNonce lowered env journal (balance-noncePrefixCharge env journal) work
        return {{result with work := result.work+noncePrefixSteps}}) := by
  rw [nonceToExit]
  exact (authorizationToNonce {authorization_entry.ARGS} noValue selected live noOverflow).paid
    (work+1) balance paid

theorem signingToExit (lowered : Bool) (env : World.Environment) (journal : Journal)
    (work balance : Nat)
    (matched : readWord env.input 100 = nonceValue env journal)
    (canonical : (nonceOwner env).toNat < 2^160)
    (writable : env.isStatic = false)
    (canStore : MIR.StorageBoundary.exhausted tvWorldConfig (nonceFrame env journal balance)
      (nonceWritePrice env journal) = false)
    (canEncode : recordFee ≤ balance-(nonceWritePrice env journal).gas) :
    EVM.State.run (work+recordSteps+1) tvWorldConfig (nonceWarmed env journal)
      (nonceFrame env journal balance) = (do
      let result ← EVM.State.Reentry.runWith tvWorldConfig (route lowered) work
        (nonceWritten env journal)
        {{signingFrame env journal with gas := balance-(nonceWritePrice env journal).gas-recordFee}}
      return {{result with work := result.work+(recordSteps+1)}}) := by
  rw [EVM.State.Reentry.correspondence _ _ (routeSound lowered)]
  exact nonceToSigning env journal work balance matched canonical writable canStore canEncode

def receipt (lowered : Bool) (env : World.Environment) (journal : Journal) (gas : Nat) : String :=
  EVM.State.describe tvWorldConfig .exact (do
    let result ← resumeNonce lowered env journal gas 2048
    return if result.stop = .success then {{result with «world» := EVM.State.finalize result.world}}
      else {{result with «world» := journal, gas :=
        if result.stop = .exception || result.stop = .outOfGas then 0 else result.gas}})

#print axioms routedNonce
#print axioms authorizationToExit
#print axioms signingToExit
end MorphoAuthorizationCompletion
"""


def certify(kernel, entry, recovery, timeout):
    text = source(Path(entry).stem, Path(recovery).stem)
    result = entry_proof.lean(kernel, NAME, text, timeout, module=True)
    output = entry_proof.success(result, NAME)
    entry_proof.check_axioms(output.splitlines(), 3)
    return f"kernel/{NAME}.lean"


def evaluate(kernel, cases, timeout):
    source = f"import {NAME}\nopen SolarTV EvmRules MorphoAuthorizationCompletion\n"
    source += "set_option maxRecDepth 16384\n"
    for index, (state, gas) in enumerate(cases):
        source += caller_gas.state_declarations(index, state)
        source += f"""
#eval IO.println ("[" ++ receipt false env{index} journal{index} {gas} ++ "," ++
  receipt true env{index} journal{index} {gas} ++ "]")
"""
    result = entry_proof.lean(
        kernel, "ReplayMorphoAuthorizationCompletion", source, timeout
    )
    output = entry_proof.success(result, "authorization from nonce SSTORE")
    try:
        receipts = [json.loads(line) for line in output.splitlines()]
    except ValueError:
        require(False, f"unsupported authorization completion\n{output}")
    require(len(receipts) == len(cases), "missing authorization receipts")
    return receipts
