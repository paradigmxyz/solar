import SolarTV.MIR.Ops

/-! Integer cleanup can remove a truncation after a lossless zero extension.
The concrete i160/i256 theorem also checks the native bit-vector width model.
The i1 examples pin the difference between truncation and boolean normalization,
and between sign and zero extension. -/

namespace SolarTV.MIR

theorem truncate_extended_address (value : BitVec 160) :
    cast .trunc 160 (cast .zext 256 value) = value := by
  simp [cast]

theorem truncate_even_word : cast .trunc 1 (2#256) = 0#1 := by decide

theorem extend_true_sign : cast .sext 256 (1#1) = -1#256 := by decide

theorem extend_true_zero : cast .zext 256 (1#1) = 1#256 := by decide

end SolarTV.MIR
