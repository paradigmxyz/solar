import SolarTV.MIR.Transforms.TransferFromMarket

/-! The word-width relationship at Morpho's collateral update. The capture
checks the concrete instruction stream while these lemmas keep the packed
storage word and the arithmetic inputs symbolic. -/

namespace SolarTV.MIR.TransferFromCollateral
open EvmRules SolarTV.Execution MIR.State
open MIR.TransferFromMarket

@[simp] theorem warmContains (journal : Journal) (owner : Address) (slot : Word) :
    (owner,slot) ∈ (warmSlot journal owner slot).warmSlots := by
  by_cases warm : (owner,slot) ∈ journal.warmSlots <;> simp [warmSlot,warm]

theorem warmAgain (journal : Journal) (owner : Address) (slot : Word) :
    warmSlot (warmSlot journal owner slot) owner slot = warmSlot journal owner slot := by
  by_cases warm : (owner,slot) ∈ journal.warmSlots <;> simp [warmSlot,warm]

@[simp] theorem warmAccount (journal : Journal) (owner address : Address) (slot : Word) :
    account (warmSlot journal owner slot) address = account journal address := rfl

@[simp] theorem warmOriginal (journal : Journal) (owner : Address) (slot : Word) :
    (warmSlot journal owner slot).original = journal.original := rfl

theorem scalarFits (width : Nat) (word : Word) (fits : word.toNat < 2^width) :
    (Scalar.ofWord width word).word = word := by
  by_cases full : width = 256
  · subst width
    rfl
  · apply BitVec.eq_of_toNat_eq
    have within := word.isLt
    simp [Scalar.ofWord,Scalar.word,wordBits,widenBits,full,Nat.mod_eq_of_lt fits,
      Nat.mod_eq_of_lt within]

theorem highFits (word : Word) : (Evm.shr 128 word).toNat < 2^128 :=
  BitVec.toNat_ushiftRight_lt word 128 (by decide)

theorem checkedAdd128 (cfg : Config) (frame : Frame) (a b : Word)
    (smallA : a.toNat < 2^128) (smallB : b.toNat < 2^128)
    (fits : (a+b).toNat < 2^128) :
    operation cfg frame (.checked .add 128 false) [a,b] = value frame (a+b) := by
  have noWrap : (a+b).toNat = a.toNat + b.toNat := by
    rw [BitVec.toNat_add, Nat.mod_eq_of_lt (by omega)]
  have narrowA : (wordBits 128 a).toNat = a.toNat := by
    simp [wordBits,Nat.mod_eq_of_lt smallA]
  have narrowB : (wordBits 128 b).toNat = b.toNat := by
    simp [wordBits,Nat.mod_eq_of_lt smallB]
  have within : a.toNat + b.toNat < 2^128 := by omega
  have safe : (wordBits 128 a).uaddOverflow (wordBits 128 b) = false := by
    simp [BitVec.uaddOverflow,narrowA,narrowB,Nat.not_le_of_lt within]
  have result :
      (Scalar.ofWord 128 (a+b)).word = widenBits (wordBits 128 a + wordBits 128 b) := by
    change widenBits (wordBits 128 (a+b)) = _
    congr 1
    exact BitVec.setWidth_add a b (by decide)
  simp only [operation,MIR.checked,safe,Bool.false_eq_true,ite_false]
  change value frame (widenBits (wordBits 128 a + wordBits 128 b)) = value frame (a+b)
  rw [← result, scalarFits _ _ fits]

theorem decodedSegment {cfg : Config} {functions : Functions} {entry steps : Nat}
    {journal : Journal} {market : List Word} {arguments : List Value}
    {decoded : Except String Frame} {result : Except String (Unit ⊕ Machine)}
    (frame : decoded = .ok (mirStart entry journal market arguments).frame)
    (checked : mirSteps cfg functions entry journal market arguments steps = result) :
    (do
      let frame ← decoded
      mirStepsFrom cfg functions ⟨{function := entry},[],frame⟩ steps) = result := by
  rw [frame]
  exact checked

end SolarTV.MIR.TransferFromCollateral
