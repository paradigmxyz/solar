import EvmRules.Bits

/-!
# A second transcription of the EVM semantics

The definitions of paradigmxyz/solar#1648, which transcribed the same execution specifications
independently of `Word.lean`: shifts by a natural count, zero divisors as an explicit case,
`SIGNEXTEND` as a pair of shifts, and comparisons as the `if` terms that its printer wrote.
Every theorem below proves that the definition the rules are checked against agrees with this
one on all inputs, so a transcription error would have to be made twice, in two different
forms, to go unnoticed. The proofs avoid `bv_decide` and rely on Lean's kernel alone.
-/

namespace EvmRules.Reference

def shl (count value : Word) : Word :=
  if count < 256#256 then value <<< count.toNat else 0#256

def shr (count value : Word) : Word :=
  if count < 256#256 then value >>> count.toNat else 0#256

def sar (count value : Word) : Word :=
  if count < 256#256 then value.sshiftRight count.toNat else value.sshiftRight 256

def div (a b : Word) : Word := if b = 0#256 then 0#256 else a / b

def mod (a b : Word) : Word := if b = 0#256 then 0#256 else a % b

def sdiv (a b : Word) : Word := if b = 0#256 then 0#256 else a.sdiv b

def smod (a b : Word) : Word := if b = 0#256 then 0#256 else a.srem b

def addmod (a b n : Word) : Word :=
  if n = 0#256 then 0#256 else ((a.setWidth 512 + b.setWidth 512) % n.setWidth 512).setWidth 256

def mulmod (a b n : Word) : Word :=
  if n = 0#256 then 0#256 else ((a.setWidth 512 * b.setWidth 512) % n.setWidth 512).setWidth 256

def exp (a b : Word) : Word := a ^ b.toNat

def byte (index value : Word) : Word :=
  if index < 32#256 then shr ((31#256 - index) * 8#256) value &&& 255#256 else 0#256

def signextend (index value : Word) : Word :=
  if index < 31#256 then sar (248#256 - index * 8#256) (shl (248#256 - index * 8#256) value)
  else value

def clz (value : Word) : Word := value.clz

/-! The comparison and selection terms #1648's printer wrote. -/

def lt (a b : Word) : Word := if a < b then 1 else 0

def gt (a b : Word) : Word := if a > b then 1 else 0

def slt (a b : Word) : Word := if BitVec.slt a b = true then 1 else 0

def sgt (a b : Word) : Word := if BitVec.slt b a = true then 1 else 0

def eq (a b : Word) : Word := if a = b then 1 else 0

def ne (a b : Word) : Word := if a ≠ b then 1 else 0

def iszero (a : Word) : Word := if a = 0 then 1 else 0

def select (c a b : Word) : Word := if c ≠ 0 then a else b

end EvmRules.Reference

namespace EvmRules

/-! Every operation agrees with the second transcription. -/

