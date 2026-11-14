import SolarTV.MIR.State.Allocation

/-! Convention-defined frame slots, shared with ordinary byte memory.
External scratch slots have base 128; the multi-return word is the pointer slot
at 32. Internal frame addresses use explicit per-activation placements. The MIR
verifier restricts multi-return slots to one word at offset zero. Slices read and write pointer before length, retaining all
overlaps and memory expansion. No implicit per-activation memory is introduced. -/

namespace SolarTV.MIR.State
open EvmRules SolarTV.Execution

def loadFrame (frame : Frame) (address : Word) (ty : ValueType) : Except String (Value × Frame) := do
  let memory ← expand frame.memory address.toNat 32
  let first := readWord memory address.toNat
  match ty with
  | .word 256 => return (.scalar (Scalar.ofWord 256 first),{frame with memory})
  | .slice _ =>
    let next := address + 32
    let memory ← expand memory next.toNat 32
    return (.aggregate [.scalar (Scalar.ofWord 256 first),
      .scalar (Scalar.ofWord 256 (readWord memory next.toNat))],{frame with memory})
  | _ => .error "invalid MIR frame slot type"

def storeFrame (frame : Frame) (address : Word) (ty : ValueType) (value : Value) : Except String Frame := do
  let words ← match ty,value with
    | .word 256,.scalar word => pure [word.word]
    | .slice _,.aggregate [pointer,length] => pure [← pointer.word,← length.word]
    | _,_ => .error "undefined or invalid MIR frame slot value"
  let mut memory := frame.memory
  for (word,index) in words.zipIdx do
    let destination := address + BitVec.ofNat 256 (32*index)
    memory ← expand memory destination.toNat 32
    memory := writeBytes memory destination.toNat (wordBytes word)
  return {frame with memory}

end SolarTV.MIR.State
