import SolarTV.EVMIR.State.Transactions.Reservoir

/-! Nested gas accounting for Amsterdam. The caller supplies the forwarded
execution gas, stipend and any already determined account-creation charge.
The suspended caller retains its own ledger while the child can spend or
refill the inherited reservoir. Frame exit settles the child before merging
its meter into the parent. The signed refund counter follows the same frame
boundary as the journal, independently of the state gas ledger. -/

namespace SolarTV.EVM.Transaction.Reservoir

structure FrameMeter where
  meter : Meter
  refund : Int := 0
  deriving DecidableEq

inductive Exit where
  | success
  | revert
  | halt
  deriving DecidableEq

def FrameMeter.settle (frame : FrameMeter) : Exit → FrameMeter
  | .success => frame
  | .revert => ⟨frame.meter.rollback,0⟩
  | .halt => ⟨{frame.meter.rollback with execution := 0},0⟩

/-- The settled child reservoir replaces the suspended copy on every exit. -/
def FrameMeter.merge (parent child : FrameMeter) (exit : Exit) : FrameMeter :=
  let child := child.settle exit
  match exit with
  | .success => ⟨⟨parent.meter.execution + child.meter.execution,child.meter.reservoir,
      parent.meter.stateSpent + child.meter.stateSpent,
      parent.meter.stateSpilled + child.meter.stateSpilled⟩,parent.refund + child.refund⟩
  | .revert =>
    ⟨{parent.meter with
        execution := parent.meter.execution + child.meter.execution
        reservoir := child.meter.reservoir},parent.refund⟩
  | .halt => ⟨{parent.meter with reservoir := child.meter.reservoir},parent.refund⟩

structure Suspended where
  parent : FrameMeter
  reservedState : Nat
  deriving DecidableEq

structure Machine where
  current : FrameMeter
  callers : List Suspended := []
  deriving DecidableEq

def Machine.enter (machine : Machine) (forward stipend reserved : Nat) : Option Machine := do
  let charged ← machine.current.meter.spendState reserved
  let paid ← charged.spendExecution forward
  let suspended : Suspended := ⟨{machine.current with meter := paid},reserved⟩
  return ⟨⟨⟨forward+stipend,paid.reservoir,0,0⟩,0⟩,suspended :: machine.callers⟩

/-- Return an upfront charge for a state creation canceled by a failed child.
The restored execution gas belongs to the parent, even for a child halt. -/
def Machine.finish (machine : Machine) (exit : Exit) : Option Machine := do
  let saved :: rest := machine.callers | none
  let merged := saved.parent.merge machine.current exit
  let resumed := if exit = .success then merged else
    {merged with meter := merged.meter.refill saved.reservedState}
  return ⟨resumed,rest⟩

inductive FrameAction where
  | charge (cost : Charge)
  | refund (amount : Int)
  | enter (forward stipend reserved : Nat)
  | finish (exit : Exit)

def Machine.step (machine : Machine) : FrameAction → Option Machine
  | .charge cost => do
    let meter ← charge machine.current.meter cost
    return {machine with current := {machine.current with meter}}
  | .refund amount => some {machine with current := {machine.current with
      refund := machine.current.refund + amount}}
  | .enter forward stipend reserved => machine.enter forward stipend reserved
  | .finish exit => machine.finish exit

abbrev Snapshot := Meter × Int × Nat

def Machine.snapshot (machine : Machine) : Snapshot :=
  (machine.current.meter,machine.current.refund,machine.callers.length)

def frameTrace (machine : Machine) : List FrameAction → List (Option Snapshot)
  | [] => [some machine.snapshot]
  | action :: rest => some machine.snapshot :: match machine.step action with
    | none => [none]
    | some next => frameTrace next rest

/-- Before clamping to zero, this expression tracks the inherited reservoir. -/
def Meter.checkpoint (meter : Meter) : Int :=
  (meter.reservoir : Int) + meter.stateSpent - meter.stateSpilled

theorem rollback_reservoir (meter : Meter) :
    meter.rollback.reservoir = meter.checkpoint.toNat := rfl

theorem state_spend_checkpoint (meter next : Meter) (cost : Nat)
    (paid : meter.spendState cost = some next) : next.checkpoint = meter.checkpoint := by
  simp only [Meter.spendState] at paid
  split at paid
  · contradiction
  · simp only [Option.some.injEq] at paid
    subst next
    simp only [Meter.checkpoint]
    omega

theorem state_refill_checkpoint (meter : Meter) (amount : Nat) :
    (meter.refill amount).checkpoint = meter.checkpoint := by
  have units := Nat.min_le_left amount meter.stateSpilled
  have spill := Nat.min_le_right amount meter.stateSpilled
  simp only [Meter.refill,Meter.checkpoint]
  omega

theorem successful_merge_preserves_checkpoint (parent child : FrameMeter)
    (valid : child.meter.checkpoint = parent.meter.reservoir) :
    (parent.merge child .success).meter.checkpoint = parent.meter.checkpoint := by
  simp only [FrameMeter.merge,FrameMeter.settle,Meter.checkpoint] at valid ⊢
  omega

theorem reverted_merge (parent child : FrameMeter)
    (valid : child.meter.checkpoint = parent.meter.reservoir) :
    parent.merge child .revert =
      {parent with meter := {parent.meter with execution :=
        parent.meter.execution + child.meter.execution + child.meter.stateSpilled}} := by
  have inherited : child.meter.rollback.reservoir = parent.meter.reservoir := by
    rw [rollback_reservoir, valid]
    simp
  change FrameMeter.mk {parent.meter with
    execution := parent.meter.execution + child.meter.rollback.execution
    reservoir := child.meter.rollback.reservoir} parent.refund = _
  rw [inherited]
  simp [Meter.rollback, Nat.add_assoc]

