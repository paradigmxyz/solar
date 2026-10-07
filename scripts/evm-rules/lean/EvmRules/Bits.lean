import EvmRules.Arith
import EvmRules.Bitblast

/-!
# Word operations bit by bit

Bit `i` of every word operation `evm_bits` (`Tactic.lean`) reads, as a bit of its operand at
an index computed from the shift count or byte index. Bit-blasting builds a barrel shifter for
every variable shift count and a 31-way choice for every variable `SIGNEXTEND` index, which
SAT solvers handle poorly once two of them interact. Stated bit by bit, a rule over variable
counts becomes linear arithmetic over the indices instead.
-/

namespace EvmRules

theorem signExtend_low_bits (x : Word) (w : Nat) (hw : 0 < w) (i : Nat) (hi : i < 256) :
    ((x.setWidth w).signExtend 256).getLsbD i = x.getLsbD (min i (w - 1)) := by
  rw [BitVec.getLsbD_signExtend]
  by_cases h : i < w
  · simp [hi, h, Nat.min_eq_left (show i ≤ w - 1 by omega)]
  · simp [hi, h, BitVec.msb_setWidth, Nat.min_eq_right (show w - 1 ≤ i by omega), hw]

/-- `SIGNEXTEND n x` repeats bit `8n + 7` of `x` upward; every index from 31 on keeps `x`. -/
theorem bits_signextend (n x : Word) (i : Nat) :
    (Evm.signextend n x).getLsbD i =
      (decide (i < 256) && x.getLsbD (min i (n.toNat * 8 + 7))) := by
  by_cases hi : i < 256
  · simp only [hi, decide_true, Bool.true_and]
    unfold Evm.signextend
    by_cases h : n.toNat < 31
    · have hn : n = BitVec.ofNat 256 n.toNat := by simp
      generalize n.toNat = k at h hn
      subst hn
      rcases k with _ | _ | _ | _ | _ | _ | _ | _ | _ | _ | _ | _ | _ | _ | _ | _ | _ | _ | _ |
        _ | _ | _ | _ | _ | _ | _ | _ | _ | _ | _ | _ | k
      all_goals first
        | omega
        | simp (disch := omega) [signExtend_low_bits, -BitVec.getLsbD_eq_getElem]
    · have hne : ∀ k : Word, k.toNat < 31 → (n == k) = false := by
        intro k hk
        simp only [beq_eq_false_iff_ne, ne_eq, BitVec.toNat_eq]
        omega
      simp (disch := simp) only [hne, Bool.cond_false]
      rw [Nat.min_eq_left (by omega)]
  · simp [hi, BitVec.getLsbD_of_ge _ _ (by omega : 256 ≤ i)]

