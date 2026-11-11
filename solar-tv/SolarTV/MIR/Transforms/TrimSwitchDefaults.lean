import SolarTV.MIR.Ops

/-! MIR cfg-simplify: trim trailing switch cases that jump to the default target. -/

namespace SolarTV.MIR

/-- One past the last non-default destination, or zero if there is none.
This is the prefix length computed by Rust's `rposition(...).map_or(0, |i| i + 1)`. -/
def keepLength (fallback : Nat) : List Case → Nat
  | [] => 0
  | (_, dst) :: rest =>
    match keepLength fallback rest with
    | 0 => if dst = fallback then 0 else 1
    | n + 1 => n + 2

theorem trim_preserves_dispatch (read : Nat → EvmRules.Word) (key : EvmRules.Word)
    (fallback : Nat) (cases : List Case) :
    dispatch read key fallback (cases.take (keepLength fallback cases)) =
      dispatch read key fallback cases := by
  induction cases with
  | nil => rfl
  | cons c rest ih =>
    obtain ⟨id, dst⟩ := c
    cases h : keepLength fallback rest with
    | zero =>
      simp [h, dispatch] at ih
      by_cases hd : dst = fallback <;> simp [keepLength, h, hd, dispatch, choose, ← ih]
    | succ n =>
      simp only [keepLength, h, List.take_succ_cons, dispatch]
      rw [h] at ih
      rw [ih]

/-- At prefix length zero the pass emits a jump. Both forms carry the same predecessor
and register environment into the successor (in particular for phi selection). -/
theorem simplify_switch (enter : Nat → Nat → (Nat → EvmRules.Word) → α)
    (pred fallback : Nat) (read : Nat → EvmRules.Word) (key : EvmRules.Word) (cases : List Case) :
    enter pred (if keepLength fallback cases = 0 then fallback
      else dispatch read key fallback (cases.take (keepLength fallback cases))) read =
    enter pred (dispatch read key fallback cases) read := by
  have h := trim_preserves_dispatch read key fallback cases
  split
  · next hz =>
      simp [hz, dispatch] at h
      exact congrArg (fun dst => enter pred dst read) h
  · rw [h]

end SolarTV.MIR
