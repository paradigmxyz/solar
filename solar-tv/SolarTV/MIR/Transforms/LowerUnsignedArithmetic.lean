import SolarTV.MIR.Transforms.ContinuedCounter
import SolarTV.State.MemoryLaws

/-! Relate unsigned checked arithmetic to the emitted overflow and zero tests.
The panic stores publish the semantic failure bytes in bounded aligned memory.
The terminal observation discards scratch memory, and physical pricing is separate. -/
namespace SolarTV.MIR.LowerUnsignedArithmetic
open EvmRules SolarTV.Execution MIR.ContinuedCounter
set_option maxRecDepth 8192

theorem word_add_overflow (left right : Word) :
    left.uaddOverflow right = (left + right).ult left := by
  have leftFits := left.isLt
  have rightFits := right.isLt
  rw [Bool.eq_iff_iff, BitVec.ult_iff_toNat_lt, BitVec.toNat_add]
  simp only [BitVec.uaddOverflow, decide_eq_true_eq]
  omega

theorem narrow_add_overflow {width : Nat} (left right : BitVec width) (fits : width < 256) :
    left.uaddOverflow right =
      (BitVec.ofNat 256 (2^width - 1)).ult (left.setWidth 256 + right.setWidth 256) := by
  have limit := Nat.pow_lt_pow_right (by decide : 1 < 2) fits
  have scaled : 2 * 2^width ≤ 2^256 := by
    have exp : width + 1 ≤ 256 := by omega
    have h := Nat.pow_le_pow_right (by decide : 0 < 2) exp
    rw [Nat.pow_succ] at h
    omega
  have leftFits := left.isLt
  have rightFits := right.isLt
  rw [Bool.eq_iff_iff, BitVec.ult_iff_toNat_lt, BitVec.toNat_add,
    BitVec.toNat_setWidth_of_le (by omega), BitVec.toNat_setWidth_of_le (by omega)]
  simp only [BitVec.uaddOverflow, decide_eq_true_eq, BitVec.toNat_ofNat]
  rw [Nat.mod_eq_of_lt (by omega : 2^width - 1 < 2^256),
    Nat.mod_eq_of_lt (by omega : left.toNat + right.toNat < 2^256)]
  omega

theorem narrow_sub_overflow {width : Nat} (left right : BitVec width) (fits : width ≤ 256) :
    left.usubOverflow right = (left.setWidth 256).ult (right.setWidth 256) := by
  simp [BitVec.usubOverflow, BitVec.ult, BitVec.toNat_setWidth_of_le fits]

