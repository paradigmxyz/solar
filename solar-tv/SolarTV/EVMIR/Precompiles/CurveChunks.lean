import SolarTV.EVMIR.Precompiles.Curve

/-! A split for concrete precompile arithmetic. The proposed checkpoint values
can come from any implementation. The equalities check only bounded segments. -/

namespace SolarTV.Precompiles.CurveChunks

abbrev PowState := Nat × Nat × Nat

def powAdvance (modulus : Nat) : Nat → PowState → PowState
  | 0, state => state
  | fuel + 1, (base, exponent, acc) =>
    if exponent = 0 then (base,exponent,acc)
    else powAdvance modulus fuel (base*base % modulus, exponent/2,
      if exponent%2 = 1 then acc*base % modulus else acc)

def powFinish (modulus fuel : Nat) (state : PowState) : Nat :=
  powLoop modulus fuel state.1 state.2.1 state.2.2

theorem pow_split (modulus steps rest : Nat) (state : PowState) :
    powFinish modulus (steps + rest) state =
      powFinish modulus rest (powAdvance modulus steps state) := by
  induction steps generalizing state with
  | zero => simp [powAdvance]
  | succ steps ih =>
    obtain ⟨base,exponent,acc⟩ := state
    by_cases closed : exponent = 0
    · subst exponent
      cases rest <;> simp [powFinish,powLoop,powAdvance]
    · simp only [powFinish,powAdvance,Nat.succ_add,powLoop,closed,ite_false]
      exact ih (base * base % modulus, exponent / 2,
        if exponent % 2 = 1 then acc * base % modulus else acc)

structure MulState (K : Type) where
  scalar : Nat
  point : Point K
  acc : Point K

def mulAdvance (field : Arithmetic K) (a : K) : Nat → MulState K → MulState K
  | 0, state => state
  | fuel + 1, state =>
    if state.scalar = 0 then state
    else mulAdvance field a fuel
      ⟨state.scalar/2, double field a state.point,
        if state.scalar%2 = 1 then add field a state.acc state.point else state.acc⟩

def mulFinish (field : Arithmetic K) (a : K) (fuel : Nat) (state : MulState K) : Point K :=
  multiplyLoop field a fuel state.scalar state.point state.acc

theorem mul_split (field : Arithmetic K) (a : K) (steps rest : Nat) (state : MulState K) :
    mulFinish field a (steps + rest) state =
      mulFinish field a rest (mulAdvance field a steps state) := by
  induction steps generalizing state with
  | zero => simp [mulAdvance]
  | succ steps ih =>
    obtain ⟨scalar,point,acc⟩ := state
    by_cases closed : scalar = 0
    · subst scalar
      cases rest <;> simp [mulFinish,multiplyLoop,mulAdvance]
    · simp only [mulFinish,mulAdvance,Nat.succ_add,multiplyLoop,closed,ite_false]
      exact ih ⟨scalar / 2, double field a point,
        if scalar % 2 = 1 then add field a acc point else acc⟩

end SolarTV.Precompiles.CurveChunks
