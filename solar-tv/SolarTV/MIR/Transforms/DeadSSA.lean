/-! Dead SSA definitions in cyclic control flow.

Actions expose all SSA reads and writes. Only total, state-independent actions
with dead results may disappear. Every retained action, phi and transfer must
avoid those results. Phi groups remain simultaneous and predecessor-sensitive.
The simulation preserves live registers and arbitrary shared state after every
block. Induction on block transfers covers cycles without assuming termination.
Handler failures stop immediately. Fuel counts blocks, not instructions or gas.
Source/schema binding and total-pure classification remain trusted; this is a
contextual stuttering proof, not opcode or backend-layout verification.
-/

namespace SolarTV.MIR.DeadSSA

structure Action where
  key : String
  reads : List Nat
  writes : List Nat
  silent : Bool
  deriving DecidableEq

structure Block where
  actions : List Action
  key : String
  reads : List Nat
  deriving DecidableEq

abbrev Registers (Value : Type) := Nat → Value

def agrees (dead : List Nat) (a b : Registers Value) : Prop :=
  ∀ id, id ∉ dead → a id = b id

def update (registers : Registers Value) (writes : List Nat)
    (reply : Registers Value) : Registers Value :=
  fun id => if id ∈ writes then reply id else registers id

def drops (dead : List Nat) (action : Action) : Bool :=
  action.silent && !action.writes.isEmpty && action.writes.all (dead.contains ·)

def safe (dead : List Nat) (block : Block) : Bool :=
  block.actions.all (fun action => drops dead action || action.reads.all (!dead.contains ·)) &&
    block.reads.all (!dead.contains ·)

def eraseBlock (dead : List Nat) (block : Block) : Block :=
  {block with actions := block.actions.filter (!drops dead ·)}

structure Handlers (Value State Observation : Type) where
  pure : String → Option Nat → List Value → Registers Value
  effect : String → Option Nat → List Value → State → Except Observation (Registers Value × State)
  transfer : String → Option Nat → List Value → State → Observation ⊕ Nat

def actions (handlers : Handlers Value State Observation) (predecessor : Option Nat) :
    List Action → Registers Value → State → Except Observation (Registers Value × State)
  | [], registers, state => .ok (registers,state)
  | action :: rest, registers, state => do
    let inputs := action.reads.map registers
    let (reply,next) ← if action.silent then
        pure (handlers.pure action.key predecessor inputs,state)
      else handlers.effect action.key predecessor inputs state
    actions handlers predecessor rest (update registers action.writes reply) next

inductive Result (Observation : Type) where
  | stopped (observation : Observation)
  | exhausted
  | missingBlock

/-- One logical fuel unit covers a complete block, retaining every effect. -/
def run (handlers : Handlers Value State Observation) (program : List Block) :
    Nat → Nat → Option Nat → Registers Value → State → Result Observation
  | 0, _, _, _, _ => .exhausted
  | fuel + 1, index, predecessor, registers, state =>
    match program[index]? with
    | none => .missingBlock
    | some block =>
      match actions handlers predecessor block.actions registers state with
      | .error failure => .stopped failure
      | .ok (registers,state) =>
        match handlers.transfer block.key predecessor (block.reads.map registers) state with
        | .inl observation => .stopped observation
        | .inr next => run handlers program fuel next (some index) registers state

theorem reads_agree (dead ids : List Nat) (a b : Registers Value)
    (live : ids.all (!dead.contains ·) = true) (same : agrees dead a b) :
    ids.map a = ids.map b := by
  apply List.map_congr_left
  intro id member
  apply same id
  have := (List.all_eq_true.mp live) id member
  simpa using this

theorem update_agrees (dead writes : List Nat) (a b reply : Registers Value)
    (same : agrees dead a b) :
    agrees dead (update a writes reply) (update b writes reply) := by
  intro id live
  simp only [update]
  split
  · rfl
  · exact same id live

theorem drop_agrees (dead : List Nat) (action : Action) (a b reply : Registers Value)
    (dropped : drops dead action = true) (same : agrees dead a b) :
    agrees dead (update a action.writes reply) b := by
  intro id live
  have parts := dropped
  simp only [drops, Bool.and_eq_true_iff] at parts
  have writes := parts.2
  have writes := List.all_eq_true.mp writes
  have outside : id ∉ action.writes := by
    intro present
    have := writes id present
    have : id ∈ dead := by simpa using this
    exact live this
  simpa [update,outside] using same id live

def related (dead : List Nat) : Except Observation (Registers Value × State) →
    Except Observation (Registers Value × State) → Prop
  | .error a, .error b => a = b
  | .ok (a,s), .ok (b,t) => agrees dead a b ∧ s = t
  | _, _ => False

