import EvmRules.Bits

/-!
# Structural rewrites

Identities that `evm_decide` (`Tactic.lean`) rewrites with before bit-blasting. Each replaces a
nest of variable shifts or extensions, which bit-blasting handles poorly, by one equivalent
operation, so that many rules over them hold syntactically afterwards. The reader states MIR
`sext value from to` as `(sar s (shl s value)) & ((1 << to) - 1)` with the count
`s = 256 - min(from, 256)`; the lemmas below match that form.
-/

namespace EvmRules

/-- Bit `i` of the reader's `sext x n m`: the low `n` bits of `x` with the top one repeated,
cut to `m` bits; zero for a zero width. -/
theorem bits_sext (n m x : Word) (i : Nat) :
    (Evm.and
      (Evm.sar (Evm.sub 256 (Evm.select (Evm.lt n 256) n 256))
        (Evm.shl (Evm.sub 256 (Evm.select (Evm.lt n 256) n 256)) x))
      (Evm.sub (Evm.shl m 1) 1)).getLsbD i =
      (decide (i < 256) && decide (i < m.toNat) && decide (0 < n.toNat) &&
        x.getLsbD (min i (n.toNat - 1))) := by
  have hs : (Evm.sub 256 (Evm.select (Evm.lt n 256) n 256)).toNat = 256 - min n.toNat 256 := by
    rw [toNat_sub_wrap, toNat_select_lt, show (256 : Word).toNat = 256 from rfl]
    omega
  rw [Evm.and, BitVec.getLsbD_and, bits_mask, bits_sar, bits_shl, hs]
  by_cases hi : i < 256 <;> by_cases hn : 0 < n.toNat <;> by_cases hm : i < m.toNat <;>
    simp [hi, hn, hm] <;> grind

/-- Nested `SIGNEXTEND`s keep the narrower extension. -/
theorem signextend_signextend (a b x : Word) :
    Evm.signextend a (Evm.signextend b x) = Evm.signextend (Evm.select (Evm.lt a b) a b) x := by
  apply BitVec.eq_of_getLsbD_eq
  intro i hi
  simp only [bits_signextend, toNat_select_lt]
  grind

theorem select_same (c a : Word) : Evm.select c a a = a := by
  unfold Evm.select
  cases c == 0 <;> rfl

/-- Extending the low `a` bits, then the low `b > a` bits of the result, extends the low `a`
bits once. -/
theorem sext_sext (a b c x : Word) (h : a < b) :
    Evm.and
      (Evm.sar (Evm.sub 256 (Evm.select (Evm.lt b 256) b 256))
        (Evm.shl (Evm.sub 256 (Evm.select (Evm.lt b 256) b 256))
          (Evm.and
            (Evm.sar (Evm.sub 256 (Evm.select (Evm.lt a 256) a 256))
              (Evm.shl (Evm.sub 256 (Evm.select (Evm.lt a 256) a 256)) x))
            (Evm.sub (Evm.shl b 1) 1))))
      (Evm.sub (Evm.shl c 1) 1) =
    Evm.and
      (Evm.sar (Evm.sub 256 (Evm.select (Evm.lt a 256) a 256))
        (Evm.shl (Evm.sub 256 (Evm.select (Evm.lt a 256) a 256)) x))
      (Evm.sub (Evm.shl c 1) 1) := by
  rw [BitVec.lt_def] at h
  apply BitVec.eq_of_getLsbD_eq
  intro i hi
  simp only [bits_sext]
  grind

/-- Equality words agree when the equalities they test are equivalent. -/
theorem eq_congr_iff {a b c d : Word} (h : a = b ↔ c = d) : Evm.eq a b = Evm.eq c d := by
  unfold Evm.eq
  by_cases e : c = d
  · rw [beq_iff_eq.mpr (h.mpr e), beq_iff_eq.mpr e]
  · rw [beq_eq_false_iff_ne.mpr (mt h.mp e), beq_eq_false_iff_ne.mpr e]

theorem ne_congr_iff {a b c d : Word} (h : a = b ↔ c = d) : Evm.ne a b = Evm.ne c d := by
  unfold Evm.ne
  by_cases e : c = d
  · rw [beq_iff_eq.mpr (h.mpr e), beq_iff_eq.mpr e]
  · rw [beq_eq_false_iff_ne.mpr (mt h.mp e), beq_eq_false_iff_ne.mpr e]

/-- A value of at most `n` bits is recovered from its extension to more bits. -/
theorem sext_inj (n m x y : Word) (h₁ : 1 ≤ n) (h₂ : n < m)
    (hx : Evm.and x (Evm.sub (Evm.shl n 1) 1) = x) (hy : Evm.and y (Evm.sub (Evm.shl n 1) 1) = y) :
    Evm.and
      (Evm.sar (Evm.sub 256 (Evm.select (Evm.lt n 256) n 256))
        (Evm.shl (Evm.sub 256 (Evm.select (Evm.lt n 256) n 256)) x))
      (Evm.sub (Evm.shl m 1) 1) =
    Evm.and
      (Evm.sar (Evm.sub 256 (Evm.select (Evm.lt n 256) n 256))
        (Evm.shl (Evm.sub 256 (Evm.select (Evm.lt n 256) n 256)) y))
      (Evm.sub (Evm.shl m 1) 1) ↔ x = y := by
  constructor
  · intro h
    rw [BitVec.le_def] at h₁
    rw [BitVec.lt_def] at h₂
    rw [BitVec.eq_of_getLsbD_eq_iff] at h hx hy ⊢
    intro i hi
    have e := h i hi
    have ex := hx i hi
    have ey := hy i hi
    rw [bits_sext, bits_sext] at e
    simp only [Evm.and, BitVec.getLsbD_and, bits_mask] at ex ey
    simp only [show (1 : Word).toNat = 1 from rfl] at h₁
    grind
  · rintro rfl
    rfl

