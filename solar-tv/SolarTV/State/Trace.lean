import Init

/-! A bounded execution split into independently checked segments. A segment
can stop at the requested cut or pass its complete state to the next segment. -/

namespace SolarTV.Execution.Trace

abbrev Step (S R : Type) := Nat → S → Except String (R ⊕ S)

/-- Preserve a failed initial decode as an error before any instruction runs. -/
def checkedStart (step : Step S R) : Step (Except String S) R := fun fuel state => do
  match ← step fuel (← state) with
  | .inl result => return .inl result
  | .inr next => return .inr (.ok next)

def run (step : Step S R) (fuel : Nat) (state : S) : Except String R := do
  match ← step (fuel-1) state with
  | .inl result => return result
  | .inr state => match fuel with
    | 0 => .error "trace fuel exhausted"
    | fuel+1 => run step fuel state
termination_by structural fuel

def segment (step : Step S R) : Nat → Nat → S → Except String (R ⊕ S)
  | 0, _, state => .ok (.inr state)
  | width+1, fuel, state => do
    match ← step (fuel-1) state with
    | .inl result => return .inl result
    | .inr state => if fuel = 0 then .error "trace fuel exhausted" else segment step width (fuel-1) state

def resume (step : Step S R) (fuel : Nat) : R ⊕ S → Except String R
  | .inl result => .ok result
  | .inr state => run step fuel state

theorem segment_next {step : Step S R} (width fuel : Nat) {state next : S}
    {result : Except String (R ⊕ S)}
    (head : step fuel state = .ok (.inr next)) (rest : segment step width fuel next = result) :
    segment step (width+1) (fuel+1) state = result := by
  simpa only [segment, Nat.add_sub_cancel, Nat.succ_ne_zero, ite_false, head, Bind.bind, Except.bind] using rest

theorem segment_continue {step : Step S R} (first rest fuel : Nat) {state next : S}
    (checked : segment step first fuel state = .ok (.inr next)) :
    segment step (first + rest) fuel state = segment step rest (fuel-first) next := by
  induction first generalizing fuel state with
  | zero =>
    cases checked
    simp
  | succ first ih =>
    cases fuel with
    | zero =>
      unfold segment at checked
      cases action : step 0 state with
      | error _ => simp [action, Bind.bind, Except.bind] at checked
      | ok signal =>
        cases signal <;> simp [action, Bind.bind, Except.bind, Pure.pure, Except.pure] at checked
    | succ fuel =>
      unfold segment at checked
      cases action : step fuel state with
      | error _ => simp [action, Bind.bind, Except.bind] at checked
      | ok signal =>
        cases signal with
        | inl result => simp [action, Bind.bind, Except.bind, Pure.pure, Except.pure] at checked
        | inr successor =>
          simp only [Nat.add_sub_cancel, action, Bind.bind, Except.bind,
            Nat.succ_ne_zero, ite_false] at checked
          rw [show first + 1 + rest = (first + rest) + 1 by omega]
          simp only [segment, Nat.sub_zero, action, Bind.bind, Except.bind,
            Nat.succ_ne_zero, ite_false, Nat.succ_sub_succ_eq_sub]
          exact ih fuel checked

theorem run_segment {step : Step S R} (width fuel : Nat) (state : S) (endState : R ⊕ S)
    (checked : segment step width fuel state = .ok endState) :
    run step fuel state = resume step (fuel-width) endState := by
  induction width generalizing fuel state with
  | zero => cases checked; rfl
  | succ width ih =>
    cases fuel with
    | zero =>
      unfold segment at checked
      cases h : step 0 state with
      | error _ => simp [h,Except.bind,Bind.bind] at checked
      | ok signal =>
        cases signal <;> simp [h,Except.bind,Bind.bind] at checked
        cases checked
        rw [run]
        simp [h,Except.bind,Bind.bind,resume,Pure.pure,Except.pure]
    | succ fuel =>
      unfold segment at checked
      cases h : step fuel state with
      | error _ => simp [h,Except.bind,Bind.bind] at checked
      | ok signal =>
        cases signal with
        | inl result =>
          simp [h,Except.bind,Bind.bind] at checked
          cases checked
          rw [run]
          simp [h,Except.bind,Bind.bind,resume,Pure.pure,Except.pure]
        | inr next =>
          simp only [Nat.add_sub_cancel, h, Except.bind, Bind.bind, Nat.succ_ne_zero, ite_false] at checked
          rw [run]
          simp only [Nat.add_sub_cancel, h, Except.bind, Bind.bind]
          simpa only [Nat.succ_sub_succ_eq_sub] using ih fuel next checked

theorem finish_run {step : Step S R} (width fuel : Nat) (state : S) (result : R)
    (checked : segment step width fuel state = .ok (.inl result)) :
    run step fuel state = .ok result :=
  run_segment width fuel state (.inl result) checked

end SolarTV.Execution.Trace
