import SolarTV.MIR.State.Ops

/-! Local scalar identities and polarity changes for checked MIR. The input
slice and its length are fixed for an activation. Each retained check keeps
its full failure outcome, including the panic code and diagnostic payload. -/

namespace SolarTV.MIR.ScalarChecks
open EvmRules MIR.State

theorem argument_projection (left right : Frame) (index : Nat) (ty : ValueType)
    (same : left.arguments = right.arguments) :
    (readValue left ⟨.argument index, ty⟩).bind (fun v => extractField ty v 1) =
    (readValue right ⟨.argument index, ty⟩).bind (fun v => extractField ty v 1) := by
  simp only [readValue, same]

theorem identities (word : Word) (flag : BitVec 1) :
    MIR.eval .mul word 1 = word ∧ MIR.eval .div word 1 = word ∧
    MIR.eval .or flag 0 = flag ∧ MIR.compare .eq word word = 1 ∧
    MIR.compare .eq (1 : Word) 0 = 0 := by
  exact ⟨BitVec.mul_one word, BitVec.udiv_one, BitVec.or_zero, by simp [MIR.compare], rfl⟩

theorem widen_bit (cfg : Config) (frame : Frame) (flag : BitVec 1) :
    operation cfg frame .word [flag.setWidth 256] = value frame (flag.setWidth 256) ∧
    (0#1).setWidth 256 = 0#256 ∧ (1#1).setWidth 256 = 1#256 := ⟨rfl, rfl, rfl⟩

theorem inactive (cfg : Config) (frame : Frame) (panic : Option Word)
    (message : Option (List SolarTV.Execution.Byte)) :
    operation cfg frame (.check false panic message) [0] = effect frame ∧
    operation cfg frame (.check true panic message) [1] = effect frame := ⟨rfl, rfl⟩

theorem negated (cfg : Config) (frame : Frame) (panic : Option Word)
    (message : Option (List SolarTV.Execution.Byte)) (flag : BitVec 1) :
    operation cfg frame (.check false panic message) [(MIR.compare .eq flag 0).setWidth 256] =
    operation cfg frame (.check true panic message) [flag.setWidth 256] := by
  rcases BitVec.eq_zero_or_eq_one flag with h | h <;> rw [h] <;> rfl

theorem repeated_comparison (cfg : Config) (frame : Frame) (a b : Word) :
    operation cfg frame (.comparison .lt 256) [a,b] =
      value frame ((MIR.compare .lt a b).setWidth 256) := rfl

end SolarTV.MIR.ScalarChecks
