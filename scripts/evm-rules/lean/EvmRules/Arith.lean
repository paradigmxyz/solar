import EvmRules.Word

/-!
# Word arithmetic as natural numbers

The lemmas `evm_arith` (`ArithTactic.lean`) rewrites with. Comparison words become the
bits of decided propositions, each operation becomes a number with its wrapping, and the
readers' overflow preconditions become bounds on exact products and sums. The bounds the
tactic adds for every quotient and remainder, and the rewrites that remove wrapping and
quotients once those bounds allow it, follow.
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

theorem iszero_bits (a : Word) : Evm.iszero a = bif decide (a.toNat = 0) then 1 else 0 := by
  by_cases h : a.toNat = 0
  · have ha : a = 0 := BitVec.eq_of_toNat_eq h
    subst ha
    rfl
  · have hb : (a == 0) = false := by
      rw [beq_eq_false_iff_ne]
      exact fun e => h (by rw [e]; rfl)
    rw [Evm.iszero, hb]
    simp [h]

theorem or_bits (p q : Bool) :
    Evm.or (bif p then 1 else 0) (bif q then 1 else 0) = bif p || q then 1 else 0 := by
  cases p <;> cases q <;> decide

theorem bits_eq_bits (p q : Prop) [Decidable p] [Decidable q] :
    (bif decide p then (1 : Word) else 0) = (bif decide q then 1 else 0) ↔ (p ↔ q) := by
  by_cases hp : p <;> by_cases hq : q <;> simp [hp, hq]

theorem bits_eq_one (p : Prop) [Decidable p] : (bif decide p then (1 : Word) else 0) = 1 ↔ p := by
  by_cases hp : p <;> simp [hp]

theorem bits_eq_zero (p : Prop) [Decidable p] :
    (bif decide p then (1 : Word) else 0) = 0 ↔ ¬p := by
  by_cases hp : p <;> simp [hp]

/-! Each operation as a number, with its wrapping and EVM's zero divisor. -/

theorem toNat_div (a b : Word) : (Evm.div a b).toNat = a.toNat / b.toNat :=
  BitVec.toNat_udiv

theorem toNat_mul (a b : Word) : (Evm.mul a b).toNat = a.toNat * b.toNat % 2 ^ 256 :=
  BitVec.toNat_mul ..

theorem toNat_mod_ite (a b : Word) :
    (Evm.mod a b).toNat = if b.toNat = 0 then 0 else a.toNat % b.toNat := by
  by_cases h : b.toNat = 0
  · have hb : b = 0 := BitVec.eq_of_toNat_eq h
    subst hb
    simp [Evm.mod]
  · have hb : (b == 0) = false := by
      rw [beq_eq_false_iff_ne]
      exact fun e => h (by rw [e]; rfl)
    rw [ite_eq_right h, Evm.mod, hb, Bool.cond_false, BitVec.toNat_umod]

theorem toNat_add_wrap (a b : Word) : (Evm.add a b).toNat = (a.toNat + b.toNat) % 2 ^ 256 :=
  BitVec.toNat_add ..

theorem toNat_sub_wrap (a b : Word) :
    (Evm.sub a b).toNat = (2 ^ 256 - b.toNat + a.toNat) % 2 ^ 256 :=
  BitVec.toNat_sub ..

/-- `ADDMOD` as a number: the exact sum, reduced by a nonzero modulus. -/
theorem toNat_addmod (a b n : Word) :
    (Evm.addmod a b n).toNat = if n.toNat = 0 then 0 else (a.toNat + b.toNat) % n.toNat := by
  have ha := a.isLt
  have hb := b.isLt
  have hn := n.isLt
  by_cases h : n.toNat = 0
  · have : n = 0 := BitVec.eq_of_toNat_eq h
    subst this
    rfl
  · have hz : (n == 0) = false := beq_eq_false_iff_ne.mpr fun e => h (by rw [e]; rfl)
    rw [Evm.addmod, hz, Bool.cond_false]
    simp only [h, ↓reduceIte]
    rw [BitVec.toNat_setWidth, BitVec.toNat_umod, BitVec.toNat_add, BitVec.toNat_setWidth,
      BitVec.toNat_setWidth, BitVec.toNat_setWidth]
    have hr := Nat.mod_lt (a.toNat + b.toNat) (Nat.pos_of_ne_zero h)
    rw [Nat.mod_eq_of_lt (by omega : a.toNat < 2 ^ 512),
      Nat.mod_eq_of_lt (by omega : b.toNat < 2 ^ 512),
      Nat.mod_eq_of_lt (by omega : n.toNat < 2 ^ 512),
      Nat.mod_eq_of_lt (by omega : a.toNat + b.toNat < 2 ^ 512),
      Nat.mod_eq_of_lt (by omega : (a.toNat + b.toNat) % n.toNat < 2 ^ 256)]

