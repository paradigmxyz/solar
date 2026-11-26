import SolarTV.EVMIR.Suffix

/-! Logical interaction refinement for instructions reordered within a block.

The checked stack interpreter retains each effect with its consumed operands.
A shared handler may fail or update arbitrary mutable state. Scalar and
transfer charges are omitted, and callers supply sufficient funding.
An optional resolved abort path is executed before observing its revert
operands. Only at that terminal observation is the rest of its stack private.
-/

namespace SolarTV.EVM.StackSchedule
open EvmRules

inductive Exit where
  | live (exit : Effects.Exit)
  | revert (label : Nat) (offset size : Word)
  deriving DecidableEq

inductive Trace where
  | fault
  | done (exit : Exit)
  | request (effect : Effects.Request) (next : Word → Trace)

def project (finish : Effects.Exit → Trace) : Effects.Tree → Trace
  | .fault => .fault
  | .done _ exit => finish exit
  | .request _ effect next => .request effect fun reply => project finish (next reply)

@[simp] theorem project_ite (finish : Effects.Exit → Trace)
    (condition : Prop) [Decidable condition] (yes no : Effects.Tree) :
    project finish (if condition then yes else no) =
      if condition then project finish yes else project finish no := by
  split <;> rfl

theorem add_left_comm (a b c : Word) : a + (b + c) = b + (a + c) := by
  rw [← BitVec.add_assoc, BitVec.add_comm a b, BitVec.add_assoc]

def revertExit (label : Nat) : Effects.Exit → Trace
  | .continuation (offset :: size :: _) => .done (.revert label offset size)
  | _ => .fault

def trace (ctx : Context) (fork : Fork) (aborts : Nat → Option (List Effects.Op))
    (ops : List Effects.Op) (stack : Stack) : Trace :=
  project (fun exit =>
    match exit with
    | .jump label words =>
      match aborts label with
      | some body => project (revertExit label) (Effects.expand ctx fork body words 0)
      | none => .done (.live exit)
    | .continuation _ => .done (.live exit)) (Effects.expand ctx fork ops stack 0)

/-- Source stack faults impose no outcome requirement. Any matched request
keeps its complete argument list and every possible response. -/
def Refines (after before : Trace) : Prop :=
  match after, before with
  | _, .fault => True
  | .done exit, .done original => exit = original
  | .request effect next, .request original rest =>
    effect = original ∧ ∀ reply, Refines (next reply) (rest reply)
  | _, _ => False

@[simp] theorem refines_ite (condition : Prop) [Decidable condition] (a b c d : Trace) :
    Refines (if condition then a else b) (if condition then c else d) ↔
      if condition then Refines a c else Refines b d := by
  split <;> rfl

abbrev Handler (World : Type) := Effects.Request → World → Option (Word × World)

def execute (handler : Handler World) : Trace → World → Option (Exit × World)
  | .fault, _ => none
  | .done exit, world => some (exit, world)
  | .request effect next, world => do
    let (reply, world) ← handler effect world
    execute handler (next reply) world

theorem effects (handler : Handler World) (before after : Trace)
    (same : Refines after before) (world : World)
    (result : Exit × World) (ran : execute handler before world = some result) :
    execute handler after world = some result := by
  induction before generalizing after world with
  | fault => simp [execute] at ran
  | done exit => cases after <;> simp_all [Refines, execute]
  | request effect rest ih =>
    cases after with
    | fault => simp [Refines] at same
    | done _ => simp [Refines] at same
    | request other next =>
      obtain ⟨equal, children⟩ := same
      subst other
      cases reply : handler effect world with
      | none => simp [execute, reply] at ran
      | some pair =>
        obtain ⟨word, middle⟩ := pair
        simp only [execute, reply] at ran ⊢
        exact ih word (next word) (children word) middle ran

inductive Outcome (World Halt : Type) where
  | stopped (halt : Halt)
  | fault
  | done (exit : Exit) (world : World)

def observe (enter : List String → Suffix.PrefixResult World Halt)
    (handler : Handler World) (fork : Fork) (common : List String)
    (aborts : Nat → Option (List Effects.Op)) (ops : List Effects.Op) : Outcome World Halt :=
  match enter common with
  | .stopped halt => .stopped halt
  | .next ctx stack _ world =>
    match execute handler (trace ctx fork aborts ops stack) world with
    | none => .fault
    | some (exit, final) => .done exit final

theorem contextual (enter : List String → Suffix.PrefixResult World Halt)
    (handler : Handler World) (fork : Fork) (common : List String)
    (aborts : Nat → Option (List Effects.Op)) (before after : List Effects.Op)
    (supported : Stack → Prop)
    (binding : ∀ ctx stack gas world, enter common = .next ctx stack gas world → supported stack)
    (same : ∀ ctx stack, supported stack →
      Refines (trace ctx fork aborts after stack) (trace ctx fork aborts before stack))
    (value : Exit) (final : World)
    (ran : observe enter handler fork common aborts before = .done value final) :
    observe enter handler fork common aborts after = .done value final := by
  cases entry : enter common with
  | stopped halt => simp [observe, entry] at ran
  | next ctx stack gas world =>
    have fits := binding ctx stack gas world entry
    simp only [observe, entry] at ran ⊢
    cases run : execute handler (trace ctx fork aborts before stack) world with
    | none => simp [run] at ran
    | some result =>
      have next := effects handler _ _ (same ctx stack fits) world result run
      simpa only [run, next] using ran

end SolarTV.EVM.StackSchedule
