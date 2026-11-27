"""Certify FlashLoan's emitted zero-assets failure from ABI entry."""

from pathlib import Path

from . import decoder_proof, revert_stub
from .callers import require


def certify(kernel, proof, runtime, decoded, catalog, decoder, route, timeout):
    module = Path(proof).stem
    namespace = module.removesuffix("Decoder")
    stack = decoder.stack.copy()
    require(stack[2] == ("load", 36), "changed FlashLoan amount at ABI exit")
    stack[2] = 0
    rows = revert_stub.plan(
        runtime, decoded, catalog, decoder.pc, stack, decoder.memory
    )
    price = route.gas + decoder.gas + revert_stub.charges(rows)
    steps = len(route.instructions) + len(decoder.instructions) + len(rows) - 1
    payload = revert_stub.error_data(b"zero assets")
    link = f"""
theorem flashLoanZeroAssets {decoder_proof.input_binders(decoder)}
    (noValue : env.value = 0)
    (selected : MIR.TransferFromDispatch.selector env.input = {route.stack[0]})
    (zero : Execution.readWord env.input 36 = 0#256)
    (work balance : Nat) :
    if balance < {price} then
      ∃ spent ≤ {steps},
        EVM.State.execute (work+1+{steps}) tvWorldConfig journal (dispatchFrame env balance 0 []) =
          .ok ⟨.outOfGas,[],journal,0,spent⟩
    else
      EVM.State.execute (work+1+{steps}) tvWorldConfig journal (dispatchFrame env balance 0 []) =
        .ok ⟨.reverted,zeroAssetsData,journal,balance-{price},{steps}+1⟩ := by
  have abi := metered_abi env journal surplus room small offsetWord slice addresses noValue selected
  rw [show readWord env.input 36 = (0 : Word) from zero] at abi
  have path := abi.append (zeroAssets env journal)
  by_cases short : balance < {price}
  · simp only [short, ite_true]
    exact path.rollbackOutOfGas (work+1) balance short
  · simp only [short, ite_false]
    have runPrefix := path.paid (work+1) balance (Nat.le_of_not_gt short)
    simp only [decoderFrame,dispatchFrame,Nat.reduceAdd] at runPrefix
    have finish := zeroAssetsReturns env journal work (balance-{price})
    simp only [decoderFrame] at finish
    simp only [EVM.State.execute,dispatchFrame,runPrefix,finish,Except.bind,Bind.bind,
      Pure.pure,Except.pure]
    rfl

#print axioms flashLoanZeroAssets
"""
    reference = revert_stub.certify(
        kernel, module, namespace, "zeroAssets", rows, payload, link, timeout
    )
    return {"proof": reference, "gas": price}
