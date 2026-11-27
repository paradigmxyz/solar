import SolarTV.MIR.Transforms.CfgFlow

/-! Compose checked MIR graphs across direct recursive calls. A frame
initializes its locals from the ordered argument values. A direct call
runs its known callee, then resumes with that callee's result. The checked
bodies match for any shared call entry and return handlers. Logical work
per frame and the number of unfolded calls are independent bounds. -/

namespace SolarTV.MIR.RecursiveCalls
open CfgFlow

structure Procedure where
  graph : Graph
  jumpLimit : Nat

structure ProcedurePair where
  id : Nat
  before : Procedure
  after : Procedure
  matched : ∀ {Value World Observation : Type} (handle : Handler Value World Observation)
    (work : Nat) (locals : Locals Value) (world : World),
    run before.graph before.jumpLimit handle work 0 locals world =
      run after.graph after.jumpLimit handle work 0 locals world

abbrev Recursion (Value World Observation : Type) :=
  Nat → List Value → World → Result Observation

structure Handlers (Value World Observation : Type) where
  ordinary : Nat → List Value → Handler Value World Observation
  operands : (caller callee signature : Nat) → (tail : Bool) →
    (parameters locals : List Value) → Except Observation (List Value)
  enter : (caller callee signature : Nat) → (tail : Bool) →
    List Value → World → Except Observation World
  resume : (caller callee signature : Nat) → (tail : Bool) →
    Result Observation → World → Except Observation (Nat × Locals Value × World)

def Handlers.dispatch (handlers : Handlers Value World Observation)
    (caller : Nat) (parameters : List Value) (recurse : Recursion Value World Observation) :
    Handler Value World Observation := fun key locals world =>
  match key with
  | .directCall callee signature tail => do
    let arguments ← handlers.operands caller callee signature tail parameters locals
    let frame ← handlers.enter caller callee signature tail arguments world
    handlers.resume caller callee signature tail (recurse callee arguments frame) world
  | _ => handlers.ordinary caller parameters key locals world

def runRecursive (procedures : Nat → Option Procedure) (handlers : Handlers Value World Observation)
    (argumentLocals : Nat → List Value → Locals Value) (work : Nat) :
    Nat → Recursion Value World Observation
  | 0, _, _, _ => .exhausted
  | callLimit+1, id, args, world =>
    match procedures id with
    | none => .invalid
    | some proc =>
      run proc.graph proc.jumpLimit
        (handlers.dispatch id args (runRecursive procedures handlers argumentLocals work callLimit))
        work 0 (argumentLocals id args) world

def procedures (pairs : List ProcedurePair) (after : Bool) (id : Nat) : Option Procedure :=
  (pairs.find? (·.id == id)).map (fun pair => if after then pair.after else pair.before)

/-- Relate each computed callee reply before the caller's next action.
Induction on the call bound covers mutually recursive functions. -/
theorem simulation (pairs : List ProcedurePair) (handlers : Handlers Value World Observation)
    (argumentLocals : Nat → List Value → Locals Value) (work callLimit id : Nat)
    (args : List Value) (state : World) :
    runRecursive (procedures pairs false) handlers argumentLocals work callLimit id args state =
      runRecursive (procedures pairs true) handlers argumentLocals work callLimit id args state := by
  induction callLimit generalizing id args state with
  | zero => rfl
  | succ callLimit ih =>
    have equalReplies :
        runRecursive (procedures pairs false) handlers argumentLocals work callLimit =
          runRecursive (procedures pairs true) handlers argumentLocals work callLimit := by
      funext target inputs world
      exact ih target inputs world
    cases found : pairs.find? (·.id == id) with
    | none => simp [runRecursive, procedures, found]
    | some pair =>
      simp only [runRecursive, procedures, found, Option.map_some, Bool.false_eq_true,
        ite_false, ite_true]
      rw [equalReplies]
      exact pair.matched _ _ _ _

end SolarTV.MIR.RecursiveCalls
