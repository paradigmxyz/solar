import SolarTV.MIR.State.Ops
import SolarTV.MIR.Transforms.RoundedExtent

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

theorem false_disjunct {Result : Type} (flag : BitVec 1) (left : Bool)
    (yes no : Result) :
    MIR.branch (if left then MIR.eval .or 0 flag else MIR.eval .or flag 0) yes no =
      MIR.branch flag yes no := by
  cases left <;> simp [MIR.eval]

theorem bounded_index (cfg : Config) (frame : Frame) (index length limit : Word)
    (small : limit.toNat < 2^255)
    (nowrap : MIR.compare .lt (MIR.eval .add length 1) length = 0)
    (size : MIR.compare .gt (MIR.eval .add length 1) limit = 0)
    (inside : MIR.compare .lt index length = 1)
    (rangePanic limitPanic : Option Word) (rangeMessage limitMessage : Option (List SolarTV.Execution.Byte)) :
    MIR.checked .add false index 1 = Outcome.returned (index + 1) ∧
    operation cfg frame (.check true rangePanic rangeMessage)
      [(MIR.compare .lt index length).setWidth 256] = effect frame ∧
    operation cfg frame (.check false limitPanic limitMessage)
      [(MIR.compare .gt index limit).setWidth 256] = effect frame := by
  have lower : length.toNat ≤ (length + 1#256).toNat := by
    simpa [MIR.compare, MIR.eval, BitVec.ult_iff_toNat_lt] using nowrap
  have upper : (length + 1#256).toNat ≤ limit.toNat := by
    simpa [MIR.compare, MIR.eval, BitVec.ult_iff_toNat_lt] using size
  have below : index.toNat < length.toNat := by
    simpa [MIR.compare, BitVec.ult_iff_toNat_lt] using inside
  have wide : ¬limit.toNat < index.toNat := by omega
  have clear : MIR.compare .gt index limit = 0 := by
    simp [MIR.compare, BitVec.ult_iff_toNat_lt, wide]
  refine ⟨?_, by rw [inside]; rfl, by rw [clear]; rfl⟩
  have paid : index.toNat + 1 < 2^256 := by omega
  simp [MIR.checked, BitVec.uaddOverflow, Nat.not_le_of_gt paid]

/-- The false edge of an unsigned length check against a shifted word cannot
reach the overflow arm of a subsequent increment of that length. -/
theorem bounded_increment {Result : Type} (word count length : Word)
    (positive : 0 < count.toNat) (rejected overflow ok : Result) :
    MIR.branch (MIR.compare .lt (MIR.shift .shr word count) length) rejected
      (MIR.branch (MIR.compare .eq (MIR.eval .add length 1) 0) overflow ok) =
    MIR.branch (MIR.compare .lt (MIR.shift .shr word count) length) rejected ok := by
  by_cases large : (MIR.shift .shr word count).toNat < length.toNat
  · simp [MIR.branch, MIR.compare, BitVec.ult_iff_toNat_lt, large]
  · have fits := MIR.RoundedExtent.shifted_length word count positive
    have sumFits : length.toNat + 1 < 2^256 := by omega
    have nonzero : length + 1#256 ≠ 0#256 := by
      intro equal
      have natural := congrArg BitVec.toNat equal
      rw [BitVec.toNat_add] at natural
      change (length.toNat + 1) % 2^256 = 0 at natural
      rw [Nat.mod_eq_of_lt sumFits] at natural
      exact Nat.succ_ne_zero _ natural
    simp [MIR.branch, MIR.compare, MIR.eval, BitVec.ult_iff_toNat_lt, large, nonzero]

end SolarTV.MIR.ScalarChecks
