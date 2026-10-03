import EvmRules.Word

/-!
# Bit-blasting forms

The shifts state the word-width case as the specification does. Lean's shifts by a
`BitVec` amount already saturate, so these forms are equal and need no comparison.
-/

namespace EvmRules.Evm

theorem shl_eq (shift value : Word) : shl shift value = value <<< shift := by
  unfold shl
  bv_decide

theorem shr_eq (shift value : Word) : shr shift value = value >>> shift := by
  unfold shr
  bv_decide

theorem sar_eq (shift value : Word) : sar shift value = value.sshiftRight' shift := by
  unfold sar
  bv_decide

end EvmRules.Evm
