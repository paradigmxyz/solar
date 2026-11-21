"""Certify the emitted revert when a caller's market is missing."""

from itertools import batched
from pathlib import Path

from . import decoder_proof, entry_proof, revert_stub


def certify(kernel, proof, runtime, decoded, catalog, market, decoder, route, timeout):
    module = Path(proof).stem
    namespace = module.removesuffix("Market")
    guard = market.guard
    rows = revert_stub.plan(
        runtime, decoded, catalog, guard.missing, guard.stack, market.memory
    )
    detail = b"market not created"
    payload = (
        bytes.fromhex("08c379a0")
        + (32).to_bytes(32, "big")
        + len(detail).to_bytes(32, "big")
    )
    payload += detail.ljust(32, b"\0")
    fragments = revert_stub.fragments(rows, "missingStub", payload)
    steps = (
        len(route.instructions)
        + len(decoder.instructions)
        + len(market.instructions)
        + len(guard.instructions)
        + len(rows)
        - 1
    )
    abi_gas = route.gas + decoder.gas
    tail = sum(row.inst.schema[1] for row in rows[:-1])
    price = f"{abi_gas} + (marketGas env journal + {guard.gas + tail})"
    link = f"""
theorem marketNotCreated {decoder_proof.input_binders(decoder)}
    (noValue : env.value = 0)
    (selected : MIR.TransferFromDispatch.selector env.input = {route.stack[0]})
    (missing : marketMissing (marketValue env journal))
    (work balance : Nat) :
    if balance < {price} then
      ∃ spent ≤ {steps},
        EVM.State.execute (work+1+{steps}) tvWorldConfig journal (dispatchFrame env balance 0 []) =
          .ok ⟨.outOfGas,[],journal,0,spent⟩
    else
      EVM.State.execute (work+1+{steps}) tvWorldConfig journal (dispatchFrame env balance 0 []) =
        .ok ⟨.reverted,missingStubData,journal,balance-({price}),{steps}+1⟩ := by
  let charged := warmSlot journal env.address (marketSlot env)
  have selectedGuard := metered_market_guard env journal surplus (marketValue env journal)
  have rejected : (if marketMissing (marketValue env journal) then {guard.missing} else {guard.present}) =
      {guard.missing} := by split <;> simp_all
  rw [rejected] at selectedGuard
  have metered := (metered_abi env journal surplus room small offsetWord slice addresses noValue selected).append
    ((metered_market env journal surplus).append (selectedGuard.append (missingStub env charged)))
  by_cases short : balance < {price}
  · simp only [short, ite_true]
    exact metered.rollbackOutOfGas (work+1) balance short
  · simp only [short, ite_false]
    have runPrefix := metered.paid (work+1) balance (Nat.le_of_not_gt short)
    simp only [decoderFrame,dispatchFrame,Nat.reduceAdd] at runPrefix
    have finish := missingStubReturns env charged work (balance-({price}))
    simp only [decoderFrame,marketGas] at finish
    simp only [EVM.State.execute,dispatchFrame,runPrefix,finish,Except.bind,Bind.bind,
      Pure.pure,Except.pure]
    rfl

#print axioms marketNotCreated
"""
    for i, body in enumerate([*map("".join, batched(fragments, 7)), link]):
        name = f"{namespace}MarketReject{i}"
        source = (
            entry_proof.import_modules([module])
            + entry_proof.HEADER
            + "open SolarTV.Execution MIR.TransferFromABI MIR.TransferFromMarket\n"
            + "set_option linter.unusedVariables false\n"
            + "set_option linter.unusedSimpArgs false\n"
            + body
        )
        result = entry_proof.lean(
            kernel, name, source, timeout, module=True, namespace=namespace
        )
        output = entry_proof.success(result, name)
        entry_proof.check_axioms(output.splitlines(), body.count("#print axioms"))
        module = name
    return {"proof": f"kernel/{module}.lean", "gas_after_guard": tail}
