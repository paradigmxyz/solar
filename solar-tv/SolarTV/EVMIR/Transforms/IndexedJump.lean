import SolarTV.EVMIR.Ops

/-! Check the source semantics of a literal PUSH followed by an indexed
jump. The selector is consumed and the remaining stack survives unchanged.
The combined stack peak includes the literal word and the lowering's
temporary table operands. Physical table bytes and gas are separate. -/

namespace SolarTV.EVM.IndexedJump
open EvmRules

inductive Failure where
  | overflow
  | missing

def target (targets : List Nat) (selector : Word) : Option Nat :=
  targets[selector.toNat]?

def transfer (growth : Nat) (targets : List Nat) (stack : Stack) :
    Except Failure (Nat × Stack) :=
  if stack.length + growth > 1024 then .error .overflow else
  match stack with
  | [] => .error .missing
  | selector :: tail => match target targets selector with
    | none => .error .missing
    | some next => .ok (next, tail)

def pushAndTransfer (growth : Nat) (targets : List Nat) (selector : Word)
    (stack : Stack) : Except Failure (Nat × Stack) :=
  if stack.length + 1 > 1024 then .error .overflow
  else transfer growth targets (selector :: stack)

/-- The selected edge checks exactly the combined source stack admission
and preserves every live word, including continuation labels. -/
theorem literal_transfer (growth : Nat) (targets : List Nat) (selector : Word)
    (stack : Stack) (next : Nat) (selected : target targets selector = some next) :
    pushAndTransfer growth targets selector stack =
      if stack.length + (1 + growth) > 1024 then .error .overflow else .ok (next, stack) := by
  by_cases combined : stack.length + (1 + growth) > 1024
  · by_cases push : stack.length + 1 > 1024
    · simp [pushAndTransfer, push, combined]
    · have table : stack.length + 1 + growth > 1024 := by omega
      simp [pushAndTransfer, push, transfer, combined, table]
  · have push : ¬ stack.length + 1 > 1024 := by omega
    have table : ¬ stack.length + 1 + growth > 1024 := by omega
    simp [pushAndTransfer, push, transfer, combined, selected, table]

end SolarTV.EVM.IndexedJump
