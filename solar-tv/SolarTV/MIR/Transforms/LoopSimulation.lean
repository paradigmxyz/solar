/-! Relational loop certificates over logical iterations. A step can change
state, emit ordered effects, advance to another cutpoint, or terminate.
Both sides must establish the invariant on entry, preserve it across
backedges and produce the same observable exit. A proof over every prefix
also preserves divergence. The prefix length is independent of MIR work
and physical gas. -/

namespace SolarTV.MIR.LoopSimulation

universe u v w

inductive Step (State : Type u) (Event : Type v) (Exit : Type w) where
  | again (events : List Event) (next : State)
  | done (events : List Event) (exit : Exit)

abbrev Loop (State : Type u) (Event : Type v) (Exit : Type w) :=
  State → Step State Event Exit

def Step.related (invariant : Left → Right → Prop) :
    Step Left Event Exit → Step Right Event Exit → Prop
  | .again le ls, .again re rs => le = re ∧ invariant ls rs
  | .done le lx, .done re rx => le = re ∧ lx = rx
  | _, _ => False

/-- Each observation retains the ordered events of the prefix, whether or not
that reached reaches an exit. An exit includes any state the continuation needs. -/
def observe (step : Loop State Event Exit) :
    Nat → State → List Event × Option Exit
  | 0, _ => ([],none)
  | fuel+1, state =>
    match step state with
    | .again events next =>
      let (rest,exit) := observe step fuel next
      (events ++ rest,exit)
    | .done events exit => (events,some exit)

structure Simulation (left : Loop Left Event Exit) (right : Loop Right Event Exit) where
  invariant : Left → Right → Prop
  step : ∀ {a b}, invariant a b → (left a).related invariant (right b)

theorem Simulation.observe (proof : Simulation left right) (fuel : Nat)
    (initial : proof.invariant a b) :
    LoopSimulation.observe left fuel a = LoopSimulation.observe right fuel b := by
  induction fuel generalizing a b with
  | zero => rfl
  | succ fuel ih =>
    have related := proof.step initial
    unfold LoopSimulation.observe
    cases leftStep : left a <;> cases rightStep : right b <;>
      simp only [leftStep,rightStep,Step.related] at related
    · obtain ⟨events,next⟩ := related
      dsimp only
      rw [events, ih next]
    · obtain ⟨events,exit⟩ := related
      rw [events,exit]

theorem terminates (step : Loop State Event Exit) (rank : State → Nat)
    (decreases : ∀ state events next, step state = .again events next → rank next < rank state)
    (fuel : Nat) (state : State) (enough : rank state < fuel) :
    (observe step fuel state).2.isSome = true := by
  induction fuel generalizing state with
  | zero => omega
  | succ fuel ih =>
    rw [observe]
    cases result : step state with
    | done _ _ => rfl
    | again events next =>
      have smaller := decreases state events next result
      exact ih next (by omega)

/-- The effect trace and terminal result persist once a prefix has stopped. -/
theorem stableExit (step : Loop State Event Exit) (fuel extra : Nat) (state : State)
    (exit : (observe step fuel state).2.isSome = true) :
    observe step (fuel+extra) state = observe step fuel state := by
  induction fuel generalizing state with
  | zero => contradiction
  | succ fuel ih =>
    rw [show fuel+1+extra = fuel+extra+1 by omega, observe, observe]
    cases result : step state with
    | done _ _ => rfl
    | again _ next =>
      have stopped : (observe step fuel next).2.isSome = true := by
        simpa [observe,result] using exit
      dsimp only
      rw [ih next stopped]

def Step.prepend (events : List Event) : Step State Event Exit → Step State Event Exit
  | .again more next => .again (events ++ more) next
  | .done more exit => .done (events ++ more) exit

/-- A cutpoint step must consume at least one primitive step. The two sides
may consume different numbers of primitive steps between related cutpoints. -/
inductive Segment (step : Loop State Event Exit) :
    State → Step State Event Exit → Prop where
  | single (equal : step state = result) : Segment step state result
  | more (advance : step state = .again events next) (rest : Segment step next result) :
      Segment step state (result.prepend events)

private def finishSegment (step : Loop State Event Exit) (fuel : Nat) :
    Step State Event Exit → List Event × Option Exit
  | .again events next =>
    let (rest,exit) := observe step fuel next
    (events ++ rest,exit)
  | .done events exit => (events,some exit)