theorem word_mul_checked (left right : Word) :
    (right == 0 || ((left * right) / right) == left) = !left.umulOverflow right := by
  rw [Bool.eq_iff_iff]
  simp only [Bool.or_eq_true, beq_iff_eq, ← BitVec.toNat_inj,
    BitVec.toNat_udiv, BitVec.toNat_mul]
  change (right.toNat = 0 ∨ (left.toNat * right.toNat) % 2^256 / right.toNat = left.toNat)
    ↔ (!(decide (2^256 ≤ left.toNat * right.toNat))) = true
  simp only [Bool.not_eq_true', decide_eq_false_iff_not]
  by_cases zero : right.toNat = 0
  · simp [zero]
  · have positive : 0 < right.toNat := by omega
    simp only [zero, false_or]
    by_cases overflow : 2^256 ≤ left.toNat * right.toNat
    · have wrapped : (left.toNat * right.toNat) % 2^256 < left.toNat * right.toNat :=
        Nat.lt_of_lt_of_le (Nat.mod_lt _ (by decide)) overflow
      have quotient : (left.toNat * right.toNat) % 2^256 / right.toNat < left.toNat :=
        (Nat.div_lt_iff_lt_mul positive).mpr wrapped
      simp [overflow, Nat.ne_of_lt quotient]
    · have exactProduct : left.toNat * right.toNat < 2^256 := by omega
      simp [overflow, Nat.mod_eq_of_lt exactProduct, Nat.mul_div_cancel _ positive]

theorem add256 (left right : Word) :
    MIR.checked .add false left right =
      (if (left + right).ult left then .reverted (.panic 17) else .returned (left + right)) := by
  simp [MIR.checked, word_add_overflow]

theorem sub256 (left right : Word) :
    MIR.checked .sub false left right =
      (if left.ult right then .reverted (.panic 17) else .returned (left - right)) := rfl

theorem add128 (left right : BitVec 128) :
    MIR.checked .add false left right =
      (if (BitVec.ofNat 256 (2^128-1)).ult (left.setWidth 256 + right.setWidth 256) then
        .reverted (.panic 17)
      else .returned ((left.setWidth 256 + right.setWidth 256).setWidth 128)) := by
  simp [MIR.checked, narrow_add_overflow left right (by decide), BitVec.setWidth_add]

theorem sub128 (left right : BitVec 128) :
    MIR.checked .sub false left right =
      (if (left.setWidth 256).ult (right.setWidth 256) then .reverted (.panic 17)
      else .returned ((left.setWidth 256 - right.setWidth 256).setWidth 128)) := by
  simp [MIR.checked, narrow_sub_overflow left right (by decide), BitVec.sub_eq_add_neg,
    BitVec.setWidth_add, BitVec.setWidth_neg_of_le]

theorem mul256 (left right : Word) :
    MIR.checked .mul false left right =
      (if !(right == 0 || (left * right) / right == left) then .reverted (.panic 17)
      else .returned (left * right)) := by
  rw [word_mul_checked]
  simp [MIR.checked]

theorem div256 (left right : Word) :
    MIR.checked .div false left right =
      (if right == 0 then .reverted (.panic 18) else .returned (left / right)) := by
  simp [MIR.checked]

def panicPayload (memory : List Byte) (code : Word) : Except String (List Byte) := do
  let active ← expand memory 32 32
  let written := writeBytes active 32 (wordBytes code)
  let active ← expand written 0 32
  let staged := writeBytes active 0 (wordBytes 0x4e487b71)
  let active ← expand staged 28 36
  return readBytes active 28 36

private theorem stage (memory : List Byte) (code : Word) (covers : 64 ≤ memory.length) :
    writeBytes (writeBytes memory 32 (wordBytes code)) 0 (wordBytes 0x4e487b71) =
      (wordBytes 0x4e487b71 ++ wordBytes code) ++ memory.drop 64 := by
  have codeFits : (wordBytes code).length ≤ memory.length-32 := by simp; omega
  have selectorFits : (wordBytes 0x4e487b71).length ≤
      (writeBytes memory 32 (wordBytes code)).length := by simp; omega
  have first : writeBytes memory 32 (wordBytes code) =
      memory.take 32 ++ wordBytes code ++ memory.drop 64 := by
    simp [writeBytes, List.take_of_length_le codeFits]
  have frontLength : (memory.take 32).length = 32 := by simp; omega
  unfold writeBytes
  simp only [List.take_zero, List.nil_append, Nat.sub_zero, Nat.zero_add]
  change (wordBytes 0x4e487b71).take (writeBytes memory 32 (wordBytes code)).length ++
      (writeBytes memory 32 (wordBytes code)).drop (wordBytes 0x4e487b71).length = _
  rw [List.take_of_length_le selectorFits, first]
  simp only [wordBytes_length, List.append_assoc]
  have drop : (memory.take 32 ++ (wordBytes code ++ memory.drop 64)).drop 32 =
      wordBytes code ++ memory.drop 64 := by
    simpa only [frontLength] using
      (List.drop_append_length (l₁ := memory.take 32) (l₂ := wordBytes code ++ memory.drop 64))
  rw [drop]

private theorem readPrefix (head suffix : List Byte) (offset size : Nat) (within : offset+size ≤ head.length) :
    readBytes (head ++ suffix) offset size = readBytes head offset size := by
  unfold readBytes
  apply List.map_congr_left
  intro index member
  have fits : offset + index < head.length := by
    have := List.mem_range.mp member
    omega
  simp only [List.getElem?_append_left fits]

theorem panic_payload (memory : List Byte) (code : Word)
    (aligned : memory.length%32 = 0) (bounded : memory.length ≤ 65536) :
    panicPayload memory code = .ok (readBytes (wordBytes 0x4e487b71 ++ wordBytes code) 28 36) := by
  let padded := padTo memory 64
  let written := writeBytes padded 32 (wordBytes code)
  let staged := writeBytes written 0 (wordBytes 0x4e487b71)
  have expanded : expand memory 32 32 = .ok padded :=
    expandAligned _ 32 32 (by decide) aligned (by decide) bounded (by decide)
  have length : padded.length = max memory.length 64 := padLength _ _
  have padAligned : padded.length%32 = 0 := by
    rw [length]
    exact maxAligned _ _ aligned (by decide)
  have padBound : padded.length ≤ 65536 := by rw [length]; omega
  have full : 64 ≤ padded.length := by rw [length]; omega
  have e0 : expand written 0 32 = .ok written := by
    have lens : written.length = padded.length := writeBytes_length _ _ _
    rw [expandAligned _ 0 32 (by decide) (by simpa only [lens] using padAligned)
      (by decide) (by simpa only [lens] using padBound) (by decide),
      padCovered _ _ (by rw [lens]; omega)]
  have e1 : expand staged 28 36 = .ok staged := by
    have lens : staged.length = padded.length := by simp [staged,written]
    rw [expandAligned _ 28 36 (by decide) (by simpa only [lens] using padAligned)
      (by decide) (by simpa only [lens] using padBound) (by decide),
      padCovered _ _ (by rw [lens]; omega)]
  unfold panicPayload
  rw [expanded]
  dsimp only [Bind.bind, Except.bind]
  rw [show expand (writeBytes padded 32 (wordBytes code)) 0 32 = _ from e0]
  dsimp only [Bind.bind, Except.bind]
  rw [show expand staged 28 36 = _ from e1]
  dsimp only [Pure.pure, Except.pure, Bind.bind, Except.bind]
  change Except.ok (readBytes staged 28 36) = _
  rw [show staged = _ from stage padded code full, readPrefix _ _ _ _ (by simp)]

theorem panic_17 (memory : List Byte) (aligned : memory.length%32 = 0) (bound : memory.length ≤ 65536) :
    panicPayload memory 17 = .ok (MIR.State.failureData (.panic 17)) := by
  rw [panic_payload _ _ aligned bound]
  rfl

theorem panic_18 (memory : List Byte) (aligned : memory.length%32 = 0) (bound : memory.length ≤ 65536) :
    panicPayload memory 18 = .ok (MIR.State.failureData (.panic 18)) := by
  rw [panic_payload _ _ aligned bound]
  rfl

end SolarTV.MIR.LowerUnsignedArithmetic
