import SolarTV.MIR.State.IR
import SolarTV.State.MemoryLaws

/-! Direct allocation semantics, with the FMP supplied in explicit byte memory.
Reservation reads word 0x40 and writes the new end without touching the reserved
range. Zero initialization follows the FMP write and touches exactly that range.
The caller includes any alignment/padding in the requested size. Checked bumps
panic with 0x41 on wrapping or exceeding uint64; an invalid infallible bump is
outside its producer's validity contract and remains unsupported. No placement,
heap floor, synthetic pointer or implicit entry initialization is inferred. -/

namespace SolarTV.MIR.State
open EvmRules SolarTV.Execution

def freePointer (frame : Frame) : Step := do
  let memory ← expand frame.memory 64 32
  return .returned (some (readWord memory 64),{frame with memory})

def allocate (frame : Frame) (size : Word) (zeroed checked : Bool) : Step := do
  let memory ← expand frame.memory 64 32
  let pointer := readWord memory 64
  let next := pointer + size
  if next.toNat < pointer.toNat || next.toNat > 2^64 - 1 then
    if checked then return .reverted (.panic 0x41)
    else .error "infallible allocation validity precondition"
  else
    let memory := writeBytes memory 64 (wordBytes next)
    let memory ← if zeroed then do
      let memory ← expand memory pointer.toNat size.toNat
      pure (writeBytes memory pointer.toNat (List.replicate size.toNat 0))
      else pure memory
    return .returned (some pointer,{frame with memory})

end SolarTV.MIR.State
