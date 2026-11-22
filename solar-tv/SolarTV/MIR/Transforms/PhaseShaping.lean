import SolarTV.MIR.Transforms.ContinuedCounter

/-! Memory facts for comparing direct execution across late MIR shaping.
A retained read inside a range already made active by a write preserves the
written memory, including every byte outside the range. -/

namespace SolarTV.MIR.PhaseShaping
open EvmRules SolarTV.Execution MIR.ContinuedCounter

theorem expand_after_write (memory bytes : List Byte) (limit atByte offset size : Nat)
    (nonempty : size > 0) (aligned : memory.length % 32 = 0)
    (limitAligned : limit % 32 = 0) (endAligned : (offset + size) % 32 = 0)
    (bounded : memory.length ≤ 65536) (limitFits : limit ≤ 65536)
    (covered : offset + size ≤ limit) :
    expand (writeBytes (padTo memory limit) atByte bytes) offset size =
      .ok (writeBytes (padTo memory limit) atByte bytes) := by
  have length : (writeBytes (padTo memory limit) atByte bytes).length =
      max memory.length limit := by simp
  have aligned' : (writeBytes (padTo memory limit) atByte bytes).length % 32 = 0 := by
    rw [length]
    exact maxAligned _ _ aligned limitAligned
  have bounded' : (writeBytes (padTo memory limit) atByte bytes).length ≤ 65536 := by
    rw [length]
    omega
  rw [expandAligned _ _ _ nonempty aligned' endAligned bounded' (by omega)]
  rw [padCovered]
  omega

theorem read_padded_write (memory : List Byte) (limit offset : Nat) (word : Word)
    (covered : offset + 32 ≤ limit) :
    readWord (writeBytes (padTo memory limit) offset (wordBytes word)) offset = word := by
  apply readWord_writeWord
  rw [padLength]
  omega

end SolarTV.MIR.PhaseShaping
