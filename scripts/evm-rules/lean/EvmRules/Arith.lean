import EvmRules.Word

/-!
# Word arithmetic as natural numbers

Rules that multiply and divide by symbolic constants are beyond bit-blasting: their
circuits contain 256-bit multipliers and dividers. These lemmas restate the `Evm`
comparisons, quotients, products and the readers' overflow preconditions as facts about
`Nat`, where the standard division lemmas prove such rules in a few steps.
-/

namespace EvmRules

/-- The all-ones word, as the printer writes `MAX`. -/
abbrev MAX : Word := 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff

theorem toNat_max : MAX.toNat = 2 ^ 256 - 1 := rfl

theorem toNat_pos {a : Word} (h : a ≠ 0) : 0 < a.toNat := by
  rcases Nat.eq_zero_or_pos a.toNat with h₀ | h₀
  · exact absurd (BitVec.eq_of_toNat_eq (by simpa using h₀)) h
  · exact h₀

/-! Comparison words are the bits of decided propositions about the numbers. -/

theorem lt_bits (a b : Word) : Evm.lt a b = bif decide (a.toNat < b.toNat) then 1 else 0 := by
  simp [Evm.lt, BitVec.ult]

theorem gt_bits (a b : Word) : Evm.gt a b = bif decide (b.toNat < a.toNat) then 1 else 0 := by
  simp [Evm.gt, BitVec.ult]

theorem eq_bits (a b : Word) : Evm.eq a b = bif decide (a.toNat = b.toNat) then 1 else 0 := by
  simp [Evm.eq, ← BitVec.toNat_eq]

