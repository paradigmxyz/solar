import SolarTV.State.ByteMemory

/-! Kernel-proved byte-memory normalization, independent of any compiler pass.
Read-over-write rules require actual overlap/active-range proofs; they never
assume that two pointers are distinct. Pre-simplification avoids expanding an
entire symbolic heap merely to read an untouched allocation-cursor word. -/

namespace SolarTV.Execution
open EvmRules

@[simp] theorem wordBytes_length (value : Word) : (wordBytes value).length = 32 := by
  simp [wordBytes]

@[simp] theorem writeBytes_length (memory bytes : List Byte) (offset : Nat) :
    (writeBytes memory offset bytes).length = memory.length := by
  simp [writeBytes]

theorem readBytes_writeBytes_disjoint (memory bytes : List Byte) (destination source count : Nat)
    (h : destination + bytes.length ≤ source ∨ source + count ≤ destination) :
    readBytes (writeBytes memory destination bytes) source count = readBytes memory source count := by
  unfold readBytes
  apply List.map_congr_left
  intro i hi
  have hi' : i < count := List.mem_range.mp hi
  have outside : ¬ (destination ≤ source + i ∧ source + i < destination + bytes.length) := by omega
  simp [writeBytes, List.getElem?_mapIdx, outside]

theorem readBytes_writeBytes_same (memory bytes : List Byte) (offset : Nat)
    (h : offset + bytes.length ≤ memory.length) :
    readBytes (writeBytes memory offset bytes) offset bytes.length = bytes := by
  apply List.ext_getElem
  · simp [readBytes]
  · intro i hi hj
    have hm : offset + i < memory.length := by omega
    simp [readBytes, writeBytes, List.getElem?_mapIdx, List.getElem?_eq_getElem hm,
      Nat.add_sub_cancel_left, List.getElem?_eq_getElem hj] <;> omega

theorem flatten_wordBytes (word : Word) : (BitVec.flattenList (wordBytes word)).setWidth 256 = word := by
  have hl : (wordBytes word).length = 32 := by simp [wordBytes]
  apply BitVec.eq_of_getLsbD_eq
  intro i hi
  have hj : 31 - i / 8 < 32 := by omega
  have hm : i % 8 < 8 := Nat.mod_lt i (by decide)
  have hp : 8 * (31 - (31 - i / 8)) + i % 8 = i := by omega
  have hr : (List.range 32)[31 - i / 8]? = some (31 - i / 8) := by
    rw [List.getElem?_eq_getElem (by simpa using hj)]
    simp
  simp only [BitVec.getLsbD_setWidth, BitVec.getLsbD_flattenList, hl, hi, decide_true, Bool.true_and]
  simp only [wordBytes, List.getElem?_map, hr, Option.map_some, Option.getD_some]
  simp only [BitVec.getLsbD_setWidth, hm, decide_true, Bool.true_and, BitVec.getLsbD_ushiftRight, hp]

@[simp↓] theorem readWord_writeWord (memory : List Byte) (offset : Nat) (word : Word)
    (h : offset + 32 ≤ memory.length) :
    readWord (writeBytes memory offset (wordBytes word)) offset = word := by
  change (BitVec.flattenListFast (readBytes (writeBytes memory offset (wordBytes word)) offset (wordBytes word).length)).setWidth 256 = word
  rw [readBytes_writeBytes_same memory (wordBytes word) offset (by simpa using h)]
  rw [← BitVec.flattenList_eq_flattenListFast]
  exact flatten_wordBytes word

@[simp↓] theorem readWord_writeBytes_disjoint (memory bytes : List Byte) (destination source : Nat)
    (h : destination + bytes.length ≤ source ∨ source + 32 ≤ destination) :
    readWord (writeBytes memory destination bytes) source = readWord memory source := by
  unfold readWord
  rw [readBytes_writeBytes_disjoint memory bytes destination source 32 h]

@[simp↓] theorem readWord_append_prefix (memory bytes : List Byte) (offset : Nat)
    (h : offset + 32 ≤ memory.length) : readWord (memory ++ bytes) offset = readWord memory offset := by
  unfold readWord
  congr 2
  unfold readBytes
  apply List.map_congr_left
  intro i hi
  have hm : offset + i < memory.length := by have := List.mem_range.mp hi; omega
  simp [List.getElem?_append_left hm]

end SolarTV.Execution