theorem halted_merge (parent child : FrameMeter)
    (valid : child.meter.checkpoint = parent.meter.reservoir) :
    parent.merge child .halt = parent := by
  have inherited : child.meter.rollback.reservoir = parent.meter.reservoir := by
    rw [rollback_reservoir, valid]
    simp
  change ⟨{parent.meter with reservoir := child.meter.rollback.reservoir},parent.refund⟩ = parent
  rw [inherited]

/-- Each suspended caller is the source of the next frame's checkpoint.
The caller can keep its suspended reservoir copy because the merge replaces it. -/
def Linked (frame : FrameMeter) : List Suspended → Prop
  | [] => True
  | saved :: rest => frame.meter.checkpoint = saved.parent.meter.reservoir ∧
    Linked saved.parent rest

theorem linked_replace (old new : FrameMeter) (callers : List Suspended)
    (same : new.meter.checkpoint = old.meter.checkpoint)
    (linked : Linked old callers) : Linked new callers := by
  cases callers <;> simpa [Linked,same] using linked

theorem execution_spend_checkpoint (meter next : Meter) (cost : Nat)
    (paid : meter.spendExecution cost = some next) : next.checkpoint = meter.checkpoint := by
  simp only [Meter.spendExecution] at paid
  split at paid
  · contradiction
  · simp only [Option.some.injEq] at paid
    subst next
    rfl

theorem merged_checkpoint (parent child : FrameMeter) (exit : Exit)
    (valid : child.meter.checkpoint = parent.meter.reservoir) :
    (parent.merge child exit).meter.checkpoint = parent.meter.checkpoint := by
  cases exit
  · exact successful_merge_preserves_checkpoint parent child valid
  · rw [reverted_merge parent child valid] <;> rfl
  · rw [halted_merge parent child valid]

theorem enter_linked (machine next : Machine) (forward stipend reserved : Nat)
    (linked : Linked machine.current machine.callers)
    (entered : machine.enter forward stipend reserved = some next) :
    Linked next.current next.callers := by
  cases stateCost : machine.current.meter.spendState reserved with
  | none => simp [Machine.enter,stateCost] at entered
  | some charged =>
    cases executionCost : charged.spendExecution forward with
    | none => simp [Machine.enter,stateCost,executionCost] at entered
    | some paid =>
      have same := (execution_spend_checkpoint _ _ _ executionCost).trans
        (state_spend_checkpoint _ _ _ stateCost)
      simp [Machine.enter,stateCost,executionCost] at entered
      subst next
      exact ⟨by simp [Meter.checkpoint], linked_replace _ _ _ same linked⟩

theorem finish_linked (machine next : Machine) (exit : Exit)
    (linked : Linked machine.current machine.callers)
    (finished : machine.finish exit = some next) :
    Linked next.current next.callers := by
  cases callers : machine.callers with
  | nil => simp [Machine.finish,callers] at finished
  | cons saved rest =>
    have child := (show Linked machine.current (saved :: rest) by rwa [callers] at linked).1
    have parent := (show Linked machine.current (saved :: rest) by rwa [callers] at linked).2
    have same := merged_checkpoint saved.parent machine.current exit child
    simp [Machine.finish,callers] at finished
    subst next
    split
    · exact linked_replace _ _ _ same parent
    · exact linked_replace _ _ _ ((state_refill_checkpoint _ _).trans same) parent

theorem charge_linked (current next : FrameMeter) (callers : List Suspended) (action : Charge)
    (meter : charge current.meter action = some next.meter)
    (linked : Linked current callers) : Linked next callers := by
  cases action with
  | state cost => exact linked_replace _ _ _ (state_spend_checkpoint _ _ _ meter) linked
  | execution cost => exact linked_replace _ _ _ (execution_spend_checkpoint _ _ _ meter) linked
  | refill amount =>
    simp only [charge,Option.some.injEq] at meter
    apply linked_replace _ _ _ _ linked
    rw [← meter,state_refill_checkpoint]
  | rollback =>
    cases callers with
    | nil => trivial
    | cons saved rest =>
      simp only [Linked] at linked ⊢
      simp only [charge,Option.some.injEq] at meter
      rw [← meter]
      refine ⟨?_,linked.2⟩
      change (current.meter.rollback.reservoir : Int) + 0 - 0 = _
      rw [rollback_reservoir,linked.1]
      simp

theorem step_linked (machine next : Machine) (action : FrameAction)
    (linked : Linked machine.current machine.callers)
    (stepped : machine.step action = some next) : Linked next.current next.callers := by
  cases action with
  | enter forward stipend reserved => exact enter_linked _ _ _ _ _ linked stepped
  | finish exit => exact finish_linked _ _ _ linked stepped
  | refund amount =>
    change some _ = some next at stepped
    injection stepped with same
    subst next
    exact linked_replace machine.current _ machine.callers rfl linked
  | charge action =>
    cases cost : charge machine.current.meter action with
    | none => simp [Machine.step,cost] at stepped
    | some paid =>
      simp [Machine.step,cost] at stepped
      subst next
      exact charge_linked _ _ _ _ cost linked

end SolarTV.EVM.Transaction.Reservoir
