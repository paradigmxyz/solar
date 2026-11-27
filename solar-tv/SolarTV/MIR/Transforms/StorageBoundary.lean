import SolarTV.EVMIR.State.Reentry
import SolarTV.MIR.State.Ops

/-! A physical SSTORE can be executed as a typed MIR store. Its gas sentry,
storage warmth and transaction-original refund belong to the frame's
metered continuation. The interpreter owns the enclosing call's rollback. -/

namespace SolarTV.MIR.StorageBoundary
open EvmRules SolarTV.Execution

def quote (ext : EVM.State.Config) (env : World.Environment) (journal : Journal)
    (slot word : Word) : Storage.Price :=
  let stored := account journal env.address
  let original := (journal.original.find? (·.address == env.address)).getD {address := env.address}
  Storage.writePrice ext.fork (journal.warmSlots.contains (env.address,slot))
    (slotsRead original.storage slot) (slotsRead stored.storage slot) word

def exhausted (ext : EVM.State.Config) (frame : EVM.State.Frame)
    (price : Storage.Price) : Bool :=
  (ext.fork.atLeast .istanbul && frame.gas ≤ 2300) || frame.gas < price.gas

def store (ext : EVM.State.Config) (journal : Journal) (frame : EVM.State.Frame)
    (slot word : Word) (rest : List Word) :
    Except String (EVM.State.Result ⊕ (Journal × EVM.State.Frame)) := do
  let price := quote ext frame.environment journal slot word
  if frame.environment.isStatic then
    return .inl ⟨.exception,[],journal,0,0⟩
  if exhausted ext frame price then
    return .inl ⟨.outOfGas,[],journal,0,0⟩
  let cfg : MIR.State.Config := ⟨ext.fork,frame.environment,ext.block,[0x55],false,
    128,some ext,[],{},[],false,none,false⟩
  let mir : MIR.State.Frame := {world := journal,memory := frame.memory,arguments := []}
  match ← MIR.State.storageOperation cfg mir false true [slot,word] with
  | .returned (none,next) =>
    return .inr ({next.world with refund := journal.refund + price.refund},
      {frame with pc := frame.pc+1, stack := rest, gas := frame.gas-price.gas})
  | _ => .error "expected typed storage update"

theorem store_eq (ext : EVM.State.Config) (journal : Journal) (frame : EVM.State.Frame)
    (slot word : Word) (rest : List Word) (work : Nat)
    (opcode : frame.code[frame.pc]? = some 0x55)
    {base dynamic : Nat} (schema : EVM.State.available ext 0x55 = ⟨true,base,dynamic,2,0⟩)
    (old : ext.fork.atLeast .amsterdam = false)
    (operands : frame.stack = slot :: word :: rest)
    (space : rest.length ≤ 1024) :
    (do
      let signal ← EVM.State.instruction ext journal frame
      EVM.State.advance ext (EVM.State.run work ext) signal) =
    (store ext journal frame slot word rest).map (fun
      | .inl result => .inl result
      | .inr (next, frame) => .inr (0,next,frame)) := by
  obtain ⟨within,_⟩ := List.getElem?_eq_some_iff.mp opcode
  have stackBound : ¬ 1024 < rest.length := by omega
  have stackEnough : ¬ rest.length+1+1 < 2 := by omega
  by_cases static : frame.environment.isStatic = true
  · simp [EVM.State.instruction, EVM.State.instructionWithImage, opcode, schema, old,
      Nat.not_le_of_gt within, operands, stackBound, stackEnough, EVM.State.pureOp,
      static, EVM.State.advance, EVM.State.stepFailed, store, Except.bind, Bind.bind,
      Pure.pure, Except.pure, Except.map]
  by_cases sentry : ext.fork.atLeast .istanbul = true ∧ frame.gas ≤ 2300
  · simp [EVM.State.instruction, EVM.State.instructionWithImage, opcode, schema, old,
      Nat.not_le_of_gt within, operands, stackBound, stackEnough, EVM.State.pureOp,
      static, sentry, EVM.State.advance, EVM.State.stepExhausted, store, exhausted,
      Except.bind, Bind.bind, Pure.pure, Except.pure, Except.map]
  by_cases enough : frame.gas < (quote ext frame.environment journal slot word).gas
  all_goals
    simp [EVM.State.instruction, EVM.State.instructionWithImage, opcode, schema, old,
      Nat.not_le_of_gt within, operands, stackBound, stackEnough, EVM.State.pureOp,
      static, sentry, quote] at enough ⊢
    simp [static, sentry, enough, EVM.State.advance, EVM.State.stepExhausted,
      EVM.State.next, store, exhausted, quote, MIR.State.storageOperation, MIR.State.effect,
      Except.bind, Bind.bind, Pure.pure, Except.pure, Except.map, putAccount, warmSlot]
  simp [Nat.not_lt.mpr enough]

def resumeWith (ext : EVM.State.Config) (journal : Journal) (frame : EVM.State.Frame)
    (slot word : Word) (rest : List Word) (proceed : EVM.State.ChildExecutor) : EVM.State.Run := do
  let result ← match ← store ext journal frame slot word rest with
    | .inl result => pure result
    | .inr (journal,frame) => proceed journal frame
  return {result with work := result.work+1}

def resume (ext : EVM.State.Config) (journal : Journal) (frame : EVM.State.Frame)
    (slot word : Word) (rest : List Word) (work : Nat) : EVM.State.Run :=
  resumeWith ext journal frame slot word rest (EVM.State.run work ext)

theorem resume_eq (ext : EVM.State.Config) (journal : Journal) (frame : EVM.State.Frame)
    (slot word : Word) (rest : List Word) (work : Nat)
    (opcode : frame.code[frame.pc]? = some 0x55)
    {base dynamic : Nat} (schema : EVM.State.available ext 0x55 = ⟨true,base,dynamic,2,0⟩)
    (old : ext.fork.atLeast .amsterdam = false)
    (operands : frame.stack = slot :: word :: rest)
    (space : rest.length ≤ 1024) :
    resume ext journal frame slot word rest work =
      EVM.State.run (work+1) ext journal frame := by
  rw [EVM.State.run]
  have verified := store_eq ext journal frame slot word rest work opcode schema old operands space
  rw [← bind_assoc, verified]
  unfold resume resumeWith
  cases store ext journal frame slot word rest with
  | error _ => rfl
  | ok effect =>
    cases effect with
    | inl _ => rfl
    | inr effect =>
      obtain ⟨next,frame⟩ := effect
      cases EVM.State.run work ext next frame <;> rfl

end SolarTV.MIR.StorageBoundary
