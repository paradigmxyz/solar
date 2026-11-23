import SolarTV.MIR.State.Interpreter

/-! Path-sensitive comparison of typed MIR graphs. The importer resolves phi
inputs for each predecessor and keeps each phi group as a parallel assignment.
A point pairs two source cursors and records which locals must agree there.
Every matched action reads equal values and preserves the agreement required
by each successor. Sharing a source tail does not require the other tail to
use the same local IDs. Empty forwarding chains consume no logical work. -/

namespace SolarTV.MIR.CfgFlow
open EvmRules

inductive ActionKey where
  | opaque (id : Nat)
  | addConstant (value : Word)
  | subConstant (value : Word)
  deriving DecidableEq

def ActionKey.normalized : ActionKey → ActionKey
  | .subConstant constant => .addConstant (-constant)
  | key => key

def ActionKey.wordResult (key : ActionKey) (input : Word) : Option Word :=
  match key with
  | .opaque _ => none
  | .addConstant constant => some (MIR.eval .add input constant)
  | .subConstant constant => some (MIR.eval .sub input constant)

/-- The normalized key retains the result of the actual wrapped word
operation for every scalar operand. -/
theorem normalized_word_result (key : ActionKey) (input : Word) :
    key.normalized.wordResult input = key.wordResult input := by
  cases key <;> simp [ActionKey.normalized, ActionKey.wordResult, MIR.eval,
    BitVec.sub_eq_add_neg]

structure Action where
  key : ActionKey
  reads : List Nat
  writes : List Nat
  targets : List Nat

inductive Node where
  | action (action : Action)
  | forward (next : Nat)
  | literalBranch (condition : Bool) (yes no : Nat)

abbrev Graph := Array Node
abbrev Locals (Value : Type) := Nat → Value
abbrev Live := List (Nat × Nat)

structure Point where
  left : Nat
  right : Nat
  live : Live
  targets : List Nat

def follow (graph : Graph) : Nat → Nat → Option Action
  | 0, _ => none
  | fuel+1, point => do
    match ← graph[point]? with
    | .forward next => follow graph fuel next
    | .literalBranch condition yes no => follow graph fuel (if condition then yes else no)
    | .action action => some action

def write (values reply : Locals Value) (writes : List Nat) : Locals Value :=
  fun id => (writes.idxOf? id).elim (values id) reply

def agrees (live : Live) (a b : Locals Value) : Prop :=
  ∀ pair ∈ live, a pair.1 = b pair.2

def transferred (live : Live) (a b : List Nat) (pair : Nat × Nat) : Bool :=
  match a.idxOf? pair.1, b.idxOf? pair.2 with
  | none, none => live.contains pair
  | some i, some j => i == j
  | _, _ => false

def edge (points : Array Point) (live : Live) (a b : Action)
    (target : Nat) (positions : Nat × Nat) : Bool :=
  match points[target]? with
  | none => false
  | some next => next.left == positions.1 && next.right == positions.2 &&
      next.live.all (transferred live a.writes b.writes)

def compatible (points : Array Point) (point : Point) (a b : Action) : Bool :=
  decide (a.key.normalized = b.key.normalized) && a.reads.length == b.reads.length &&
  (a.reads.zip b.reads).all point.live.contains &&
  a.targets.length == point.targets.length && b.targets.length == point.targets.length &&
  (point.targets.zip (a.targets.zip b.targets)).all (fun (i, positions) =>
    edge points point.live a b i positions)

def checkAt (before after : Graph) (points : Array Point) (limit : Nat) (point : Point) : Bool :=
  match follow before limit point.left, follow after limit point.right with
  | some a, some b => compatible points point a b
  | _, _ => false

def Valid (before after : Graph) (points : Array Point) (limit : Nat) : Prop :=
  points.all (checkAt before after points limit) = true

theorem append_checked {a b : Array Point} {check : Point → Bool}
    (left : a.all check = true) (right : b.all check = true) :
    (a ++ b).all check = true := by
  simp only [Array.all_append, left, right, Bool.true_and]

/-- A handler reads the ordered typed operands and reports a halt or a
successor arm, result values by position, and the new shared state. -/
abbrev Handler (Value State Observation : Type) :=
  ActionKey → List Value → State → Except Observation (Nat × Locals Value × State)

