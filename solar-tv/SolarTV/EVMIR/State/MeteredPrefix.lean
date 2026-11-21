import SolarTV.EVMIR.State.CodeChunks

/-! A bytecode prefix whose steps each check the exact debit. Insufficient
gas stops at the first unpaid instruction, before any of its effects. -/

namespace SolarTV.EVM.State
open EvmRules SolarTV.Execution

theorem CodeChunks.instruction_eq (cfg : Config) (journal : Journal) (code : CodeChunks)
    (frame : Frame) :
    instruction cfg journal {frame with code := code.bytes} =
    instructionWithImage cfg journal {frame with code := code.bytes}
      code.size ((code.get? frame.pc).getD 0) (code.immediate frame.pc)
      (fun target => (code.jumps 0).contains target.toNat) := by
  have immediate : code.immediate frame.pc = pushValue code.bytes frame.pc :=
    funext (code.immediate_eq frame.pc)
  simp only [instruction, CodeChunks.get_eq, immediate, CodeChunks.jump_eq,
    ← CodeChunks.size_eq]

inductive MeteredPrefix (cfg : Config) :
    Journal → Frame → Journal → Frame → Nat → Nat → Prop where
  | done (journal : Journal) (frame : Frame) : MeteredPrefix cfg journal frame journal frame 0 0
  | step {journal nextWorld lastWorld before after last count rest charge} :
      (∀ balance, instruction cfg journal {before with gas := balance} =
        if balance < charge then stepExhausted journal
        else .ok (.next nextWorld {after with gas := balance-charge})) →
      MeteredPrefix cfg nextWorld after lastWorld last count rest →
      MeteredPrefix cfg journal before lastWorld last (count+1) (charge+rest)

theorem CodeChunks.meteredStep (cfg : Config) (journal nextWorld : Journal)
    (code : CodeChunks) (before after : Frame) (opcode : Byte) (charge : Nat)
    (image : before.code = code.bytes) (found : code.get? before.pc = some opcode)
    (debit : ∀ balance, instructionWithImage cfg journal {before with gas := balance}
      code.size opcode (code.immediate before.pc) (fun target => (code.jumps 0).contains target.toNat) =
        if balance < charge then stepExhausted journal
        else .ok (.next nextWorld {after with gas := balance-charge})) :
    ∀ balance, instruction cfg journal {before with gas := balance} =
      if balance < charge then stepExhausted journal
      else .ok (.next nextWorld {after with gas := balance-charge}) := by
  intro balance
  have frame : ({before with gas := balance, code := code.bytes} : Frame) =
      {before with gas := balance} := by cases before; simp_all
  simpa only [frame, found, Option.getD_some] using
    (code.instruction_eq cfg journal {before with gas := balance}).trans (by
      simp only [frame, found, Option.getD_some]
      exact debit balance)

theorem MeteredPrefix.append {cfg : Config} {first middle last : Frame}
    {startWorld middleWorld lastWorld : Journal} {left right price fee : Nat}
    (head : MeteredPrefix cfg startWorld first middleWorld middle left price)
    (tail : MeteredPrefix cfg middleWorld middle lastWorld last right fee) :
    MeteredPrefix cfg startWorld first lastWorld last (left+right) (price+fee) := by
  induction head with
  | done => simpa using tail
  | step debit _ ih =>
    simpa only [Nat.succ_add, Nat.add_succ, Nat.add_zero, Nat.add_assoc] using
      MeteredPrefix.step debit (ih tail)

theorem MeteredPrefix.paid {cfg : Config} {before after : Frame} {journal nextWorld : Journal}
    {count price : Nat} (trace : MeteredPrefix cfg journal before nextWorld after count price)
    (work balance : Nat) (enough : price ≤ balance) :
    run (work+count) cfg journal {before with gas := balance} = (do
      let result ← run work cfg nextWorld {after with gas := balance-price}
      return {result with work := result.work+count}) := by
  induction trace generalizing balance with
  | done =>
    simp only [Nat.add_zero, Nat.sub_zero]
    cases run work cfg _ _ <;> rfl
  | @step journal nextWorld lastWorld before after last count rest charge debit _ ih =>
    have paid : ¬ balance < charge := by omega
    have still : rest ≤ balance-charge := by omega
    have cost : balance-charge-rest = balance-(charge+rest) := by omega
    rw [Nat.add_succ, run_next (work+count) (by simpa only [paid, ite_false] using debit balance),
      ih _ still, cost]
    cases run work cfg lastWorld {last with gas := balance-(charge+rest)} <;>
      simp [Except.bind, Bind.bind, Except.pure, Pure.pure, Nat.add_assoc]

theorem MeteredPrefix.unpaid {cfg : Config} {before after : Frame} {journal nextWorld : Journal}
    {count price : Nat} (trace : MeteredPrefix cfg journal before nextWorld after count price)
    (work balance : Nat) (short : balance < price) :
    ∃ result, run (work+count) cfg journal {before with gas := balance} = .ok result ∧
      result.stop = .outOfGas ∧ result.data = [] ∧ result.gas = 0 ∧ result.work ≤ count := by
  induction trace generalizing balance with
  | done => omega
  | @step journal nextWorld lastWorld before after last count rest charge debit _ ih =>
    rw [Nat.add_succ]
    by_cases first : balance < charge
    · rw [run_done (work+count) (by simpa only [first, ite_true, stepExhausted] using debit balance)]
      exact ⟨_,rfl,rfl,rfl,rfl,by simp⟩
    · rw [run_next (work+count) (by simpa only [first, ite_false] using debit balance)]
      obtain ⟨result, executed, stopped, data, gas, steps⟩ := ih (balance-charge) (by omega)
      rw [executed]
      exact ⟨_,rfl,stopped,data,gas,by simp; omega⟩

theorem MeteredPrefix.rollbackOutOfGas {cfg : Config} {before after : Frame}
    {journal nextWorld : Journal} {count price : Nat}
    (trace : MeteredPrefix cfg journal before nextWorld after count price)
    (work balance : Nat) (short : balance < price) :
    ∃ spent ≤ count, execute (work+count) cfg journal {before with gas := balance} =
      .ok ⟨.outOfGas,[],journal,0,spent⟩ := by
  obtain ⟨result, executed, stopped, data, _, steps⟩ := trace.unpaid work balance short
  refine ⟨result.work,steps,?_⟩
  simp [execute, executed, stopped, data, Bind.bind, Except.bind, Pure.pure, Except.pure]

end SolarTV.EVM.State