theorem Segment.observe (path : Segment step state result) :
    ∃ spent, 0 < spent ∧ ∀ fuel,
      LoopSimulation.observe step (spent+fuel) state = finishSegment step fuel result := by
  induction path with
  | single equal =>
    refine ⟨1,by decide,fun fuel => ?_⟩
    rw [Nat.add_comm 1 fuel, LoopSimulation.observe, equal]
    cases result <;> rfl
  | @more state events next result advance rest ih =>
    obtain ⟨spent,_,run⟩ := ih
    refine ⟨spent+1,by omega,fun fuel => ?_⟩
    rw [show spent+1+fuel = spent+fuel+1 by omega, LoopSimulation.observe, advance]
    dsimp only
    rw [run]
    cases result <;> simp [finishSegment,Step.prepend,List.append_assoc]

structure Cutpoints (step macroStep : Loop State Event Exit) where
  valid : State → Prop
  path : ∀ {state}, valid state → Segment step state (macroStep state)
  nextValid : ∀ {state events next}, valid state → macroStep state = .again events next → valid next

theorem Cutpoints.observe {step macroStep : Loop State Event Exit}
    (cut : Cutpoints step macroStep) (fuel : Nat) (state : State)
    (valid : cut.valid state) :
    ∃ steps, fuel ≤ steps ∧
      LoopSimulation.observe step steps state = LoopSimulation.observe macroStep fuel state := by
  induction fuel generalizing state with
  | zero => exact ⟨0,by simp,rfl⟩
  | succ fuel ih =>
    obtain ⟨spent,positive,run⟩ := (cut.path valid).observe
    cases edge : macroStep state with
    | done _ _ =>
      refine ⟨spent+fuel,by omega,?_⟩
      rw [run,edge,LoopSimulation.observe,edge]
      rfl
    | again events next =>
      obtain ⟨steps,bound,rest⟩ := ih next (cut.nextValid valid edge)
      refine ⟨spent+steps,by omega,?_⟩
      rw [run,edge,LoopSimulation.observe,edge,finishSegment]
      dsimp only
      rw [rest]

def diverges (step : Loop State Event Exit) (initial : State) : Prop :=
  ∀ fuel, (observe step fuel initial).2 = none

theorem Simulation.diverges (proof : Simulation left right) (initial : proof.invariant a b) :
    LoopSimulation.diverges left a ↔ LoopSimulation.diverges right b := by
  unfold LoopSimulation.diverges
  constructor <;> intro h fuel
  · rw [← proof.observe fuel initial]
    exact h fuel
  · rw [proof.observe fuel initial]
    exact h fuel

theorem Cutpoints.diverges (cut : Cutpoints step macroStep) (valid : cut.valid state) :
    LoopSimulation.diverges step state ↔ LoopSimulation.diverges macroStep state := by
  constructor
  · intro infinite fuel
    obtain ⟨steps,_,equal⟩ := cut.observe fuel state valid
    rw [← equal]
    exact infinite steps
  · intro infinite fuel
    obtain ⟨steps,bound,equal⟩ := cut.observe fuel state valid
    have reached := infinite fuel
    rw [← equal] at reached
    cases exit : (LoopSimulation.observe step fuel state).2 with
    | none => rfl
    | some result =>
      have stable := stableExit step fuel (steps-fuel) state (by simp [exit])
      rw [show fuel+(steps-fuel) = steps by omega] at stable
      rw [stable,exit] at reached
      contradiction

/-- Lift a simulation on related loop cutpoints back to primitive steps.
Every logical prefix has a pair of concrete executions with the same trace
and final observation. Neither loop can hide termination behind stuttering. -/
theorem Simulation.concretePrefixes (simulation : Simulation leftMacro rightMacro)
    (leftCut : Cutpoints left leftMacro) (rightCut : Cutpoints right rightMacro)
    (leftValid : leftCut.valid a) (rightValid : rightCut.valid b)
    (initial : simulation.invariant a b) (fuel : Nat) :
    ∃ l r, fuel ≤ l ∧ fuel ≤ r ∧
      LoopSimulation.observe left l a = LoopSimulation.observe right r b := by
  obtain ⟨l,leftBound,leftEq⟩ := leftCut.observe fuel a leftValid
  obtain ⟨r,rightBound,rightEq⟩ := rightCut.observe fuel b rightValid
  exact ⟨l,r,leftBound,rightBound,leftEq.trans ((simulation.observe fuel initial).trans rightEq.symm)⟩

theorem Simulation.concreteDivergence (simulation : Simulation leftMacro rightMacro)
    (leftCut : Cutpoints left leftMacro) (rightCut : Cutpoints right rightMacro)
    (leftValid : leftCut.valid a) (rightValid : rightCut.valid b)
    (initial : simulation.invariant a b) :
    LoopSimulation.diverges left a ↔ LoopSimulation.diverges right b :=
  (leftCut.diverges leftValid).trans
    ((simulation.diverges initial).trans (rightCut.diverges rightValid).symm)

end SolarTV.MIR.LoopSimulation
