import SolarTV.MIR.State.Layout

/-! Direct link-boundary semantics. Known entries must agree with the supplied
image after their library patches. Deferred bytes and neighboring image bytes
are explicit inputs, not products of lowering. Empty copies observe no data or
relocation. No source-to-image correspondence theorem is claimed. -/

namespace SolarTV.MIR.State
open EvmRules SolarTV.Execution

def patchData (cfg : Config) (bytes : List Byte) (relocations : List (Nat × Nat)) :
    Except String (List Byte) :=
  relocations.foldlM (fun bytes (offset,id) => do
    let address ← required (layoutBinding cfg.layout.libraries id) "missing library relocation"
    if address.toNat ≥ 2^160 then .error "invalid library relocation" else
    if offset + 20 > bytes.length then .error "invalid data relocation" else
    pure (writeBytes bytes offset ((wordBytes address).drop 12))) bytes

def copyData (cfg : Config) (frame : Frame) (id : Nat) (raw : Option (List Byte))
    (relocations : List (Nat × Nat)) (offset : Nat) (destination count : Word) : Step := do
  if count == 0 then return .returned (none,frame)
  let memory ← expand frame.memory destination.toNat count.toNat
  let known ← raw.mapM (fun bytes => patchData cfg bytes relocations)
  let bytes ← match layoutBinding cfg.layout.data id with
    | some (base,size) => do
      let code ← required cfg.layout.code "missing explicit MIR code image"
      if base+size > code.length then .error "linked data outside code image" else
      if known.any (fun bytes => bytes.length != size || bytes != readBytes code base size) then
        .error "linked data disagrees with declaration" else
      pure (readBytes code (base+offset) count.toNat)
    | none => do
      let bytes ← required known "missing linked data extent"
      if offset+count.toNat > bytes.length then .error "datacopy past its data" else
      pure (readBytes bytes offset count.toNat)
  return .returned (none,{frame with memory := writeBytes memory destination.toNat bytes})

end SolarTV.MIR.State
