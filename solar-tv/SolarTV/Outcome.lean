/-! Observable termination and refinement for the supported, gas-independent fragment.
A semantic Panic(uint256) has a fixed four-byte selector and one ABI word, so its
code identifies the complete payload and cannot alias the empty revert payload.
`invalid` is an exceptional halt, distinct from both forms of revert. Unsupported
IR and checker limits are host-side failures, never execution outcomes.
-/

namespace SolarTV

inductive RevertData where
  | empty
  | panic (code : BitVec 256)
  deriving DecidableEq

inductive Outcome (α : Type) where
  | returned (value : α)
  | reverted (data : RevertData)
  | invalid
  deriving DecidableEq

/-- Continue only after successful execution. Later failures cannot overwrite the first one. -/
def Outcome.bind (result : Outcome α) (next : α → Outcome β) : Outcome β :=
  match result with
  | .returned value => next value
  | .reverted data => .reverted data
  | .invalid => .invalid

@[simp] theorem Outcome.bind_return (result : Outcome α) :
    result.bind Outcome.returned = result := by
  cases result <;> rfl

@[simp] theorem Outcome.bind_returned (value : α) (next : α → Outcome β) :
    (Outcome.returned value).bind next = next value := rfl

@[simp] theorem Outcome.bind_reverted (data : RevertData) (next : α → Outcome β) :
    (Outcome.reverted data).bind next = Outcome.reverted data := rfl

@[simp] theorem Outcome.bind_invalid (next : α → Outcome β) :
    Outcome.invalid.bind next = Outcome.invalid := rfl

@[simp] theorem Outcome.bind_ite (condition : Prop) [Decidable condition]
    (yes no : Outcome α) (next : α → Outcome β) :
    (if condition then yes else no).bind next =
      if condition then yes.bind next else no.bind next := by
  split <;> rfl

/-- A behavior records the outcomes a program permits at a fixed input.
This allows future nondeterministic semantics without equating failure with undefinedness. -/
abbrev Behavior (α : Type) := Outcome α → Prop

def Behavior.exact (result : Outcome α) : Behavior α := fun observed => observed = result

/-- A candidate may produce only outcomes permitted by the original program. -/
def Behavior.refines (original candidate : Behavior α) : Prop :=
  ∀ observed, candidate observed → original observed

/-- The current importer emits deterministic, defined executions. -/
def refines (original candidate : Outcome α) : Prop :=
  Behavior.refines (Behavior.exact original) (Behavior.exact candidate)

@[simp] theorem refines_iff_eq (original candidate : Outcome α) :
    refines original candidate ↔ original = candidate := by
  constructor
  · intro h
    exact (h candidate rfl).symm
  · intro h
    subst candidate
    exact fun _ observed => observed

theorem refinement_transitive (a b c : Behavior α)
    (ab : Behavior.refines a b) (bc : Behavior.refines b c) : Behavior.refines a c :=
  fun result hc => ab result (bc result hc)

/-- A bit-vector projection used only to ask the solver for candidate witnesses.
The importer bounds return widths by 256. Every proposed input is then checked
against the actual refinement proposition, independently of this projection. -/
def Outcome.code (result : Outcome (BitVec width)) : BitVec 258 :=
  match result with
  | .returned value => value.setWidth 258
  | .reverted .empty => 1#258 <<< 256
  | .reverted (.panic code) => (2#258 <<< 256) ||| code.setWidth 258
  | .invalid => 3#258 <<< 256

@[simp] theorem Outcome.code_returned (value : BitVec width) :
    Outcome.code (.returned value) = value.setWidth 258 := rfl

@[simp] theorem Outcome.code_empty :
    Outcome.code (Outcome.reverted .empty : Outcome (BitVec width)) = 1#258 <<< 256 := rfl

@[simp] theorem Outcome.code_panic (code : BitVec 256) :
    Outcome.code (Outcome.reverted (.panic code) : Outcome (BitVec width)) =
      (2#258 <<< 256) ||| code.setWidth 258 := rfl

@[simp] theorem Outcome.code_invalid :
    Outcome.code (Outcome.invalid : Outcome (BitVec width)) = 3#258 <<< 256 := rfl

@[simp] theorem Outcome.code_cond (condition : Bool) (yes no : Outcome (BitVec width)) :
    Outcome.code (bif condition then yes else no) =
      bif condition then Outcome.code yes else Outcome.code no := by
  cases condition <;> rfl

@[simp] theorem Outcome.code_ite (condition : Prop) [Decidable condition]
    (yes no : Outcome (BitVec width)) :
    Outcome.code (if condition then yes else no) =
      if condition then Outcome.code yes else Outcome.code no := by
  split <;> rfl

/-- Render a concrete observation without erasing the reason execution failed. -/
def Outcome.describe (result : Outcome (BitVec width)) : String :=
  match result with
  | .returned value => toString value.toNat
  | .reverted .empty => "revert(empty)"
  | .reverted (.panic code) => s!"revert Panic({code.toNat})"
  | .invalid => "invalid"

end SolarTV
