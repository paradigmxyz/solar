import EvmRules.Tactic

/-!
# Lemmas for hand-written rule proofs

Facts about the `Evm` definitions that the scripts in `proofs/` share.
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

/-- An arithmetic right shift by at least the width fills every bit with the sign, the same
as a shift by exactly the width. -/
theorem sshiftRight_saturate {w n : Nat} (x : BitVec w) (h : w ≤ n) :
    x.sshiftRight n = x.sshiftRight w := by
  apply BitVec.eq_of_getLsbD_eq
  intro i hi
  have h₁ : ¬n + i < w := by omega
  have h₂ : ¬w + i < w := by omega
  simp [BitVec.getLsbD_sshiftRight, h₁, h₂]

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

/-- A positive count below the word width gives a nonzero power of two. -/
theorem shl_one_ne_zero (k : Word) (h : k < 256) : Evm.shl k 1 ≠ 0 := by
  rw [Evm.shl_eq]
  bv_decide

end EvmRules