theorem shl_agrees (count value : Word) : Evm.shl count value = Reference.shl count value := by
  rw [Evm.shl_eq, BitVec.shiftLeft_eq']
  unfold Reference.shl
  split
  · rfl
  · rename_i h
    rw [BitVec.shiftLeft_eq_zero (by simp only [BitVec.lt_def] at h; simp at h; omega)]

theorem shr_agrees (count value : Word) : Evm.shr count value = Reference.shr count value := by
  rw [Evm.shr_eq, BitVec.ushiftRight_eq']
  unfold Reference.shr
  split
  · rfl
  · rename_i h
    rw [BitVec.ushiftRight_eq_zero (by simp only [BitVec.lt_def] at h; simp at h; omega)]

theorem sar_agrees (count value : Word) : Evm.sar count value = Reference.sar count value := by
  rw [Evm.sar_eq, BitVec.sshiftRight_eq']
  unfold Reference.sar
  split
  · rfl
  · rename_i h
    have hc : 256 ≤ count.toNat := by
      simp only [BitVec.lt_def] at h
      simp at h
      omega
    exact BitVec.sshiftRight_eq_sshiftRight_of_le hc (by omega)

theorem div_agrees (a b : Word) : Evm.div a b = Reference.div a b := by
  unfold Evm.div Reference.div
  split
  · rename_i h
    subst h
    exact BitVec.udiv_zero
  · rfl

theorem mod_agrees (a b : Word) : Evm.mod a b = Reference.mod a b := by
  unfold Evm.mod Reference.mod
  by_cases h : b = 0 <;> simp [h]

theorem sdiv_agrees (a b : Word) : Evm.sdiv a b = Reference.sdiv a b := by
  unfold Evm.sdiv Reference.sdiv
  split
  · rename_i h
    subst h
    exact BitVec.sdiv_zero
  · rfl

theorem smod_agrees (a b : Word) : Evm.smod a b = Reference.smod a b := by
  unfold Evm.smod Reference.smod
  by_cases h : b = 0 <;> simp [h]

theorem addmod_agrees (a b n : Word) : Evm.addmod a b n = Reference.addmod a b n := by
  unfold Evm.addmod Reference.addmod
  by_cases h : n = 0 <;> simp [h]

theorem mulmod_agrees (a b n : Word) : Evm.mulmod a b n = Reference.mulmod a b n := by
  unfold Evm.mulmod Reference.mulmod
  by_cases h : n = 0 <;> simp [h]

theorem exp_agrees (a b : Word) : Evm.exp a b = Reference.exp a b := rfl

theorem clz_agrees (a : Word) : Evm.clz a = Reference.clz a := rfl

theorem byte_agrees (index value : Word) : Evm.byte index value = Reference.byte index value := by
  unfold Evm.byte Reference.byte
  by_cases h : index.toNat < 32
  · have hc : ((31#256 - index) * 8#256).toNat < 256 := by bv_omega
    rw [show index.ult 32 = true by simp [BitVec.ult, h],
      ite_eq_left (show index < 32#256 from h), ← shr_agrees, Evm.shr_eq]
    rfl
  · rw [show index.ult 32 = false by simp [BitVec.ult]; omega,
      ite_eq_right (show ¬index < 32#256 from h)]
    rfl

theorem signextend_agrees (index value : Word) :
    Evm.signextend index value = Reference.signextend index value := by
  unfold Reference.signextend
  rw [← shl_agrees, ← sar_agrees]
  apply BitVec.eq_of_getLsbD_eq
  intro i hi
  rw [bits_signextend]
  split
  · rename_i h
    have hn : index.toNat < 31 := h
    have hs : (248#256 - index * 8#256).toNat = 248 - index.toNat * 8 := by bv_omega
    rw [bits_sar, bits_shl, hs]
    have e₁ : min (248 - index.toNat * 8 + i) 255 < 256 := by omega
    have e₂ : ¬min (248 - index.toNat * 8 + i) 255 < 248 - index.toNat * 8 := by omega
    have e₃ : min (248 - index.toNat * 8 + i) 255 - (248 - index.toNat * 8) =
        min i (index.toNat * 8 + 7) := by omega
    simp only [hi, e₁, e₂, e₃, decide_true, decide_false, Bool.not_false, Bool.true_and]
  · rename_i h
    have hn : 31 ≤ index.toNat := by
      simp only [BitVec.lt_def] at h
      simp at h
      omega
    simp [hi, Nat.min_eq_left (show i ≤ index.toNat * 8 + 7 by omega)]

theorem lt_agrees (a b : Word) : Evm.lt a b = Reference.lt a b := by
  unfold Evm.lt Reference.lt
  rw [show a.ult b = decide (a < b) by simp [BitVec.ult, BitVec.lt_def]]
  by_cases h : a < b <;> simp [h]

theorem gt_agrees (a b : Word) : Evm.gt a b = Reference.gt a b := by
  unfold Evm.gt Reference.gt
  rw [show b.ult a = decide (a > b) by simp [BitVec.ult, BitVec.lt_def]]
  by_cases h : a > b <;> simp [h]

theorem slt_agrees (a b : Word) : Evm.slt a b = Reference.slt a b := by
  unfold Evm.slt Reference.slt
  cases a.slt b <;> rfl

theorem sgt_agrees (a b : Word) : Evm.sgt a b = Reference.sgt a b := by
  unfold Evm.sgt Reference.sgt
  cases b.slt a <;> rfl

theorem eq_agrees (a b : Word) : Evm.eq a b = Reference.eq a b := by
  unfold Evm.eq Reference.eq
  by_cases h : a = b <;> simp [h]

theorem ne_agrees (a b : Word) : Evm.ne a b = Reference.ne a b := by
  unfold Evm.ne Reference.ne
  by_cases h : a = b <;> simp [h]

theorem iszero_agrees (a : Word) : Evm.iszero a = Reference.iszero a := by
  unfold Evm.iszero Reference.iszero
  by_cases h : a = 0 <;> simp [h]

theorem select_agrees (c a b : Word) : Evm.select c a b = Reference.select c a b := by
  unfold Evm.select Reference.select
  by_cases h : c = 0 <;> simp [h]

end EvmRules