theorem ne_bits (a b : Word) : Evm.ne a b = bif decide (a.toNat ≠ b.toNat) then 1 else 0 := by
  by_cases h : a = b
  · subst h
    simp [Evm.ne]
  · have h' : a.toNat ≠ b.toNat := fun e => h (BitVec.eq_of_toNat_eq e)
    simp [Evm.ne, h, h']

theorem or_bits (p q : Bool) :
    Evm.or (bif p then 1 else 0) (bif q then 1 else 0) = bif p || q then 1 else 0 := by
  cases p <;> cases q <;> decide

/-- Equivalent propositions have equal comparison words. `rw` cannot rewrite under
`decide`, whose instance depends on the proposition. -/
theorem bits_congr {p q : Prop} [Decidable p] [Decidable q] (h : p ↔ q) :
    (bif decide p then (1 : Word) else 0) = bif decide q then 1 else 0 := by
  rw [decide_eq_decide.mpr h]

theorem toNat_div (a b : Word) : (Evm.div a b).toNat = a.toNat / b.toNat :=
  BitVec.toNat_udiv

theorem toNat_mod {a b : Word} (h : b ≠ 0) : (Evm.mod a b).toNat = a.toNat % b.toNat := by
  have hb : (b == 0) = false := by simpa using h
  rw [Evm.mod, hb, Bool.cond_false, BitVec.toNat_umod]

theorem toNat_mul (a b : Word) : (Evm.mul a b).toNat = a.toNat * b.toNat % 2 ^ 256 :=
  BitVec.toNat_mul ..

/-! The readers' overflow preconditions bound the exact product or sum. -/

/-- `u256_mul_fits a b`: the exact product is a word. -/
theorem mul_fits {a b : Word} (h : b ≠ 0 → a ≤ Evm.div MAX b) :
    a.toNat * b.toNat < 2 ^ 256 := by
  by_cases hb : b = 0
  · subst hb
    simp
  · have h := h hb
    rw [BitVec.le_def, toNat_div, toNat_max] at h
    have := (Nat.le_div_iff_mul_le (toNat_pos hb)).mp h
    omega

/-- `u256_mul_fits a b` is false: the exact product exceeds `MAX`. -/
theorem mul_overflows {a b : Word} (h : ¬(b ≠ 0 → a ≤ Evm.div MAX b)) :
    2 ^ 256 ≤ a.toNat * b.toNat := by
  have hb : b ≠ 0 := fun hb => h fun h' => absurd hb h'
  have hab : ¬a ≤ Evm.div MAX b := fun h' => h fun _ => h'
  rw [BitVec.le_def, toNat_div, toNat_max] at hab
  have : ¬a.toNat * b.toNat ≤ 2 ^ 256 - 1 := fun h' =>
    hab ((Nat.le_div_iff_mul_le (toNat_pos hb)).mpr h')
  omega

theorem toNat_mul_of_fits {a b : Word} (h : a.toNat * b.toNat < 2 ^ 256) :
    (Evm.mul a b).toNat = a.toNat * b.toNat := by
  rw [toNat_mul, Nat.mod_eq_of_lt h]

/-- `u256_add_fits a b`: the exact sum is a word. -/
theorem add_fits {a b : Word} (h : a ≤ Evm.sub MAX b) : a.toNat + b.toNat < 2 ^ 256 := by
  have hb := b.isLt
  rw [BitVec.le_def, Evm.sub, BitVec.toNat_sub, toNat_max] at h
  have : (2 ^ 256 - b.toNat + (2 ^ 256 - 1)) % 2 ^ 256 = 2 ^ 256 - 1 - b.toNat := by omega
  omega

/-- `u256_add_fits a b` is false: the exact sum exceeds `MAX`. -/
theorem add_overflows {a b : Word} (h : ¬a ≤ Evm.sub MAX b) : 2 ^ 256 ≤ a.toNat + b.toNat := by
  have hb := b.isLt
  rw [BitVec.le_def, Evm.sub, BitVec.toNat_sub, toNat_max] at h
  have : (2 ^ 256 - b.toNat + (2 ^ 256 - 1)) % 2 ^ 256 = 2 ^ 256 - 1 - b.toNat := by omega
  omega

theorem toNat_add_of_fits {a b : Word} (h : a.toNat + b.toNat < 2 ^ 256) :
    (Evm.add a b).toNat = a.toNat + b.toNat := by
  rw [Evm.add, BitVec.toNat_add, Nat.mod_eq_of_lt h]

theorem toNat_sub_one {c : Word} (h : c ≠ 0) : (Evm.sub c 1).toNat = c.toNat - 1 := by
  have := toNat_pos h
  have hc := c.isLt
  have hone : (1 : Word).toNat = 1 := rfl
  rw [Evm.sub, BitVec.toNat_sub, hone]
  omega

/-- Rounding down to a multiple of `c` never exceeds the dividend, so it never wraps. -/
theorem toNat_round (x c : Word) :
    (Evm.mul (Evm.div x c) c).toNat = x.toNat / c.toNat * c.toNat := by
  have := Nat.div_mul_le_self x.toNat c.toNat
  have := x.isLt
  rw [toNat_mul_of_fits (by rw [toNat_div]; omega), toNat_div]

theorem round_le (x c : Word) : (Evm.mul (Evm.div x c) c).toNat ≤ x.toNat := by
  rw [toNat_round]
  exact Nat.div_mul_le_self ..

/-- A product whose wrapped value divides back to its factor did not wrap. -/
theorem wrapped_div_eq_iff {x c : Nat} (hc : 0 < c) :
    x * c % 2 ^ 256 / c = x ↔ x * c < 2 ^ 256 := by
  constructor
  · intro h
    apply Classical.byContradiction
    intro hlt
    have hr : x * c % 2 ^ 256 < x * c := by
      have := Nat.mod_lt (x * c) (Nat.two_pow_pos 256)
      omega
    have := (Nat.div_lt_iff_lt_mul hc).mpr hr
    omega
  · intro h
    rw [Nat.mod_eq_of_lt h, Nat.mul_div_cancel _ hc]

/-- The quotient bound a checked product by `c` stays within: `x ≤ MAX / c`. -/
theorem lt_max_div_succ_iff {x c : Nat} (hc : 0 < c) :
    x < (2 ^ 256 - 1) / c + 1 ↔ x * c < 2 ^ 256 := by
  have := (Nat.le_div_iff_mul_le hc (x := x) (y := 2 ^ 256 - 1))
  omega

/-- A quotient exceeds `d` exactly when the dividend reaches `c * (d + 1)`. -/
theorem lt_div_iff {x c d : Nat} (hc : 0 < c) : d < x / c ↔ c * d + (c - 1) < x := by
  rw [show d < x / c ↔ d + 1 ≤ x / c from Iff.rfl, Nat.le_div_iff_mul_le hc, Nat.add_mul,
    Nat.one_mul, Nat.mul_comm d c]
  omega

end EvmRules
