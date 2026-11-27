import SolarTV.EVMIR.Transforms.ReturnBuffer
import SolarTV.State.MemoryLaws

/-! The scratch encoding used by the emitted FlashLoan callback and its
lowered MIR. The cursor is shared across the selector, static words, length,
zeroing store, and calldata copy. Expansion is charged by each caller. -/

namespace SolarTV.MIR.FlashCallback
open EvmRules SolarTV.Execution

def padded (length : Nat) : Nat := 32 * EVM.State.ReturnBuffer.words length

theorem padding (length : Nat) : length ≤ padded length ∧ padded length - length < 32 := by
  unfold padded EVM.State.ReturnBuffer.words
  omega

def selector : Word := 0x31f5707200000000000000000000000000000000000000000000000000000000

def head (memory : List Byte) (base : Nat) (amount : Word) : List Byte :=
  let memory := writeBytes memory base (wordBytes selector)
  let memory := writeBytes memory (base+4) (wordBytes amount)
  writeBytes memory (base+36) (wordBytes 64)

def staged (memory : List Byte) (base : Nat) (amount : Word) (data : List Byte) : List Byte :=
  let memory := head memory base amount
  let memory := writeBytes memory (base+68+padded data.length) (wordBytes 0)
  let memory := writeBytes memory (base+68) (wordBytes (BitVec.ofNat 256 data.length))
  writeBytes memory (base+100) data

def payload (amount : Word) (data : List Byte) : List Byte :=
  (wordBytes selector).take 4 ++ wordBytes amount ++ wordBytes 64 ++
    wordBytes (BitVec.ofNat 256 data.length) ++ data ++ List.replicate (padded data.length-data.length) 0

theorem headBytes (memory : List Byte) (base : Nat) (amount : Word)
    (covered : base+68 ≤ memory.length) :
    readBytes (head memory base amount) base 68 =
      (wordBytes selector).take 4 ++ wordBytes amount ++ wordBytes 64 := by
  let first := writeBytes memory base (wordBytes selector)
  let second := writeBytes first (base+4) (wordBytes amount)
  have short : readBytes first base 4 = (wordBytes selector).take 4 := by
    have whole := readBytes_writeBytes_same memory (wordBytes selector) base (by simp <;> omega)
    simp only [wordBytes_length] at whole
    rw [← readBytes_take _ _ 32 _ (by decide), whole]
  have value : readBytes second (base+4) 32 = wordBytes amount := by
    have whole := readBytes_writeBytes_same first (wordBytes amount) (base+4) (by simp [first]; omega)
    simpa only [wordBytes_length] using whole
  have offset : readBytes (head memory base amount) (base+36) 32 = wordBytes 64 := by
    have whole := readBytes_writeBytes_same second (wordBytes 64) (base+36) (by simp [second, first]; omega)
    simpa only [head, second, first, wordBytes_length] using whole
  rw [show 68 = 4+(32+32) by decide, readBytes_split, readBytes_split _ (base+4)]
  change readBytes (writeBytes second (base+36) (wordBytes 64)) base 4 ++
    (readBytes (writeBytes second (base+36) (wordBytes 64)) (base+4) 32 ++
      readBytes (head memory base amount) (base+36) 32) = _
  rw [readBytes_writeBytes_disjoint second (wordBytes 64) (base+36) base 4 (Or.inr (by omega)),
    readBytes_writeBytes_disjoint second (wordBytes 64) (base+36) (base+4) 32 (Or.inr (by omega)),
    offset, value]
  unfold second
  rw [readBytes_writeBytes_disjoint first (wordBytes amount) (base+4) base 4 (Or.inr (by omega)), short]
  simp only [List.append_assoc]

private theorem readsZero (size offset count : Nat) :
    readBytes (List.replicate size (0 : Byte)) offset count = List.replicate count 0 := by
  apply List.ext_getElem?
  intro index
  by_cases within : index < count
  · simp [readBytes, List.getElem?_replicate, within]
    split <;> rfl
  · simp [readBytes, List.getElem?_replicate, within]

/-- The lowered stores and calldata copy produce the ABI message, including
the partial word's padding even when the original arena contains dirty bytes. -/
theorem stagedBytes (memory data : List Byte) (base : Nat) (amount : Word)
    (covered : base+100+padded data.length ≤ memory.length) :
    readBytes (staged memory base amount data) base (100+padded data.length) =
      payload amount data := by
  let hd := head memory base amount
  let clear := writeBytes hd (base+68+padded data.length) (wordBytes 0)
  let count := writeBytes clear (base+68) (wordBytes (BitVec.ofNat 256 data.length))
  let result := writeBytes count (base+100) data
  have enough := (padding data.length).1
  have padShort := (padding data.length).2
  have headLength : hd.length = memory.length := by simp [hd, head]
  have clearLength : clear.length = memory.length := by simp [clear, headLength]
  have countLength : count.length = memory.length := by simp [count, clearLength]
  have header : readBytes result base 68 =
      (wordBytes selector).take 4 ++ wordBytes amount ++ wordBytes 64 := by
    unfold result count clear
    rw [readBytes_writeBytes_disjoint _ _ _ _ _ (Or.inr (by omega)),
      readBytes_writeBytes_disjoint _ _ _ _ _ (Or.inr (by omega)),
      readBytes_writeBytes_disjoint _ _ _ _ _ (Or.inr (by omega))]
    exact headBytes memory base amount (by omega)
  have lengthWord : readBytes result (base+68) 32 = wordBytes (BitVec.ofNat 256 data.length) := by
    unfold result
    rw [readBytes_writeBytes_disjoint _ _ _ _ _ (Or.inr (by omega))]
    unfold count
    have whole := readBytes_writeBytes_same clear (wordBytes (BitVec.ofNat 256 data.length)) (base+68)
      (by simp only [wordBytes_length, clearLength]; omega)
    simpa only [wordBytes_length] using whole
  have dynamic : readBytes result (base+100) data.length = data := by
    unfold result
    exact readBytes_writeBytes_same _ _ _ (by rw [countLength]; omega)
  have zeros : readBytes result (base+100+data.length) (padded data.length-data.length) =
      List.replicate (padded data.length-data.length) 0 := by
    unfold result count clear
    rw [readBytes_writeBytes_disjoint _ _ _ _ _ (Or.inl (by omega)),
      readBytes_writeBytes_disjoint _ _ _ _ _ (Or.inl (by simp <;> omega)),
      readBytes_writeBytes_window]
    · rw [show wordBytes 0 = List.replicate 32 (0 : Byte) by decide +kernel]
      exact readsZero _ _ _
    · omega
    · simp <;> omega
    · rw [headLength]
      omega
  change readBytes result base (100+padded data.length) = _
  rw [show 100+padded data.length = 68+(32+(data.length+(padded data.length-data.length))) by omega,
    readBytes_split, readBytes_split result (base+68), readBytes_split result (base+68+32)]
  change readBytes result base 68 ++ (readBytes result (base+68) 32 ++
    (readBytes result (base+100) data.length ++
      readBytes result (base+100+data.length) (padded data.length-data.length))) = _
  rw [header, lengthWord, dynamic, zeros]
  simp only [payload, List.append_assoc]

end SolarTV.MIR.FlashCallback
