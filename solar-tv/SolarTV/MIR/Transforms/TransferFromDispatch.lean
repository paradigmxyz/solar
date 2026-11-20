import SolarTV.MIR.Transforms.TransferFromEntry
import SolarTV.State.MemoryLaws

/-! The emitted selector dispatcher is independent of calldata following
the selector. The generated certificate imports the actual opcode, jump
and gas at each routed PC. This theorem connects the word read by the
runtime to the four-byte ABI field. -/

namespace SolarTV.MIR.TransferFromDispatch
open EvmRules SolarTV.Execution

def selector (input : List Byte) : Word :=
  (BitVec.flattenListFast (readBytes input 0 4)).setWidth 256

theorem selector_word (input : List Byte) :
    readWord input 0 >>> 224 = selector input := by
  apply BitVec.eq_of_getLsbD_eq
  intro i hi
  simp only [readWord,selector,BitVec.getLsbD_ushiftRight,BitVec.getLsbD_setWidth,
    ← BitVec.flattenList_eq_flattenListFast,BitVec.getLsbD_flattenList]
  simp only [readBytes,List.length_map,List.length_range,Nat.zero_add]
  have same : (224+i) % 8 = i % 8 := by omega
  have index : 31 - (224+i) / 8 = 3 - i / 8 := by omega
  by_cases inside : i < 32
  · have range : 224+i < 256 := by omega
    have word : 3 - i/8 < 4 := by omega
    have full : 3 - i/8 < 32 := by omega
    simp [range,hi,inside,same,index,word,full]
  · have range : ¬ 224+i < 256 := by omega
    simp [range,hi,inside]

theorem selector_append (head tail : List Byte) (width : head.length = 4) :
    selector (head ++ tail) = selector head := by
  have read : readBytes (head ++ tail) 0 4 = readBytes head 0 4 := by
    unfold readBytes
    apply List.map_congr_left
    intro i hi
    have bound : i < head.length := width ▸ List.mem_range.mp hi
    simp [List.getElem?_append_left bound]
  apply BitVec.eq_of_getLsbD_eq
  intro i _
  simp only [selector,BitVec.getLsbD_setWidth, ← BitVec.flattenList_eq_flattenListFast,
    BitVec.getLsbD_flattenList,read]

end SolarTV.MIR.TransferFromDispatch
