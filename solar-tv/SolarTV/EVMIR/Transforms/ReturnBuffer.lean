import SolarTV.EVMIR.State.Types
import SolarTV.State.MemoryLaws

/-! Memory after a helper appends returndata to an already active arena.
The continuation charges the header store and payload copy separately. -/

namespace SolarTV.EVM.State.ReturnBuffer
open EvmRules SolarTV.Execution

def words (length : Nat) : Nat := (length+31)/32

def cursorWord (cursor length : Nat) : Word :=
  BitVec.ofNat 256 (cursor+32+32*words length)

def bumped (memory : List Byte) (cursor length : Nat) : List Byte :=
  writeBytes memory 64 (wordBytes (cursorWord cursor length))

def header (memory : List Byte) (cursor length : Nat) : List Byte :=
  writeBytes (bumped memory cursor length ++ List.replicate 32 0) cursor
    (wordBytes (BitVec.ofNat 256 length))

def active (memory : List Byte) (cursor length : Nat) : List Byte :=
  header memory cursor length ++ List.replicate (32*words length) 0

def copied (memory buffer : List Byte) (cursor : Nat) : List Byte :=
  writeBytes (active memory cursor buffer.length) (cursor+32) buffer

@[simp] theorem bumped_length (memory : List Byte) (cursor length : Nat) :
    (bumped memory cursor length).length = memory.length := by simp [bumped]

@[simp] theorem header_length (memory : List Byte) (cursor length : Nat) :
    (header memory cursor length).length = memory.length+32 := by simp [header]

@[simp] theorem active_length (memory : List Byte) (cursor length : Nat) :
    (active memory cursor length).length = memory.length+32+32*words length := by simp [active]

@[simp] theorem copied_length (memory buffer : List Byte) (cursor : Nat) :
    (copied memory buffer cursor).length = memory.length+32+32*words buffer.length := by
  simp [copied]

theorem required (memory : List Byte) (count : Nat) (aligned : memory.length % 32 = 0) :
    requiredWords memory memory.length count = memoryWords memory+words count := by
  simp only [requiredWords, memoryWords, words]
  split <;> omega

theorem expand_end (memory : List Byte) (count : Nat)
    (aligned : memory.length % 32 = 0) (fits : memory.length+32*words count ≤ 65536) :
    expand memory memory.length count = .ok (memory ++ List.replicate (32*words count) 0) := by
  have size : 32*(memoryWords memory+words count) = memory.length+32*words count := by
    unfold memoryWords
    omega
  simp [expand, required memory count aligned, size, Nat.not_lt.mpr fits]

theorem expand_at_cursor (memory : List Byte) (cursor count : Nat)
    (atEnd : memory.length = cursor) (aligned : cursor % 32 = 0)
    (fits : cursor+32*words count ≤ 65536) :
    expand memory cursor count = .ok (memory ++ List.replicate (32*words count) 0) := by
  subst cursor
  exact expand_end memory count aligned fits

theorem charge_at_cursor (memory : List Byte) (cursor count : Nat)
    (atEnd : memory.length = cursor) (aligned : cursor % 32 = 0) :
    memoryCharge memory cursor count =
      memoryCost (cursor/32+words count) - memoryCost (cursor/32) := by
  subst cursor
  rw [memoryCharge, required memory count aligned]
  rfl

theorem expand_covered (memory : List Byte) (offset count : Nat)
    (aligned : memory.length % 32 = 0) (limit : memory.length ≤ 65536)
    (covered : offset+count ≤ memory.length) : expand memory offset count = .ok memory := by
  have wordsEq : requiredWords memory offset count = memory.length/32 := by
    simp only [requiredWords, memoryWords]
    split <;> omega
  simp [expand, wordsEq, show 32*(memory.length/32) = memory.length by omega, Nat.not_lt.mpr limit]

theorem charge_covered (memory : List Byte) (offset count : Nat)
    (aligned : memory.length % 32 = 0) (covered : offset+count ≤ memory.length) :
    memoryCharge memory offset count = 0 := by
  have wordsEq : requiredWords memory offset count = memory.length/32 := by
    simp only [requiredWords, memoryWords]
    split <;> omega
  simp [memoryCharge, memoryWords, wordsEq]

theorem read_header (memory buffer : List Byte) (cursor : Nat)
    (atEnd : memory.length = cursor) :
    readWord (copied memory buffer cursor) cursor = BitVec.ofNat 256 buffer.length := by
  unfold copied
  rw [readWord_writeBytes_disjoint _ _ _ _ (Or.inr (by omega))]
  unfold active
  rw [readWord_append_prefix _ _ _ (by simp [atEnd])]
  unfold header
  exact readWord_writeWord _ _ _ (by simp [atEnd])

theorem read_payload_word (memory buffer : List Byte) (cursor : Nat)
    (atEnd : memory.length = cursor) (full : 32 ≤ buffer.length) :
    readWord (copied memory buffer cursor) (cursor+32) = readWord buffer 0 := by
  have whole : readBytes (copied memory buffer cursor) (cursor+32) buffer.length = buffer := by
    unfold copied
    exact readBytes_writeBytes_same _ _ _ (by simp [atEnd, words]; omega)
  unfold readWord
  rw [← readBytes_take _ _ _ _ full, whole]
  rw [← readBytes_take buffer 0 buffer.length 32 full, readBytes_entire]

end SolarTV.EVM.State.ReturnBuffer
