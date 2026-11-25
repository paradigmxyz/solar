import SolarTV.EVMIR.Suffix
import SolarTV.EVMIR.Transforms.EqIszeroJumpi

/-! Peephole certificates observe stack words at the common continuation and
the truth of a condition at a matched static branch. The context supplies
unchanged prefix execution, dynamic replies and physical branch charges. -/

namespace SolarTV.EVM.ContextualPeephole
open EvmRules

theorem double_iszero_zero (a : Word) :
    Evm.iszero (Evm.iszero a) = 0#256 ↔ a = 0#256 := by
  by_cases zero : a = 0#256 <;> simp [Evm.iszero, zero]

private theorem invert_lt_words (a upper lower : Word) (adjacent : upper.toNat = lower.toNat + 1) :
    Evm.iszero (Evm.lt a upper) = Evm.gt a lower := by
  by_cases below : a.toNat < upper.toNat
  · have n : ¬ lower.toNat < a.toNat := by omega
    simp [Evm.iszero, Evm.lt, Evm.gt, BitVec.ult, below, n]
  · have n : lower.toNat < a.toNat := by omega
    simp [Evm.iszero, Evm.lt, Evm.gt, BitVec.ult, below, n]

theorem invert_lt (a : Word) (upper : Nat) (positive : 0 < upper) (fits : upper < 2^256) :
    Evm.iszero (Evm.lt a (BitVec.ofNat 256 upper)) =
      Evm.gt a (BitVec.ofNat 256 (upper - 1)) := by
  apply invert_lt_words
  have lowerFits : upper - 1 < 2^256 := by omega
  simp [BitVec.toNat_ofNat, Nat.mod_eq_of_lt fits, Nat.mod_eq_of_lt lowerFits]
  omega

theorem odd_word (x : Word) :
    Evm.or (1#256) (x + x) = Evm.sub x (Evm.not x) := by
  have even : (x + x) &&& 1#256 = 0#256 := by
    rw [← BitVec.toNat_inj, BitVec.toNat_and, BitVec.toNat_add]
    change ((x.toNat + x.toNat) % 2^256) &&& (2^1 - 1) = 0
    rw [Nat.and_two_pow_sub_one_eq_mod]
    omega
  change (1#256) ||| (x + x) = x - ~~~x
  rw [BitVec.or_comm,
    ← BitVec.add_eq_or_of_and_eq_zero _ _ even, BitVec.not_eq_neg_add,
    BitVec.sub_eq_add_neg (-x) (1#256), ← BitVec.sub_sub]
  simp only [BitVec.sub_neg]

theorem add_literal (a b : Nat) :
    Evm.add (BitVec.ofNat 256 a) (BitVec.ofNat 256 b) = BitVec.ofNat 256 (a + b) := by
  simp [Evm.add, BitVec.ofNat_add]

end SolarTV.EVM.ContextualPeephole