inductive Result (Observation : Type) where
  | halted (terminal : Observation)
  | exhausted
  | invalid

def run (graph : Graph) (limit : Nat) (handle : Handler Value World Observation) :
    Nat → Nat → Locals Value → World → Result Observation
  | 0, _, _, _ => .exhausted
  | work+1, cursor, locals, world =>
    match follow graph limit cursor with
    | none => .invalid
    | some action =>
      match handle action.key.normalized (action.reads.map locals) world with
      | .error terminal => .halted terminal
      | .ok (arm,reply,next) =>
        match action.targets[arm]? with
        | none => .invalid
        | some successor => run graph limit handle work successor
            (write locals reply action.writes) next

theorem map_agrees (live : Live) (a b : Locals Value) (left right : List Nat)
    (lengths : left.length = right.length)
    (reads : (left.zip right).all live.contains = true) (same : agrees live a b) :
    left.map a = right.map b := by
  induction left generalizing right with
  | nil => cases right <;> simp_all
  | cons l ls ih =>
    cases right with
    | nil => simp at lengths
    | cons r rs =>
      simp only [List.length_cons, Nat.add_right_cancel_iff] at lengths
      simp only [List.zip_cons_cons, List.all_cons, Bool.and_eq_true,
        List.contains_iff_mem] at reads
      simp [same (l,r) reads.1, ih rs lengths reads.2]

theorem write_agrees (live next : Live) (old new reply : Locals Value) (a b : List Nat)
    (safe : next.all (transferred live a b) = true) (same : agrees live old new) :
    agrees next (write old reply a) (write new reply b) := by
  rintro ⟨l,r⟩ member
  have checked := List.all_eq_true.mp safe (l,r) member
  cases left : a.idxOf? l <;> cases right : b.idxOf? r <;>
    simp [transferred,left,right] at checked
  · simpa [write,left,right] using same (l,r) checked
  · simp [write,left,right,checked]

/-- Every checked pair of cursors has the same observation for every logical
work bound. The two local environments need agree only on the live pairs at
that point. Loops use the same invariant as the other successors. -/
theorem simulation (before after : Graph) (points : Array Point) (limit : Nat)
    (valid : Valid before after points limit) (handle : Handler Value World Observation)
    (work i : Nat) (point : Point) (found : points[i]? = some point)
    (old new : Locals Value) (world : World) (same : agrees point.live old new) :
    run before limit handle work point.left old world =
      run after limit handle work point.right new world := by
  induction work generalizing i point old new world with
  | zero => rfl
  | succ work ih =>
    have checked := Array.all_eq_true_iff_forall_mem.mp valid point
      (Array.mem_of_getElem? found)
    cases left : follow before limit point.left with
    | none => simp [checkAt,left] at checked
    | some a =>
      cases right : follow after limit point.right with
      | none => simp [checkAt,left,right] at checked
      | some b =>
        simp only [checkAt,left,right,compatible,Bool.and_eq_true,beq_iff_eq,
          decide_eq_true_eq] at checked
        obtain ⟨⟨⟨⟨⟨key,readsLength⟩,reads⟩,leftLength⟩,rightLength⟩,edges⟩ := checked
        have operands := map_agrees point.live old new a.reads b.reads readsLength reads same
        simp only [run,left,right,key,operands]
        cases executed : handle b.key.normalized (b.reads.map new) world with
        | error obs => rfl
        | ok result =>
          obtain ⟨arm,reply,state⟩ := result
          by_cases reaches : arm < point.targets.length
          · have aInside : arm < a.targets.length := by omega
            have bInside : arm < b.targets.length := by omega
            let target := point.targets[arm]
            let l := a.targets[arm]
            let r := b.targets[arm]
            have member : (target,(l,r)) ∈ point.targets.zip (a.targets.zip b.targets) := by
              apply List.mem_of_getElem? (i := arm)
              simp [target,l,r,List.getElem?_zip_eq_some, List.getElem?_eq_getElem reaches,
                List.getElem?_eq_getElem aInside, List.getElem?_eq_getElem bInside]
            have safe := List.all_eq_true.mp edges (target,(l,r)) member
            cases next : points[target]? with
            | none => simp [edge,next] at safe
            | some nextPoint =>
              simp only [edge,next,Bool.and_eq_true,beq_iff_eq] at safe
              rcases safe with ⟨⟨lEq,rEq⟩,writes⟩
              simp only [List.getElem?_eq_getElem aInside, List.getElem?_eq_getElem bInside]
              change run before limit handle work l (write old reply a.writes) state =
                run after limit handle work r (write new reply b.writes) state
              rw [← lEq, ← rEq]
              exact ih target nextPoint next (write old reply a.writes) (write new reply b.writes)
                state (write_agrees point.live nextPoint.live old new reply a.writes b.writes writes same)
          · simp [List.getElem?_eq_none (show a.targets.length ≤ arm by omega),
              List.getElem?_eq_none (show b.targets.length ≤ arm by omega)]

