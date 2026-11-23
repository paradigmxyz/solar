import SolarTV.State.ByteMemory

/-! Constructor code reads use the actual assembled offsets. A declared
data read is wholly within its byte string. A read at the end of the sole
data entry uses the constructor arguments appended at invocation. -/

namespace SolarTV.EVM.CodeRelocations
open EvmRules Execution

theorem read_after_prefix (head tail : List Byte) (offset count : Nat) :
    readBytes (head ++ tail) (head.length + offset) count = readBytes tail offset count := by
  simp only [readBytes]
  apply List.map_congr_left
  intro i _
  rw [List.getElem?_append_right (by omega : head.length ≤ head.length + offset + i)]
  congr 2
  omega

theorem declared_read (head data suffix : List Byte) (offset count : Nat)
    (inside : offset + count ≤ data.length) :
    readBytes (head ++ data ++ suffix) (head.length + offset) count =
      readBytes data offset count := by
  rw [List.append_assoc, read_after_prefix]
  simp only [readBytes]
  apply List.map_congr_left
  intro i hi
  have within := List.mem_range.mp hi
  rw [List.getElem?_append_left (by omega : offset + i < data.length)]

theorem argument_read (image args : List Byte) :
    readBytes (image ++ args) image.length args.length = readBytes args 0 args.length := by
  simpa using read_after_prefix image args 0 args.length

theorem argument_length (image args : List Byte) :
    (BitVec.ofNat 256 (image ++ args).length : Word) - BitVec.ofNat 256 image.length =
      BitVec.ofNat 256 args.length := by
  rw [List.length_append, BitVec.ofNat_add, BitVec.add_comm, BitVec.add_sub_cancel]

end SolarTV.EVM.CodeRelocations