set_option exponentiation.threshold 512 in
/-- `MULMOD` as a number: the exact product, reduced by a nonzero modulus. -/
theorem toNat_mulmod (a b n : Word) :
    (Evm.mulmod a b n).toNat = if n.toNat = 0 then 0 else a.toNat * b.toNat % n.toNat := by
  have ha := a.isLt
  have hb := b.isLt
  have hn := n.isLt
  have hab : a.toNat * b.toNat < 2 ^ 512 :=
    calc a.toNat * b.toNat < 2 ^ 256 * 2 ^ 256 := Nat.mul_lt_mul'' ha hb
      _ = 2 ^ 512 := by rw [← Nat.pow_add]
  by_cases h : n.toNat = 0
  · have : n = 0 := BitVec.eq_of_toNat_eq h
    subst this
    rfl
  · have hz : (n == 0) = false := beq_eq_false_iff_ne.mpr fun e => h (by rw [e]; rfl)
    rw [Evm.mulmod, hz, Bool.cond_false]
    simp only [h, ↓reduceIte]
    rw [BitVec.toNat_setWidth, BitVec.toNat_umod, BitVec.toNat_mul, BitVec.toNat_setWidth,
      BitVec.toNat_setWidth, BitVec.toNat_setWidth]
    have hr := Nat.mod_lt (a.toNat * b.toNat) (Nat.pos_of_ne_zero h)
    rw [Nat.mod_eq_of_lt (by omega : a.toNat < 2 ^ 512),
      Nat.mod_eq_of_lt (by omega : b.toNat < 2 ^ 512),
      Nat.mod_eq_of_lt (by omega : n.toNat < 2 ^ 512), Nat.mod_eq_of_lt hab,
      Nat.mod_eq_of_lt (by omega : a.toNat * b.toNat % n.toNat < 2 ^ 256)]

/-! The readers' overflow preconditions as bounds on the exact product or sum. -/

/-- `u256_mul_fits a b`: the exact product is a word. -/
theorem mul_fits_iff (a b : Word) :
    (b ≠ 0 → a ≤ Evm.div MAX b) ↔ a.toNat * b.toNat < 2 ^ 256 := by
  rw [BitVec.le_def, toNat_div, toNat_max]
  constructor
  · intro h
    by_cases hb : b = 0
    · subst hb
      simp
    · have := (Nat.le_div_iff_mul_le (toNat_pos hb)).mp (h hb)
      omega
  · intro h hb
    exact (Nat.le_div_iff_mul_le (toNat_pos hb)).mpr (by omega)

/-- `u256_add_fits a b`: the exact sum is a word. -/
theorem add_fits_iff (a b : Word) : a ≤ Evm.sub MAX b ↔ a.toNat + b.toNat < 2 ^ 256 := by
  have hb := b.isLt
  rw [BitVec.le_def, Evm.sub, BitVec.toNat_sub, toNat_max]
  have : (2 ^ 256 - b.toNat + (2 ^ 256 - 1)) % 2 ^ 256 = 2 ^ 256 - 1 - b.toNat := by omega
  omega

