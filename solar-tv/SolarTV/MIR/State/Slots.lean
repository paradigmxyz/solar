import SolarTV.MIR.State.Allocation
import SolarTV.Keccak

/-! Solidity slot derivation with its observable memory footprint. Fixed-width
keys and array parents occupy reserved scratch words. Byte keys are copied to
unreserved free memory, followed by the parent word, without advancing FMP.
Reads precede overlapping writes just as for MCOPY. Address arithmetic wraps;
active-memory limits are unsupported executions, never successful hashes.
These are direct semantic operations, not calls to their compiler lowering. -/

namespace SolarTV.MIR.State
open EvmRules SolarTV.Execution

/-- Hash fixed-width big-endian words, preserving the scratch writes and MSIZE. -/
def fixedSlot (frame : Frame) (words : List Word) : Except String (Word × Frame) := do
  let bytes := words.flatMap wordBytes
  let memory ← expand frame.memory 0 bytes.length
  return (Keccak.hash bytes,{frame with memory := writeBytes memory 0 bytes})

/-- A memory object reads its header; a calldata slice supplies a logical range. -/
def bytesSlot (cfg : Config) (frame : Frame) (key : Value) (slot : Word) : Except String (Word × Frame) := do
  let (source,length,calldata,frame) ← match key with
    | .scalar object => do
      let memory ← expand frame.memory object.word.toNat 32
      let length := readWord memory object.word.toNat
      pure (object.word + 32,length,false,{frame with memory})
    | .aggregate [pointer,length] => do
      pure (← pointer.word,← length.word,true,frame)
    | _ => .error "invalid mapping bytes key"
  let memory ← expand frame.memory 64 32
  let scratch := readWord memory 64
  let memory ← expand memory scratch.toNat length.toNat
  let memory ← if calldata then pure memory else expand memory source.toNat length.toNat
  let input := if calldata then cfg.environment.input else memory
  let bytes := readBytes input source.toNat length.toNat
  let memory := writeBytes memory scratch.toNat bytes
  let address := scratch + length
  let memory ← expand memory address.toNat 32
  let memory := writeBytes memory address.toNat (wordBytes slot)
  let count := length + 32
  let memory ← expand memory scratch.toNat count.toNat
  return (Keccak.hash (readBytes memory scratch.toNat count.toNat),{frame with memory})

/-- ERC-7201 hashes the byte object, then hashes the decremented digest in scratch. -/
def namespaceSlot (frame : Frame) (object : Word) : Except String (Word × Frame) := do
  let memory ← expand frame.memory object.toNat 32
  let length := readWord memory object.toNat
  let source := object + 32
  let memory ← expand memory source.toNat length.toNat
  let inner := Keccak.hash (readBytes memory source.toNat length.toNat) - 1
  let (outer,next) ← fixedSlot {frame with memory} [inner]
  return (outer &&& ~~~(255 : Word),next)

end SolarTV.MIR.State
