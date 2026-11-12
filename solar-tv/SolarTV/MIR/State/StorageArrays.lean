import SolarTV.MIR.State.StorageBytes

/-! Dynamic storage-array reads, including packed scalars and nested byte strings.
The parent allocation and length store precede scratch hashing, even for an empty
array. Packed words are read once per group; the final group is bounded by length.
Enum failures retain panic 0x21 and oversized allocations panic 0x41. Child byte
objects share the parent's FMP and scratch memory. Work above 2048 elements is
unsupported, not silently truncated. -/

namespace SolarTV.MIR.State
open EvmRules SolarTV.Execution

def arrayBits : ArrayElement → Nat
  | .uint bits | .sint bits | .fixedBytes bits => bits
  | .bytes => 256

def arrayScalar (element : ArrayElement) (word : Word) (index : Nat) : Word :=
  let bits := arrayBits element
  let field := narrow bits (word >>> (index * bits))
  match element with
  | .sint _ => (field.setWidth bits).signExtend 256
  | .fixedBytes _ => field <<< (256-bits)
  | _ => field

def loadStorageArray (cfg : Config) (frame : Frame) (slot : Word)
    (element : ArrayElement) (variants : Option Nat) : Step := do
  let bits := arrayBits element
  if bits = 0 || bits > 256 || bits % 8 != 0 then
    .error "invalid storage array element width" else
  let (length,frame) := storageRead cfg frame slot
  if length.toNat > (2^64-1)/32 then return .reverted (.panic 0x41) else
  match ← allocate frame (BitVec.ofNat 256 ((length.toNat+1)*32)) false true with
  | .returned (some pointer,frame) =>
    let memory ← expand frame.memory pointer.toNat 32
    let frame := {frame with memory := writeBytes memory pointer.toNat (wordBytes length)}
    let (base,frame) ← fixedSlot frame [slot]
    if length.toNat > 2048 then .error "MIR storage array work limit of 2048 elements" else
    let mut frame := frame
    let mut packed : Word := 0
    for index in List.range length.toNat do
      let value : Except (Outcome (Option Word × Frame)) Word ← match element with
        | .bytes => do
          match ← loadStorageBytes cfg frame (base + BitVec.ofNat 256 index) with
          | .returned (some value,next) => frame := next; pure (.ok value)
          | .reverted data => pure (.error (.reverted data))
          | .invalid => pure (.error .invalid)
          | _ => .error "MIR byte object result missing"
        | _ => do
          let perSlot := 256 / bits
          if index % perSlot = 0 then
            let (word,next) := storageRead cfg frame (base + BitVec.ofNat 256 (index/perSlot))
            packed := word
            frame := next
          pure (.ok (arrayScalar element packed (index % perSlot)))
      match value with
      | .error outcome => return outcome
      | .ok value =>
        if variants.isSome && value.toNat ≥ variants.getD 0 then
          return .reverted (.panic 0x21) else
        let destination := pointer + BitVec.ofNat 256 (32+32*index)
        let memory ← expand frame.memory destination.toNat 32
        frame := {frame with memory := writeBytes memory destination.toNat (wordBytes value)}
    return .returned (some pointer,frame)
  | .reverted data => return .reverted data
  | .invalid => return .invalid
  | _ => .error "MIR array allocation result missing"

end SolarTV.MIR.State
