import EvmRules.Word

/-!
# Bit-blasting forms

The shifts state the word-width case as the specification does. Lean's shifts by a
`BitVec` amount already saturate, so these forms are equal and need no comparison.
-/

namespace EvmRules.Evm

theorem shl_eq (shift value : Word) : shl shift value = value <<< shift := by
  unfold shl
  by_cases h : shift.toNat < 256
  · simp [BitVec.ult, h]
  · have h' : shift.ult 256 = false := by simp [BitVec.ult]; omega
    rw [h', Bool.cond_false, BitVec.shiftLeft_eq', BitVec.shiftLeft_eq_zero (by omega)]
    rfl

theorem shr_eq (shift value : Word) : shr shift value = value >>> shift := by
  unfold shr
  by_cases h : shift.toNat < 256
  · simp [BitVec.ult, h]
  · have h' : shift.ult 256 = false := by simp [BitVec.ult]; omega
    rw [h', Bool.cond_false, BitVec.ushiftRight_eq', BitVec.ushiftRight_eq_zero (by omega)]
    rfl

theorem sar_eq (shift value : Word) : sar shift value = value.sshiftRight' shift := by
  unfold sar
  by_cases h : shift.toNat < 256
  · simp [BitVec.ult, h]
  · have h' : shift.ult 256 = false := by simp [BitVec.ult]; omega
    rw [h', Bool.cond_false]
    apply BitVec.eq_of_getLsbD_eq
    intro i hi
    rw [BitVec.getLsbD_sshiftRight', ite_eq_right (by omega)]
    cases hm : value.msb
    · simp
    · rw [Bool.cond_true, show (-1 : Word) = BitVec.allOnes 256 by decide, BitVec.getLsbD_allOnes]
      simp [hi]

end EvmRules.Evm
