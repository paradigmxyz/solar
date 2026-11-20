import SolarTV.MIR.Transforms.LowerAddressCall
import SolarTV.MIR.Transforms.LowerReturndata

/-! An implicit-gas static call with slice input and a cleared output word
lowers to the slice fields, a scratch store, GAS, and STATICCALL. The local
equivalence uses the budget at the message boundary after caller work. -/

namespace SolarTV.MIR.LowerStaticWord
open EvmRules SolarTV.Execution MIR.State
open MIR.LowerReturndata (invoke)

private theorem implicitBudget (cfg : Config) (frame : Frame) (ext : EVM.State.Config)
    (memory : List Byte) (message : Calls.Message) (resolution : Nat)
    (budgets : List Nat) (budget : Nat) (sample : Word)
    (ledger : cfg.messageGas = some budgets)
    (selected : budgets[frame.messageGas.length]? = some budget)
    (eip150 : cfg.fork.atLeast .tangerineWhistle = true)
    (cap : budget ≤ sample.toNat) :
    callBudget cfg frame ext memory message 0xfa resolution none =
      callBudget cfg frame ext memory message 0xfa resolution (some sample) := by
  simp only [callBudget, ledger, selected, required, Bind.bind, Except.bind,
    Option.map_none, Option.getD_none, Option.map_some, Option.getD_some]
  split
  · rfl
  · rw [LowerAddressCall.forwardSample _ _ _ _ eip150 (Nat.le_trans (Nat.sub_le ..) cap)]

private theorem implicitRequest (cfg : Config) (fuel : Nat) (frame : Frame)
    (target pointer length sample : Word) (budgets : List Nat) (budget : Nat)
    (ledger : cfg.messageGas = some budgets)
    (selected : budgets[frame.messageGas.length]? = some budget)
    (old : cfg.fork.atLeast .amsterdam = false)
    (eip150 : cfg.fork.atLeast .tangerineWhistle = true)
    (cap : budget ≤ sample.toNat) (limit : sample.toNat ≤ 30000000) :
    external fuel cfg frame 0xfa [target,pointer,length,0,32] none =
      external fuel cfg frame 0xfa [target,pointer,length,0,32] (some sample) := by
  cases bound : cfg.externalConfig with
  | none => simp [external, bound, required, Bind.bind, Except.bind]
  | some ext =>
    simp [external, bound, required, old, ledger, eip150, Nat.not_lt.mpr limit,
      Bind.bind, Except.bind, Pure.pure, Except.pure]
    simp only [implicitBudget cfg frame ext _ _ _ budgets budget sample ledger selected eip150 cap]

def lowered (cfg : Config) (fuel : Nat) (frame : Frame) (target pointer length : Word) : MessageStep := do
  match ← memoryOperation cfg frame .store [0,0] with
  | .returned (none,next) =>
    match ← layoutOperation cfg next .gasSample [] with
    | .returned (some sample,next) =>
      external fuel cfg next 0xfa [target,pointer,length,0,32] (some sample)
    | .invalid => return some (.invalid,0)
    | _ => .error "expected static-call gas sample"
  | _ => .error "expected static-call scratch clear"

theorem staticWord (cfg : Config) (fuel : Nat) (frame : Frame)
    (address input : Operand) (target : Word) (pointer length : Scalar)
    (ha : read frame address = .ok target)
    (hi : readValue frame input = .ok (.aggregate [.scalar pointer,.scalar length]))
    (available : cfg.available.contains 0xfa = true)
    (old : cfg.fork.atLeast .amsterdam = false)
    (eip150 : cfg.fork.atLeast .tangerineWhistle = true)
    (gasBefore budget : Nat) (budgets : List Nat)
    (sampled : cfg.layout.gasBefore = some gasBefore)
    (paid : 2 ≤ gasBefore)
    (cap : budget ≤ gasBefore-2) (limit : gasBefore-2 ≤ 30000000)
    (ledger : cfg.messageGas = some budgets)
    (selected : budgets[frame.messageGas.length]? = some budget) :
    invoke cfg fuel (messageArguments cfg frame (.address 0xfa false false true true true)
      [address,input]) = lowered cfg fuel frame target pointer.word length.word := by
  have present : (0xfa : Nat) ∈ cfg.available := List.contains_iff_mem.mp available
  simp [invoke, messageArguments, ha, hi, Value.word, present, lowered, memoryOperation,
    layoutOperation, sampled, Nat.not_lt.mpr paid, required, effect,
    Except.bind, Bind.bind, Except.map, Functor.map, Pure.pure, Except.pure]
  cases cleared : expand frame.memory 0 32 with
  | error _ => rfl
  | ok mem =>
    have sampleWord : (BitVec.ofNat 256 (gasBefore-2)).toNat = gasBefore-2 := by
      simp [BitVec.toNat_ofNat, Nat.mod_eq_of_lt (by omega : gasBefore-2 < 2^256)]
    exact implicitRequest cfg fuel {frame with memory := writeBytes mem 0 (wordBytes 0)}
      target pointer.word length.word (BitVec.ofNat 256 (gasBefore-2))
      budgets budget ledger selected old eip150
      (by simpa only [sampleWord] using cap) (by simpa only [sampleWord] using limit)

end SolarTV.MIR.LowerStaticWord
