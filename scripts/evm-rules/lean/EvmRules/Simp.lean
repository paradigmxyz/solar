import EvmRules.Casts

/-!
# Simplification lemmas

The `@[simp]` lemmas of paradigmxyz/solar#1648, restated for the definitions in `Word.lean`.
`evm_simp` (`Tactic.lean`) rewrites the goal and every hypothesis with them before deciding
what remains: products and quotients by powers of two become shifts, nested shifts become one,
a shift distributes over bitwise operations and addition, `EXP` with a literal base or exponent
folds, and a cast recovers its input.
-/

namespace EvmRules

/-- A count clamped to the word width, as the e-graph's `shift_sum` clamps each count. -/
theorem clamp_toNat (x : Word) :
    (Evm.select (Evm.lt x 256) x 256).toNat = min x.toNat 256 := by
  unfold Evm.select Evm.lt
  by_cases hx : x.toNat < 256 <;> simp [BitVec.ult, hx] <;> omega

/-- The count the e-graph gives two nested shifts by `a` then `b`: each count clamped to the
word width, then their sum clamped again. As a number, it is `min (a + b) 256`. -/
theorem shift_sum_toNat (a b : Word) :
    (Evm.select
        (Evm.lt (Evm.add (Evm.select (Evm.lt a 256) a 256) (Evm.select (Evm.lt b 256) b 256))
          256)
        (Evm.add (Evm.select (Evm.lt a 256) a 256) (Evm.select (Evm.lt b 256) b 256))
        256).toNat = min (a.toNat + b.toNat) 256 := by
  rw [clamp_toNat]
  have h₁ := clamp_toNat a
  have h₂ := clamp_toNat b
  generalize Evm.select (Evm.lt a 256) a 256 = t₁ at *
  generalize Evm.select (Evm.lt b 256) b 256 = t₂ at *
  have hsum : (Evm.add t₁ t₂).toNat = t₁.toNat + t₂.toNat := by
    rw [Evm.add, BitVec.toNat_add]
    apply Nat.mod_eq_of_lt
    omega
  omega

theorem one_pow (n : Nat) : (1 : Word) ^ n = 1 := by
  induction n with
  | zero => rfl
  | succ n ih => rw [BitVec.pow_succ, ih]; rfl

theorem two_pow (n : Nat) : (2 : Word) ^ n = BitVec.twoPow 256 n := by
  induction n with
  | zero => rfl
  | succ n ih =>
    rw [BitVec.pow_succ, ih, show (2 : Word) = BitVec.twoPow 256 1 from rfl,
      BitVec.twoPow_mul_twoPow_eq]

/-! Products, quotients and remainders. -/

