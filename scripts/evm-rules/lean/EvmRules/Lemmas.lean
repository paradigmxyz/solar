import EvmRules.Tactic

/-!
# Lemmas for hand-written rule proofs

Facts about the `Evm` definitions that the scripts in `proofs/` share.
-/

namespace EvmRules

/-- `EXP` of the all-ones word, which is `-1`: one for even exponents, itself for odd ones. -/
theorem max_pow (k : Nat) : MAX ^ k = if k % 2 = 0 then 1 else MAX := by
  induction k with
  | zero => rfl
  | succ k ih =>
    rw [BitVec.pow_succ, ih]
    by_cases h : k % 2 = 0
    · rw [ite_eq_left h, ite_eq_right (by omega)]
      exact BitVec.one_mul _
    · rw [ite_eq_right h, ite_eq_left (by omega)]
      decide

/-- A left shift that the same right shift undoes kept every bit: it multiplied by a power of
two without wrapping. -/
theorem toNat_shl_of_shr_shl {s x : Word} (h : Evm.shr s (Evm.shl s x) = x) :
    (Evm.shl s x).toNat = x.toNat * 2 ^ s.toNat := by
  have hd := congrArg BitVec.toNat h
  rw [toNat_shr] at hd
  have hdvd : 2 ^ s.toNat ∣ (Evm.shl s x).toNat := by
    rw [toNat_shl]
    by_cases hs : s.toNat ≤ 256
    · rw [Nat.dvd_mod_iff (Nat.pow_dvd_pow 2 hs)]
      exact Nat.dvd_mul_left _ _
    · rw [Nat.mod_eq_zero_of_dvd (Nat.dvd_mul_left_of_dvd (Nat.pow_dvd_pow 2 (by omega)) _)]
      exact Nat.dvd_zero _
  rw [← Nat.div_mul_cancel hdvd, hd]

theorem shr_shl (s x : Word) : Evm.shr s (Evm.shl s x) = Evm.and x (Evm.shr s MAX) := by
  apply BitVec.eq_of_getLsbD_eq
  intro i hi
  simp only [bits_shr, bits_shl, Evm.and, BitVec.getLsbD_and, bits_max]
  grind

/-- With its top `s` bits clear, `x` survives a left shift by `s`. -/
theorem shr_shl_of_clear {s x : Word} (h : Evm.and (Evm.shr s MAX) x = x) :
    Evm.shr s (Evm.shl s x) = x := by
  rw [shr_shl, Evm.and, BitVec.and_comm]
  exact h

/-- Multiplying by an odd word is invertible modulo 2²⁵⁶. -/
theorem mul_odd_inj {c : Word} (hc : Evm.and c 1 = 1) (x y : Word) :
    Evm.mul x c = Evm.mul y c ↔ x = y := by
  constructor
  · intro h
    have hodd : c.toNat % 2 = 1 := by
      have := congrArg BitVec.toNat hc
      rwa [Evm.and, BitVec.toNat_and, show (1 : Word).toNat = 1 from rfl,
        Nat.and_one_is_mod] at this
    have hcop : Nat.Coprime (2 ^ 256) c.toNat := by
      apply Nat.Coprime.pow_left
      show Nat.gcd 2 c.toNat = 1
      rw [Nat.gcd_rec, hodd]
      rfl
    have h' := congrArg BitVec.toNat h
    rw [toNat_mul, toNat_mul] at h'
    have hx := x.isLt
    have hy := y.isLt
    apply BitVec.eq_of_toNat_eq
    rcases Nat.le_total y.toNat x.toNat with hle | hle
    · have hd : 2 ^ 256 ∣ (x.toNat - y.toNat) * c.toNat := by
        rw [Nat.sub_mul]
        exact Nat.dvd_of_mod_eq_zero (Nat.sub_mod_eq_zero_of_mod_eq h')
      have := Nat.eq_zero_of_dvd_of_lt (hcop.dvd_of_dvd_mul_right hd) (by omega)
      omega
    · have hd : 2 ^ 256 ∣ (y.toNat - x.toNat) * c.toNat := by
        rw [Nat.sub_mul]
        exact Nat.dvd_of_mod_eq_zero (Nat.sub_mod_eq_zero_of_mod_eq h'.symm)
      have := Nat.eq_zero_of_dvd_of_lt (hcop.dvd_of_dvd_mul_right hd) (by omega)
      omega
  · rintro rfl
    rfl

/-- A power of two up to the word width divides a word's signed value exactly when it divides
its unsigned value. -/
theorem pow_dvd_toInt_iff (x : Word) {k : Nat} (hk : k ≤ 256) :
    ((2 ^ k : Nat) : Int) ∣ x.toInt ↔ x.toNat % 2 ^ k = 0 := by
  have hw : ((2 ^ k : Nat) : Int) ∣ ((2 ^ 256 : Nat) : Int) :=
    Int.natCast_dvd_natCast.mpr (Nat.pow_dvd_pow 2 hk)
  rw [← Nat.dvd_iff_mod_eq_zero, ← Int.natCast_dvd_natCast, BitVec.toInt_eq_toNat_cond]
  split
  · exact Iff.rfl
  · constructor
    · intro h
      simpa using Int.dvd_add h hw
    · intro h
      exact Int.dvd_sub h hw

/-- The mask below a power of two `m = 1 << k`. -/
theorem toNat_mask_of_pow {m k : Word} (h : k < 256) (hm : m = Evm.shl k 1) :
    (Evm.sub m 1).toNat = 2 ^ k.toNat - 1 := by
  subst hm
  have hp : 2 ^ k.toNat < 2 ^ 256 := Nat.pow_lt_pow_right (by decide) (show k.toNat < 256 from h)
  have h1 : 1 ≤ 2 ^ k.toNat := Nat.one_le_two_pow
  rw [Evm.sub, BitVec.toNat_sub, toNat_shl_one h, show (1 : Word).toNat = 1 from rfl]
  omega

/-- A signed remainder by a power of two below the sign bit is zero exactly when the low bits
are. -/
theorem smod_pow_eq_zero_iff (x m k : Word) (h₁ : k < 256) (h₂ : m = Evm.shl k 1)
    (h₃ : k.toNat < 255) : Evm.smod x m = 0 ↔ Evm.and x (Evm.sub m 1) = 0 := by
  have hmask := toNat_mask_of_pow h₁ h₂
  subst h₂
  have hc := toNat_shl_one h₁
  have hp : 2 ^ k.toNat < 2 ^ 255 := Nat.pow_lt_pow_right (by decide) h₃
  have hcm : (Evm.shl k 1).msb = false := by
    rw [BitVec.msb_eq_false_iff_two_mul_lt, hc]
    omega
  have hm0 : Evm.shl k 1 ≠ 0 := by
    intro e
    have := congrArg BitVec.toNat e
    rw [hc] at this
    have := Nat.one_le_two_pow (n := k.toNat)
    simp_all
  rw [Evm.smod, beq_eq_false_iff_ne.mpr hm0, Bool.cond_false, ← BitVec.toInt_inj,
    BitVec.toInt_srem, BitVec.toInt_eq_toNat_of_msb hcm, hc, show (0 : Word).toInt = 0 from rfl,
    ← Int.dvd_iff_tmod_eq_zero, pow_dvd_toInt_iff x (by omega), ← BitVec.toNat_inj, Evm.and,
    BitVec.toNat_and, hmask, Nat.and_two_pow_sub_one_eq_mod]
  rfl

end EvmRules
