import SolarTV.MIR.Ops

/-! A dominating bound makes the unsigned increment's overflow check redundant.
The operation otherwise retains its defined Panic(0x11) outcome; it is never
modeled as undefined behavior merely because a check fails. -/

namespace SolarTV.MIR

theorem increment_below_bound (value : BitVec 256) (bound : value.ult 100 = true) :
    checked .add false value 1 = Outcome.returned (value + 1) := by
  have h : value.toNat < 100 := by simpa [BitVec.ult_iff_toNat_lt] using bound
  simp [checked, BitVec.uaddOverflow] <;> omega

/-- Replacing a defined failure by success fails refinement in either direction. -/
theorem cannot_drop_overflow :
    ¬ refines (checked .add false (-1#256) 1) (Outcome.returned (0#256)) := by
  simp [checked, BitVec.uaddOverflow]

end SolarTV.MIR
