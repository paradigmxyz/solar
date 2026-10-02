/-! Lemmas shared by the hand-written proofs in `proofs/`. They are checked with
every proof that uses them; `bv_decide` proofs do not include them. -/

namespace EvmRules

/-- The count the e-graph gives two nested shifts by `a` then `b`: each count clamped to the
word width, then their sum clamped again. As a value, it is `min (a + b) 256`. -/
theorem shift_sum_toNat (a b : BitVec 256) :
    (if ((if b.ult 256#256 = true then b else 256#256) +
          if a.ult 256#256 = true then a else 256#256).ult 256#256 = true then
        (if b.ult 256#256 = true then b else 256#256) +
          if a.ult 256#256 = true then a else 256#256
      else 256#256).toNat = min (a.toNat + b.toNat) 256 := by
  have clamp : ∀ x : BitVec 256,
      (if x.ult 256#256 = true then x else 256#256).toNat = min x.toNat 256 := by
    intro x
    by_cases hx : x.toNat < 256 <;> simp [BitVec.ult, hx] <;> omega
  have h₁ := clamp b
  have h₂ := clamp a
  generalize (if b.ult 256#256 = true then b else 256#256) = t₁ at *
  generalize (if a.ult 256#256 = true then a else 256#256) = t₂ at *
  have hsum : (t₁ + t₂).toNat = t₁.toNat + t₂.toNat := by
    rw [BitVec.toNat_add]
    apply Nat.mod_eq_of_lt
    omega
  have h256 : (256#256).toNat = 256 := by decide
  have hc : (t₁ + t₂).ult 256#256 = decide (t₁.toNat + t₂.toNat < 256) := by
    rw [BitVec.ult, hsum, h256]
  rw [hc]
  by_cases hlt : t₁.toNat + t₂.toNat < 256
  · simp only [hlt, decide_true, ↓reduceIte, hsum]
    omega
  · simp only [hlt, decide_false, Bool.false_eq_true, ↓reduceIte, h256]
    omega

/-- An arithmetic right shift by at least the width fills every bit with the sign, the same
as a shift by exactly the width. -/
theorem sshiftRight_saturate {w n : Nat} (x : BitVec w) (h : w ≤ n) :
    x.sshiftRight n = x.sshiftRight w := by
  apply BitVec.eq_of_getLsbD_eq
  intro i hi
  have h₁ : ¬n + i < w := by omega
  have h₂ : ¬w + i < w := by omega
  simp [BitVec.getLsbD_sshiftRight, h₁, h₂]

/-- Unsigned division behind a definition, so `bv_decide` treats each application as an opaque
word instead of bit-blasting a divider. -/
def opaqueUDiv (x y : BitVec w) : BitVec w := BitVec.smtUDiv x y

/-- Unsigned remainder behind a definition, for the same reason as `opaqueUDiv`. -/
def opaqueUMod (x y : BitVec w) : BitVec w := x % y

/-- A quotient is zero when the dividend is below the divisor; otherwise it stays opaque. -/
theorem smtUDiv_split (x y : BitVec w) :
    BitVec.smtUDiv x y = bif x.ult y then 0#w else opaqueUDiv x y := by
  cases h : x.ult y
  · rfl
  · have hlt : x < y := by simpa [BitVec.ult, BitVec.lt_def] using h
    have hy : y ≠ 0#w := by
      intro hy
      subst hy
      simp [BitVec.lt_def] at hlt
    simp [BitVec.smtUDiv_eq, hy, BitVec.udiv_eq_zero_iff_eq_zero_or_lt.mpr (Or.inr hlt)]

/-- A remainder is the dividend when it is below the divisor; otherwise it stays opaque. -/
theorem umod_split (x y : BitVec w) : x % y = bif x.ult y then x else opaqueUMod x y := by
  cases h : x.ult y
  · rfl
  · have hlt : x < y := by simpa [BitVec.ult, BitVec.lt_def] using h
    simp [BitVec.umod_eq_of_lt hlt]

end EvmRules

open EvmRules