/-- Moving across an actual typed MIR unconditional jump changes only the
cursor and its predecessor. Phi entry remains a separate operation. -/
theorem typed_jump (cfg : State.Config) (functions : State.Functions) (machine : State.Machine)
    (body : State.Function) (block : State.Block) (target fuel : Nat)
    (resolved : State.function functions machine.cursor.function = .ok body)
    (selected : body.blocks[machine.cursor.block]? = some block)
    (entered : machine.cursor.entered = true)
    (endOfBlock : block.instructions.length ≤ machine.cursor.next)
    (transfer : block.terminator = .jump target) :
    State.transition fuel cfg functions machine =
      .ok (.next 0 ⟨State.branch machine.cursor target,machine.stack,machine.frame⟩) := by
  cases machine with
  | mk cursor stack frame =>
    change cursor.entered = true at entered
    simp [State.transition,resolved,selected,State.required,entered,transfer,
      show ¬cursor.next < block.instructions.length from Nat.not_lt.mpr endOfBlock,
      State.next,Bind.bind,Except.bind]

theorem typed_enter_empty (cfg : State.Config) (functions : State.Functions)
    (machine : State.Machine) (body : State.Function) (block : State.Block) (fuel : Nat)
    (resolved : State.function functions machine.cursor.function = .ok body)
    (selected : body.blocks[machine.cursor.block]? = some block)
    (entered : machine.cursor.entered = false) (empty : block.phis = []) :
    State.transition fuel cfg functions machine =
      .ok (.next 0 ⟨{machine.cursor with entered := true},machine.stack,machine.frame⟩) := by
  cases machine with
  | mk cursor stack frame =>
    change cursor.entered = false at entered
    simp [State.transition,resolved,selected,State.required,entered,empty,
      State.enter,Bind.bind,Except.bind,Pure.pure,Except.pure]

/-- A literal i1 selects its successor and sets the predecessor just as a
direct jump does. Phi operands are selected when that successor is entered. -/
theorem typed_literal_branch (cfg : State.Config) (functions : State.Functions)
    (machine : State.Machine) (body : State.Function) (block : State.Block)
    (condition : Bool) (yes no fuel : Nat)
    (resolved : State.function functions machine.cursor.function = .ok body)
    (selected : body.blocks[machine.cursor.block]? = some block)
    (entered : machine.cursor.entered = true)
    (endOfBlock : block.instructions.length ≤ machine.cursor.next)
    (transfer : block.terminator =
      .branch ⟨.constant ⟨1, if condition then 1 else 0⟩, .word 1⟩ yes no) :
    State.transition fuel cfg functions machine = .ok (.next 0
      ⟨State.branch machine.cursor (if condition then yes else no),machine.stack,machine.frame⟩) := by
  cases machine with
  | mk cursor stack frame =>
    change cursor.entered = true at entered
    cases condition <;>
      simp [State.transition,resolved,selected,State.required,entered,transfer,
        show ¬cursor.next < block.instructions.length from Nat.not_lt.mpr endOfBlock,
        State.next,State.readBits,State.readValue,State.Value.fits,State.ValueType.width,
        Bind.bind,Except.bind,Pure.pure,Except.pure]

end SolarTV.MIR.CfgFlow