/-- `a ≤ MAX / b`, as the readers state that the exact product is a word when `b ≠ 0` is a
separate precondition. -/
theorem le_div_max_iff (a b : Word) :
    a ≤ Evm.div MAX b ↔ a.toNat * b.toNat < 2 ^ 256 ∧ (b.toNat = 0 → a.toNat = 0) := by
  rw [BitVec.le_def, toNat_div, toNat_max]
  by_cases hb : b.toNat = 0
  · simp [hb]
  · rw [Nat.le_div_iff_mul_le (Nat.pos_of_ne_zero hb)]
    omega

/-! Bounds that `evm_arith` adds for every quotient and remainder by a non-literal. -/

theorem div_le_half (a b : Nat) : 2 ≤ b → a / b ≤ a / 2 := fun h => Nat.div_le_div_left h (by decide)

theorem lt_div_mul_add_of_pos (a b : Nat) : 0 < b → a < a / b * b + b :=
  fun h => Nat.lt_div_mul_add h

theorem mod_lt_of_pos (a b : Nat) : 0 < b → a % b < b := fun h => Nat.mod_lt a h

/-! Rewrites that remove wrapping and quotients once those bounds allow it. -/

theorem sub_wrap_of_le {a b n : Nat} (h : b ≤ a) (ha : a < n) : (n - b + a) % n = a - b := by
  rw [show n - b + a = a - b + n by omega, Nat.add_mod_right, Nat.mod_eq_of_lt (by omega)]

/-- Rounding down to a multiple of `y` and dividing by `y` again gives the quotient, also
when `y` is zero. -/
theorem div_mul_div_cancel (x y : Nat) : x / y * y / y = x / y := by
  by_cases h : y = 0
  · simp [h]
  · rw [Nat.mul_div_cancel _ (Nat.pos_of_ne_zero h)]

theorem lt_div_succ_iff {x c m : Nat} (hc : 0 < c) : x < m / c + 1 ↔ x * c ≤ m := by
  rw [Nat.lt_succ_iff, Nat.le_div_iff_mul_le hc]

/-- A product whose wrapped value divides back to its factor did not wrap. -/
theorem wrapped_div_eq_iff {x c n : Nat} (hc : 0 < c) (hn : 0 < n) :
    x * c % n / c = x ↔ x * c < n := by
  constructor
  · intro h
    apply Classical.byContradiction
    intro hlt
    have hr : x * c % n < x * c := by
      have := Nat.mod_lt (x * c) hn
      omega
    have := (Nat.div_lt_iff_lt_mul hc).mpr hr
    omega
  · intro h
    rw [Nat.mod_eq_of_lt h, Nat.mul_div_cancel _ hc]

theorem eq_wrapped_div_iff {x c n : Nat} (hc : 0 < c) (hn : 0 < n) :
    x = x * c % n / c ↔ x * c < n := by
  rw [eq_comm]
  exact wrapped_div_eq_iff hc hn

/-- The test of a checked product `c * x`, which divides by the variable factor. -/
theorem checked_product_left {x c n : Nat} (hn : 0 < n) :
    (x = 0 ∨ x * c % n / x = c) ↔ x * c < n := by
  by_cases hx : x = 0
  · simp [hx, hn]
  · simp only [hx, false_or]
    rw [Nat.mul_comm]
    exact wrapped_div_eq_iff (Nat.pos_of_ne_zero hx) hn

theorem checked_product_left' {x c n : Nat} (hn : 0 < n) :
    (x * c % n / x = c ∨ x = 0) ↔ x * c < n := by
  rw [or_comm]
  exact checked_product_left hn

/-- A share `c / d ≤ 1` of `x` never exceeds `x`, also when `x * c` wraps: the wrapped
value is still at most `x * c ≤ x * d`. -/
theorem not_lt_share {x c d n : Nat} (hcd : c ≤ d) : ¬x < x * c % n / d := by
  rcases Nat.eq_zero_or_pos d with hd | hd
  · simp [hd]
  · have h₁ := Nat.mod_le (x * c) n
    have h₂ := Nat.mul_le_mul_left x hcd
    have h₃ : x * c % n / d ≤ x * d / d := Nat.div_le_div_right (by omega)
    rw [Nat.mul_div_cancel _ hd] at h₃
    omega

end EvmRules
