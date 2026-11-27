"""Compose checked EVM debits into an exact paid or exhausted prefix."""

from dataclasses import dataclass


@dataclass
class Debit:
    name: str
    pc: int
    opcode: int
    gas: int | str
    after: str
    world: str = "journal"


def debit(
    name,
    binders,
    before,
    after,
    opcode,
    charge,
    *,
    current="journal",
    next_world="journal",
):
    return f"""
theorem {name}Debit {binders} (balance : Nat) :
    EVM.State.instructionWithImage tvWorldConfig {current}
      {{({before}) with gas := balance}} code.size {opcode}
      (code.immediate ({before}).pc) (fun target => (code.jumps 0).contains target.toNat) =
    if balance < ({charge}) then EVM.State.stepExhausted {current}
    else .ok (.next {next_world} {{({after}) with gas := balance-({charge})}}) := by
"""


def paid_step(
    step,
    binders,
    args,
    helper,
    fuel,
    before,
    after,
    pcs,
    balance,
    left,
    frame_definition,
    *,
    current="journal",
):
    proof = (
        "dispatchNext _ _ _ _ _ _ _ _"
        if step.world == current
        else "ordinaryNext _ _ _ _ _ _ _ _ _"
    )
    return f"""
theorem {step.name} {binders} :
    chunkedByteStep {helper} tvWorldConfig code Keccak.hash {fuel} ⟨{before},{current},[],[],{pcs}⟩ =
      .ok (.inr ⟨{after},{step.world},[],[],{[step.pc, *pcs]}⟩) := by
  refine {proof} {step.opcode} ?_ ?_ (by decide) ?_
  · change {step.pc} ≠ {helper}
    decide
  · change code.get? {step.pc} = some {step.opcode}
    decide +kernel
  have paid : ¬ ({balance}) < ({step.gas}) := by omega
  have spent : ({balance})-({step.gas}) = {left} := by omega
  have debited := {step.name}Debit {args} ({balance})
  simp only [{frame_definition}, Nat.add_zero] at paid spent debited ⊢
  simp only [paid, ite_false, spent] at debited
  exact debited
"""


def certify(name, binders, args, total, before, after, debits, *, current="journal"):
    out = f"""
theorem {name} {binders} :
    EVM.State.MeteredPrefix tvWorldConfig {current} ({before}) {debits[-1].world}
      ({after}) {len(debits)} ({total}) := by
"""
    if isinstance(total, int):
        fees = [total]
        for step in debits:
            fees.append(fees[-1] - step.gas)
        assert fees[-1] == 0
    else:
        fees = ["0"]
        for step in reversed(debits):
            fees.append(f"({step.gas}) + ({fees[-1]})")
        fees.reverse()
        out += f"  rw [show ({total}) = {fees[0]} by omega]\n"
    for index, step in enumerate(debits):
        out += f"""  refine .step (nextWorld := {step.world}) (after := ({step.after}))
    (count := {len(debits) - index - 1}) (rest := ({fees[index + 1]})) (charge := ({step.gas})) ?_ ?_
  · refine EVM.State.CodeChunks.meteredStep _ _ _ code _ _ {step.opcode} ({step.gas}) rfl ?_
      ({step.name}Debit {args})
    change code.get? {step.pc} = some {step.opcode}
    decide +kernel
"""
    return out + "  exact .done _ _\n"


def abi_failure(binders, args, start, end, instructions, cost, selector="selector"):
    return f"""
theorem metered_abi {binders}
    (noValue : env.value = 0)
    (selected : MIR.TransferFromDispatch.selector env.input = {selector}) :
    EVM.State.MeteredPrefix tvWorldConfig journal ({start}) journal ({end})
      {instructions} {cost} :=
  (metered_dispatch env journal surplus (by omega) small noValue selected).append (metered_decoder {args})

theorem abi_prefix_out_of_gas {binders}
    (noValue : env.value = 0)
    (selected : MIR.TransferFromDispatch.selector env.input = {selector})
    (work balance : Nat) (short : balance < {cost}) :
    ∃ spent ≤ {instructions},
      EVM.State.execute (work+{instructions}) tvWorldConfig journal
        (dispatchFrame env balance 0 []) = .ok ⟨.outOfGas,[],journal,0,spent⟩ := by
  exact (metered_abi {args} noValue selected).rollbackOutOfGas work balance short

#print axioms abi_prefix_out_of_gas
"""