theorem mul_shl_one (x c : Word) : Evm.mul x (Evm.shl c 1) = Evm.shl c x := by
  rw [Evm.mul, Evm.shl_eq, Evm.shl_eq, BitVec.shiftLeft_eq', BitVec.shiftLeft_eq',
    show (1 : Word) = 1#256 from rfl, ← BitVec.twoPow_eq, BitVec.mul_twoPow_eq_shiftLeft]

theorem shl_one_mul (x c : Word) : Evm.mul (Evm.shl c 1) x = Evm.shl c x := by
  rw [Evm.mul, BitVec.mul_comm, ← Evm.mul, mul_shl_one]

theorem div_lt (x y : Word) (h : x < y) : Evm.div x y = 0 := by
  apply BitVec.eq_of_toNat_eq
  rw [Evm.div, BitVec.toNat_udiv, Nat.div_eq_of_lt h]
  rfl

theorem mod_lt (x y : Word) (h : x < y) : Evm.mod x y = x := by
  have hy : (y == 0) = false := by
    rw [beq_eq_false_iff_ne]
    intro e
    subst e
    exact absurd h (by simp [BitVec.lt_def])
  rw [Evm.mod, hy, Bool.cond_false, BitVec.umod_eq_of_lt h]

theorem mod_self (x : Word) : Evm.mod x x = 0 := by
  unfold Evm.mod
  cases x == 0 <;> simp [BitVec.umod_self]

theorem smod_self (x : Word) : Evm.smod x x = 0 := by
  unfold Evm.smod
  cases x == 0 <;> simp

/-- A remainder by a power of two below the word width keeps the low bits. -/
theorem mod_shl_one (x c : Word) (h : c < 256) :
    Evm.mod x (Evm.shl c 1) = Evm.and x (Evm.sub (Evm.shl c 1) 1) := by
  have hc : c.toNat < 256 := h
  have hn := toNat_shl_one h
  have hp : 2 ^ c.toNat < 2 ^ 256 := Nat.pow_lt_pow_right (by decide) hc
  have hz : (Evm.shl c 1 == 0) = false := by
    rw [beq_eq_false_iff_ne]
    intro e
    rw [e] at hn
    have := Nat.two_pow_pos c.toNat
    simp at hn
    omega
  apply BitVec.eq_of_toNat_eq
  rw [Evm.mod, hz, Bool.cond_false, BitVec.toNat_umod, Evm.and, BitVec.toNat_and, Evm.sub,
    BitVec.toNat_sub, hn, show (1 : Word).toNat = 1 from rfl]
  have h1 := Nat.one_le_two_pow (n := c.toNat)
  rw [show (2 ^ 256 - 1 + 2 ^ c.toNat) % 2 ^ 256 = 2 ^ c.toNat - 1 by omega,
    Nat.and_two_pow_sub_one_eq_mod]

/-- `CLZ` is at most 256, so it is below any larger divisor. -/
theorem div_clz (x b : Word) (h : 256 < b) : Evm.div (Evm.clz x) b = 0 :=
  div_lt _ _ (Nat.lt_of_le_of_lt BitVec.clz_le h)

theorem mod_clz (x b : Word) (h : 256 < b) : Evm.mod (Evm.clz x) b = Evm.clz x :=
  mod_lt _ _ (Nat.lt_of_le_of_lt BitVec.clz_le h)

/-! `EXP` with a literal base or exponent. -/

theorem exp_two (c : Word) : Evm.exp 2 c = Evm.shl c 1 := by
  rw [Evm.exp, two_pow, Evm.shl_eq, BitVec.shiftLeft_eq', BitVec.twoPow_eq]
  rfl

theorem exp_square (x : Word) : Evm.exp x 2 = Evm.mul x x := by
  rw [Evm.exp, Evm.mul, show (2 : Word).toNat = 2 from rfl, BitVec.pow_succ, BitVec.pow_succ,
    BitVec.pow_zero, BitVec.one_mul]

theorem exp_zero (x : Word) : Evm.exp x 0 = 1 := by
  rw [Evm.exp, show (0 : Word).toNat = 0 from rfl, BitVec.pow_zero]
  rfl

theorem exp_one (x : Word) : Evm.exp x 1 = x := by
  rw [Evm.exp, show (1 : Word).toNat = 1 from rfl, BitVec.pow_succ, BitVec.pow_zero,
    BitVec.one_mul]

theorem one_exp (x : Word) : Evm.exp 1 x = 1 := by
  rw [Evm.exp, one_pow]

/-! Nested shifts by `a`, then `b`, are one shift by their sum clamped to the word width. -/

/-- The clamped sum of two shift counts. -/
def shiftSum (a b : Word) : Word := BitVec.ofNat 256 (min 256 (a.toNat + b.toNat))

theorem shiftSum_toNat (a b : Word) : (shiftSum a b).toNat = min 256 (a.toNat + b.toNat) := by
  rw [shiftSum, BitVec.toNat_ofNat]
  apply Nat.mod_eq_of_lt
  have := Nat.min_le_left 256 (a.toNat + b.toNat)
  omega

/-- The e-graph's `shift_sum` of two counts is their clamped sum. -/
theorem shift_sum_eq (a b : Word) :
    Evm.select
        (Evm.lt (Evm.add (Evm.select (Evm.lt a 256) a 256) (Evm.select (Evm.lt b 256) b 256))
          256)
        (Evm.add (Evm.select (Evm.lt a 256) a 256) (Evm.select (Evm.lt b 256) b 256))
        256 = shiftSum a b := by
  apply BitVec.eq_of_toNat_eq
  rw [shift_sum_toNat, shiftSum_toNat, Nat.min_comm]

theorem shl_shl (a b x : Word) : Evm.shl a (Evm.shl b x) = Evm.shl (shiftSum a b) x := by
  simp only [Evm.shl_eq, BitVec.shiftLeft_eq', shiftSum_toNat, ← BitVec.shiftLeft_add]
  by_cases h : a.toNat + b.toNat < 256
  · rw [Nat.min_eq_right (by omega), Nat.add_comm]
  · rw [Nat.min_eq_left (by omega), BitVec.shiftLeft_eq_zero (by omega),
      BitVec.shiftLeft_eq_zero (by omega)]

theorem shr_shr (a b x : Word) : Evm.shr a (Evm.shr b x) = Evm.shr (shiftSum a b) x := by
  simp only [Evm.shr_eq, BitVec.ushiftRight_eq', shiftSum_toNat, ← BitVec.shiftRight_add]
  by_cases h : a.toNat + b.toNat < 256
  · rw [Nat.min_eq_right (by omega), Nat.add_comm]
  · rw [Nat.min_eq_left (by omega), BitVec.ushiftRight_eq_zero (by omega),
      BitVec.ushiftRight_eq_zero (by omega)]

theorem sar_sar (a b x : Word) : Evm.sar a (Evm.sar b x) = Evm.sar (shiftSum a b) x := by
  simp only [Evm.sar_eq, BitVec.sshiftRight_eq', shiftSum_toNat, ← BitVec.sshiftRight_add]
  by_cases h : a.toNat + b.toNat < 256
  · rw [Nat.min_eq_right (by omega), Nat.add_comm]
  · rw [Nat.min_eq_left (by omega)]
    exact BitVec.sshiftRight_eq_sshiftRight_of_le (by omega) (by omega)

/-! A shift by one count distributes over bitwise operations and addition. -/

theorem and_shl (n x y : Word) :
    Evm.and (Evm.shl n x) (Evm.shl n y) = Evm.shl n (Evm.and x y) := by
  simp only [Evm.and, Evm.shl_eq, BitVec.shiftLeft_eq', BitVec.shiftLeft_and_distrib]

theorem or_shl (n x y : Word) :
    Evm.or (Evm.shl n x) (Evm.shl n y) = Evm.shl n (Evm.or x y) := by
  simp only [Evm.or, Evm.shl_eq, BitVec.shiftLeft_eq', BitVec.shiftLeft_or_distrib]

theorem xor_shl (n x y : Word) :
    Evm.xor (Evm.shl n x) (Evm.shl n y) = Evm.shl n (Evm.xor x y) := by
  simp only [Evm.xor, Evm.shl_eq, BitVec.shiftLeft_eq', BitVec.shiftLeft_xor_distrib]

theorem add_shl (n x y : Word) :
    Evm.add (Evm.shl n x) (Evm.shl n y) = Evm.shl n (Evm.add x y) := by
  simp only [Evm.add, Evm.shl_eq, BitVec.shiftLeft_eq', BitVec.shiftLeft_add_distrib]

theorem and_shr (n x y : Word) :
    Evm.and (Evm.shr n x) (Evm.shr n y) = Evm.shr n (Evm.and x y) := by
  simp only [Evm.and, Evm.shr_eq, BitVec.ushiftRight_eq', BitVec.ushiftRight_and_distrib]

theorem or_shr (n x y : Word) :
    Evm.or (Evm.shr n x) (Evm.shr n y) = Evm.shr n (Evm.or x y) := by
  simp only [Evm.or, Evm.shr_eq, BitVec.ushiftRight_eq', BitVec.ushiftRight_or_distrib]

theorem xor_shr (n x y : Word) :
    Evm.xor (Evm.shr n x) (Evm.shr n y) = Evm.shr n (Evm.xor x y) := by
  simp only [Evm.xor, Evm.shr_eq, BitVec.ushiftRight_eq', BitVec.ushiftRight_xor_distrib]

theorem and_sar (n x y : Word) :
    Evm.and (Evm.sar n x) (Evm.sar n y) = Evm.sar n (Evm.and x y) := by
  simp only [Evm.and, Evm.sar_eq, BitVec.sshiftRight_eq', BitVec.sshiftRight_and_distrib]

theorem or_sar (n x y : Word) :
    Evm.or (Evm.sar n x) (Evm.sar n y) = Evm.sar n (Evm.or x y) := by
  simp only [Evm.or, Evm.sar_eq, BitVec.sshiftRight_eq', BitVec.sshiftRight_or_distrib]

theorem xor_sar (n x y : Word) :
    Evm.xor (Evm.sar n x) (Evm.sar n y) = Evm.sar n (Evm.xor x y) := by
  simp only [Evm.xor, Evm.sar_eq, BitVec.sshiftRight_eq', BitVec.sshiftRight_xor_distrib]

/-! A cast recovers a value that fits its source width (`signedMask_recover` in #1648). -/

theorem sext_recover (n m x : Word) (h₁ : 1 ≤ n) (h₂ : n < m)
    (hx : Evm.and x (Evm.sub (Evm.shl n 1) 1) = x) :
    Evm.and
      (Evm.and
        (Evm.sar (Evm.sub 256 (Evm.select (Evm.lt n 256) n 256))
          (Evm.shl (Evm.sub 256 (Evm.select (Evm.lt n 256) n 256)) x))
        (Evm.sub (Evm.shl m 1) 1))
      (Evm.sub (Evm.shl n 1) 1) = x := by
  rw [BitVec.le_def] at h₁
  rw [BitVec.lt_def] at h₂
  apply BitVec.eq_of_getLsbD_eq
  intro i hi
  have ex := congrArg (BitVec.getLsbD · i) hx
  simp only [Evm.and, BitVec.getLsbD_and, bits_mask] at ex
  rw [Evm.and, BitVec.getLsbD_and, bits_sext, bits_mask]
  simp only [show (1 : Word).toNat = 1 from rfl] at h₁
  grind

end EvmRules