theorem bits_shl (s x : Word) (i : Nat) :
    (Evm.shl s x).getLsbD i =
      (decide (i < 256) && !decide (i < s.toNat) && x.getLsbD (i - s.toNat)) := by
  rw [Evm.shl_eq, BitVec.getLsbD_shiftLeft']

/-- Bits shifted in from beyond the word are zero, also once the count reaches the width. -/
theorem bits_shr (s x : Word) (i : Nat) : (Evm.shr s x).getLsbD i = x.getLsbD (s.toNat + i) := by
  rw [Evm.shr_eq, BitVec.ushiftRight_eq', BitVec.getLsbD_ushiftRight]

/-- Bits shifted in from beyond the word copy the sign bit 255. -/
theorem bits_sar (s x : Word) (i : Nat) :
    (Evm.sar s x).getLsbD i = (decide (i < 256) && x.getLsbD (min (s.toNat + i) 255)) := by
  rw [Evm.sar_eq, BitVec.getLsbD_sshiftRight', BitVec.msb_eq_getLsbD_last]
  by_cases hi : i < 256
  · by_cases h : s.toNat + i < 256
    · simp [hi, h, Nat.min_eq_left (show s.toNat + i ≤ 255 by omega)]
    · simp [hi, h, Nat.min_eq_right (show 255 ≤ s.toNat + i by omega)]
  · simp [hi, show 256 ≤ i by omega]

/-- The low-bits mask `(1 << n) - 1`: every bit below `n`, so all of them from 256 on. -/
theorem bits_mask (n : Word) (i : Nat) :
    (Evm.sub (Evm.shl n 1) 1).getLsbD i = (decide (i < 256) && decide (i < n.toNat)) := by
  rw [Evm.sub, Evm.shl_eq, BitVec.shiftLeft_eq', show (1 : Word) = 1#256 from rfl,
    ← BitVec.not_neg, ← BitVec.shiftLeft_neg]
  simp only [BitVec.getLsbD_not, BitVec.getLsbD_shiftLeft, BitVec.neg_one_eq_allOnes,
    BitVec.getLsbD_allOnes]
  by_cases hi : i < 256 <;> simp [hi] <;> omega

theorem bits_max (i : Nat) :
    (0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff : Word).getLsbD i =
      decide (i < 256) := by
  rw [show (0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff : Word) =
    BitVec.allOnes 256 from rfl, BitVec.getLsbD_allOnes]

theorem bits_address_mask (i : Nat) :
    (0xffffffffffffffffffffffffffffffffffffffff : Word).getLsbD i = decide (i < 160) := by
  rw [show (0xffffffffffffffffffffffffffffffffffffffff : Word) = BitVec.ofNat 256 (2 ^ 160 - 1)
    from rfl, BitVec.getLsbD_ofNat, Nat.testBit_two_pow_sub_one]
  by_cases h : i < 160 <;> simp [h] <;> omega

theorem bits_one (i : Nat) : (1 : Word).getLsbD i = decide (i = 0) := by
  rw [show (1 : Word) = 1#256 from rfl, BitVec.getLsbD_one]
  simp

theorem bits_zero (i : Nat) : (0 : Word).getLsbD i = false := by
  rw [show (0 : Word) = 0#256 from rfl, BitVec.getLsbD_zero]

/-! The indices above are numbers; these state the counts the readers compute as numbers. -/

/-- The smaller of two words, as the e-graph selects it. -/
theorem toNat_select_lt (a b : Word) :
    (Evm.select (Evm.lt a b) a b).toNat = min a.toNat b.toNat := by
  unfold Evm.select Evm.lt
  by_cases h : a.toNat < b.toNat <;> simp [BitVec.ult, h] <;> omega

/-- The larger of two words, as the e-graph selects it. -/
theorem toNat_select_gt (a b : Word) :
    (Evm.select (Evm.gt a b) a b).toNat = max a.toNat b.toNat := by
  unfold Evm.select Evm.gt
  by_cases h : b.toNat < a.toNat <;> simp [BitVec.ult, h] <;> omega

theorem toNat_shr (s x : Word) : (Evm.shr s x).toNat = x.toNat / 2 ^ s.toNat := by
  rw [Evm.shr_eq, BitVec.ushiftRight_eq', BitVec.toNat_ushiftRight, Nat.shiftRight_eq_div_pow]

theorem toNat_shl (s x : Word) : (Evm.shl s x).toNat = x.toNat * 2 ^ s.toNat % 2 ^ 256 := by
  rw [Evm.shl_eq, BitVec.shiftLeft_eq', BitVec.toNat_shiftLeft, Nat.shiftLeft_eq]

theorem toNat_shl_one {k : Word} (h : k < 256) : (Evm.shl k 1).toNat = 2 ^ k.toNat := by
  rw [toNat_shl, show (1 : Word).toNat = 1 from rfl, Nat.one_mul,
    Nat.mod_eq_of_lt (Nat.pow_lt_pow_right (by decide) (show k.toNat < 256 from h))]

/-- A count that is a whole number of bytes, as the readers test it. -/
theorem and_seven_eq_zero (s : Word) : Evm.and s 7 = 0 ↔ s.toNat % 8 = 0 := by
  rw [Evm.and, ← BitVec.toNat_inj, BitVec.toNat_and, show (7 : Word).toNat = 2 ^ 3 - 1 from rfl,
    Nat.and_two_pow_sub_one_eq_mod]
  rfl

/-- Multiplication by the low-bits mask negates within those bits. -/
theorem mul_low_mask {lo hi : Nat} (x : BitVec (hi + lo)) :
    (x * (BitVec.allOnes lo).setWidth (hi + lo)) &&& (BitVec.allOnes lo).setWidth (hi + lo) =
      (-x) &&& (BitVec.allOnes lo).setWidth (hi + lo) := by
  rw [BitVec.and_setWidth_allOnes, BitVec.and_setWidth_allOnes]
  congr 1
  rw [BitVec.setWidth_mul _ _ (by omega), BitVec.setWidth_neg_of_le (by omega)]
  simp [← BitVec.neg_one_eq_allOnes, BitVec.mul_neg]

end EvmRules