theorem actions_erase (handlers : Handlers Value State Observation) (dead : List Nat)
    (predecessor : Option Nat) (code : List Action) (a b : Registers Value) (state : State)
    (valid : code.all (fun action => drops dead action || action.reads.all (!dead.contains ·)) = true)
    (same : agrees dead a b) :
    related dead (actions handlers predecessor code a state)
      (actions handlers predecessor (code.filter (!drops dead ·)) b state) := by
  induction code generalizing a b state with
  | nil => exact ⟨same,rfl⟩
  | cons action rest ih =>
    have valid : (drops dead action || action.reads.all (!dead.contains ·)) = true ∧
        rest.all (fun action => drops dead action || action.reads.all (!dead.contains ·)) = true := by
      simpa only [List.all_cons, Bool.and_eq_true_iff] using valid
    have head := valid.1
    have tail := valid.2
    cases dropped : drops dead action with
    | true =>
      have silent : action.silent = true := by
        have h := dropped
        simp only [drops, Bool.and_eq_true_iff] at h
        exact h.1.1
      simpa [actions,dropped,silent,List.filter_cons,Except.bind,Bind.bind,Pure.pure,Except.pure] using
        ih (update a action.writes (handlers.pure action.key predecessor (action.reads.map a))) b state tail
          (drop_agrees dead action a b _ dropped same)
    | false =>
      have live : action.reads.all (!dead.contains ·) = true := by simpa [dropped] using head
      have inputs := reads_agree dead action.reads a b live same
      simp only [List.filter_cons, dropped, Bool.not_false, ↓reduceIte, actions]
      rw [inputs]
      cases silent : action.silent with
      | true =>
        simpa [silent,Except.bind,Bind.bind,Pure.pure,Except.pure] using
          ih (update a action.writes (handlers.pure action.key predecessor (action.reads.map b)))
            (update b action.writes (handlers.pure action.key predecessor (action.reads.map b)))
            state tail (update_agrees dead action.writes a b _ same)
      | false =>
        cases reply : handlers.effect action.key predecessor (action.reads.map b) state with
        | error failure => simp [Except.bind,Bind.bind,related]
        | ok pair =>
          obtain ⟨value,next⟩ := pair
          simpa [silent,reply,Except.bind,Bind.bind] using
            ih (update a action.writes value) (update b action.writes value) next tail
              (update_agrees dead action.writes a b value same)

/-- Live registers and shared state agree after every block, for all bounds. -/
theorem simulation (handlers : Handlers Value State Observation) (dead : List Nat)
    (program : List Block) (valid : program.all (safe dead) = true)
    (fuel index : Nat) (predecessor : Option Nat) (a b : Registers Value) (state : State)
    (same : agrees dead a b) :
    run handlers program fuel index predecessor a state =
      run handlers (program.map (eraseBlock dead)) fuel index predecessor b state := by
  induction fuel generalizing index predecessor a b state with
  | zero => rfl
  | succ fuel ih =>
    simp only [run,List.getElem?_map]
    cases found : program[index]? with
    | none => rfl
    | some block =>
      have present : block ∈ program := List.mem_of_getElem? found
      have safeBlock : block.actions.all (fun action => drops dead action || action.reads.all (!dead.contains ·)) = true ∧ block.reads.all (!dead.contains ·) = true := by
        simpa only [safe, Bool.and_eq_true_iff] using (List.all_eq_true.mp valid) block present
      have instructions := actions_erase handlers dead predecessor block.actions a b state
        safeBlock.1 same
      change related dead (actions handlers predecessor block.actions a state)
        (actions handlers predecessor (eraseBlock dead block).actions b state) at instructions
      simp only [Option.map_some]
      cases left : actions handlers predecessor block.actions a state with
      | error failure =>
        cases right : actions handlers predecessor (eraseBlock dead block).actions b state with
        | error other =>
          have equal : failure = other := by simpa [left,right,related] using instructions
          simp [equal]
        | ok pair => simp [left,right,related] at instructions
      | ok pair =>
        obtain ⟨locals,next⟩ := pair
        cases right : actions handlers predecessor (eraseBlock dead block).actions b state with
        | error other => simp [left,right,related] at instructions
        | ok other =>
          obtain ⟨otherLocals,otherState⟩ := other
          have equal : agrees dead locals otherLocals ∧ next = otherState := by
            simpa [left,right,related] using instructions
          obtain ⟨live, rfl⟩ := equal
          have inputs := reads_agree dead block.reads locals otherLocals safeBlock.2 live
          simp only [eraseBlock]
          rw [inputs]
          cases transfer : handlers.transfer block.key predecessor (block.reads.map otherLocals) next with
          | inl observation => rfl
          | inr target => exact ih target (some index) locals otherLocals next live

end SolarTV.MIR.DeadSSA