theorem eq_sext_sext (n m x y : Word) (h₁ : 1 ≤ n) (h₂ : n < m)
    (hx : Evm.and x (Evm.sub (Evm.shl n 1) 1) = x) (hy : Evm.and y (Evm.sub (Evm.shl n 1) 1) = y) :
    Evm.eq
      (Evm.and
        (Evm.sar (Evm.sub 256 (Evm.select (Evm.lt n 256) n 256))
          (Evm.shl (Evm.sub 256 (Evm.select (Evm.lt n 256) n 256)) x))
        (Evm.sub (Evm.shl m 1) 1))
      (Evm.and
        (Evm.sar (Evm.sub 256 (Evm.select (Evm.lt n 256) n 256))
          (Evm.shl (Evm.sub 256 (Evm.select (Evm.lt n 256) n 256)) y))
        (Evm.sub (Evm.shl m 1) 1)) = Evm.eq x y :=
  eq_congr_iff (sext_inj n m x y h₁ h₂ hx hy)

theorem ne_sext_sext (n m x y : Word) (h₁ : 1 ≤ n) (h₂ : n < m)
    (hx : Evm.and x (Evm.sub (Evm.shl n 1) 1) = x) (hy : Evm.and y (Evm.sub (Evm.shl n 1) 1) = y) :
    Evm.ne
      (Evm.and
        (Evm.sar (Evm.sub 256 (Evm.select (Evm.lt n 256) n 256))
          (Evm.shl (Evm.sub 256 (Evm.select (Evm.lt n 256) n 256)) x))
        (Evm.sub (Evm.shl m 1) 1))
      (Evm.and
        (Evm.sar (Evm.sub 256 (Evm.select (Evm.lt n 256) n 256))
          (Evm.shl (Evm.sub 256 (Evm.select (Evm.lt n 256) n 256)) y))
        (Evm.sub (Evm.shl m 1) 1)) = Evm.ne x y :=
  ne_congr_iff (sext_inj n m x y h₁ h₂ hx hy)

theorem eq_sext_zero (n m x : Word) (h₁ : 1 ≤ n) (h₂ : n < m)
    (hx : Evm.and x (Evm.sub (Evm.shl n 1) 1) = x) :
    Evm.eq
      (Evm.and
        (Evm.sar (Evm.sub 256 (Evm.select (Evm.lt n 256) n 256))
          (Evm.shl (Evm.sub 256 (Evm.select (Evm.lt n 256) n 256)) x))
        (Evm.sub (Evm.shl m 1) 1)) 0 = Evm.eq x 0 := by
  simpa [Evm.shl, Evm.sar, Evm.and] using
    eq_sext_sext n m x 0 h₁ h₂ hx (by simp [Evm.and])

theorem ne_sext_zero (n m x : Word) (h₁ : 1 ≤ n) (h₂ : n < m)
    (hx : Evm.and x (Evm.sub (Evm.shl n 1) 1) = x) :
    Evm.ne
      (Evm.and
        (Evm.sar (Evm.sub 256 (Evm.select (Evm.lt n 256) n 256))
          (Evm.shl (Evm.sub 256 (Evm.select (Evm.lt n 256) n 256)) x))
        (Evm.sub (Evm.shl m 1) 1)) 0 = Evm.ne x 0 := by
  simpa [Evm.shl, Evm.sar, Evm.and] using
    ne_sext_sext n m x 0 h₁ h₂ hx (by simp [Evm.and])

/-- Unsigned division by a power of two is a right shift, also once the count reaches the
word width and the divisor is zero. -/
theorem div_shl_one (x n : Word) : Evm.div x (Evm.shl n 1) = Evm.shr n x := by
  apply BitVec.eq_of_toNat_eq
  rw [toNat_div, toNat_shr]
  by_cases h : n < 256
  · rw [toNat_shl_one h]
  · have hn : 256 ≤ n.toNat := by
      have : ¬n.toNat < 256 := h
      omega
    have hz : Evm.shl n 1 = 0 := by
      rw [Evm.shl, show n.ult 256 = false by simp [BitVec.ult]; omega]
      rfl
    rw [hz, show (0 : Word).toNat = 0 from rfl, Nat.div_zero, Nat.div_eq_of_lt]
    exact Nat.lt_of_lt_of_le x.isLt (Nat.pow_le_pow_right (by decide) hn)

/-- A word of at most one is a boolean, for `evm_ring` to split on. -/
theorem le_one_iff (c : Word) : c ≤ 1 ↔ c = 0 ∨ c = 1 := by
  constructor
  · intro h
    bv_omega
  · rintro (rfl | rfl) <;> decide

end EvmRules
