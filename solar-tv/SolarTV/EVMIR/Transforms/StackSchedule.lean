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

/-- Execute the projection without evaluating prices that will be erased.
The effect keys still include every operand and the storage sentry. -/
def expand (ctx : Context) (fork : Fork) (finish : Effects.Exit → Trace) :
    List Effects.Op → Stack → Trace
  | [], stack =>
    if stack.length > 1024 then .fault else finish (.continuation stack)
  | .scalar op :: ops, stack =>
    match stepChecked ctx op stack with
    | .error _ => .fault
    | .ok next => expand ctx fork finish ops next
  | .effect opcode inputs outputs :: ops, stack =>
    if stack.length < inputs || stack.length - inputs + outputs > 1024 || stack.length > 1024 then
      .fault
    else .request (Effects.request fork opcode inputs outputs stack) fun word =>
      expand ctx fork finish ops (if outputs == 1 then word :: stack.drop inputs else stack.drop inputs)
  | .guard label _ _ :: ops, condition :: rest =>
    if rest.length + 1 + 1 > 1024 then .fault else
      if condition = 0 then expand ctx fork finish ops rest else finish (.jump label rest)
  | .guard .. :: _, [] => .fault
  | .reserve scratch :: ops, stack =>
    if stack.length + scratch > 1024 then .fault else expand ctx fork finish ops stack

theorem project_expand (ctx : Context) (fork : Fork) (finish : Effects.Exit → Trace)
    (ops : List Effects.Op) (stack : Stack) (charge : Nat) :
    project finish (Effects.expand ctx fork ops stack charge) = expand ctx fork finish ops stack := by
  induction ops generalizing stack charge with
  | nil => simp only [Effects.expand,expand]; split <;> rfl
  | cons op ops ih =>
    cases op with
    | scalar operation =>
      cases stepped : stepChecked ctx operation stack with
      | error failure => simp [Effects.expand, expand, stepped, project]
      | ok next => simpa only [Effects.expand, expand, stepped] using ih next (charge + Gas.pureCost operation fork)
    | effect opcode inputs outputs =>
      simp only [Effects.expand, expand]
      split
      · rfl
      · simp only [project, ih]
    | guard label pushCharge jumpCharge =>
      cases stack with
      | nil => rfl
      | cons condition rest =>
        simp only [Effects.expand, expand]
        split
        · rfl
        · split
          · exact ih _ _
          · rfl
    | reserve scratch =>
      simp only [Effects.expand, expand]
      split
      · rfl
      · exact ih _ _

theorem expand_scalar {ctx : Context} {fork : Fork} {finish : Effects.Exit → Trace}
    {op : Op} {ops : List Effects.Op} {stack next : Stack}
    (checked : stepChecked ctx op stack = .ok next) :
    expand ctx fork finish (.scalar op :: ops) stack = expand ctx fork finish ops next := by
  simp only [expand, checked]

theorem step_fits {ctx : Context} {op : Op} {stack next : Stack}
    (stepped : step ctx op stack = some next) (before : stack.length ≤ 1024)
    (after : next.length ≤ 1024) : stepChecked ctx op stack = .ok next := by
  simp [stepChecked, stepped, Nat.not_lt_of_ge before, Nat.not_lt_of_ge after]

theorem expand_effect {ctx : Context} {fork : Fork} {finish : Effects.Exit → Trace}
    {opcode inputs outputs : Nat} {ops : List Effects.Op} {stack : Stack}
    (under : inputs ≤ stack.length) (room : stack.length - inputs + outputs ≤ 1024)
    (fits : stack.length ≤ 1024) :
    expand ctx fork finish (.effect opcode inputs outputs :: ops) stack =
      .request (Effects.request fork opcode inputs outputs stack) fun word =>
        expand ctx fork finish ops (if outputs == 1 then word :: stack.drop inputs else stack.drop inputs) := by
  simp [expand, Nat.not_lt_of_ge under, Nat.not_lt_of_ge room, Nat.not_lt_of_ge fits]

theorem expand_reserve {ctx : Context} {fork : Fork} {finish : Effects.Exit → Trace}
    {scratch : Nat} {ops : List Effects.Op} {stack : Stack} (fits : stack.length + scratch ≤ 1024) :
    expand ctx fork finish (.reserve scratch :: ops) stack = expand ctx fork finish ops stack := by
  simp [expand, Nat.not_lt_of_ge fits]

theorem expand_nil {ctx : Context} {fork : Fork} {finish : Effects.Exit → Trace}
    {stack : Stack} (fits : stack.length ≤ 1024) :
    expand ctx fork finish [] stack = finish (.continuation stack) := by
  simp [expand, Nat.not_lt_of_ge fits]

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
  expand ctx fork (fun exit =>
    match exit with
    | .jump label words =>
      match aborts label with
      | some body => expand ctx fork (revertExit label) body words
      | none => .done (.live exit)
    | .continuation _ => .done (.live exit)) ops stack

theorem trace_project (ctx : Context) (fork : Fork) (aborts : Nat → Option (List Effects.Op))
    (ops : List Effects.Op) (stack : Stack) :
    trace ctx fork aborts ops stack = project (fun exit =>
      match exit with
      | .jump label words =>
        match aborts label with
        | some body => project (revertExit label) (Effects.expand ctx fork body words 0)
        | none => .done (.live exit)
      | .continuation _ => .done (.live exit)) (Effects.expand ctx fork ops stack 0) := by
  simp only [trace, project_expand]

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
