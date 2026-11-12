import SolarTV.MIR.State.Slots

/-! Solidity byte-string storage operations. Short headers embed up to 31 bytes;
long headers encode twice the length plus one and address data by Keccak(slot).
Header validation precedes allocation or mutation. Reads retain full-word memory
traffic, writes mask dirty padding, and shrinking clears old tail words. The
scratch hash, FMP reservation, static violations and exact panic payloads are
observable. Loops exceeding 2048 storage words are unsupported, not truncated. -/

namespace SolarTV.MIR.State
open EvmRules SolarTV.Execution

def bytesHeader (header : Word) : Except RevertData (Bool × Nat) :=
  let long := header.toNat % 2 == 1
  let length := if long then header.toNat / 2 else header.toNat / 2 % 128
  if long == (length < 32) then .error (.panic 0x22) else .ok (long,length)

def storageRead (cfg : Config) (frame : Frame) (slot : Word) : Word × Frame :=
  (slotsRead (account frame.world cfg.environment.address).storage slot,
   {frame with world := warmSlot frame.world cfg.environment.address slot})

def storageWrite (cfg : Config) (frame : Frame) (slot word : Word) : Except String Frame :=
  let old := account frame.world cfg.environment.address
  let world := putAccount (warmSlot frame.world cfg.environment.address slot) {old with storage := slotsWrite old.storage slot word}
  .ok {frame with world}

/-- Even an empty clear retains its slot-hashing scratch write. -/
def clearStorageWords (cfg : Config) (frame : Frame) (slot first last : Word) : Step := do
  let (base,frame) ← fixedSlot frame [slot]
  let count := last.toNat - first.toNat
  if count > 2048 then .error "MIR storage work limit of 2048 words" else
  if count > 0 && cfg.environment.isStatic then return .invalid else
  let mut frame := frame
  for i in List.range count do
    frame ← storageWrite cfg frame (base + first + BitVec.ofNat 256 i) 0
  return .returned (none,frame)

def loadStorageBytes (cfg : Config) (frame : Frame) (slot : Word) : Step := do
  let (header,frame) := storageRead cfg frame slot
  match bytesHeader header with
  | .error data => return .reverted data
  | .ok (long,length) =>
    if length + 63 ≥ 2^256 then return .reverted (.panic 0x41) else
    let size := (length + 31) / 32 * 32 + 32
    match ← allocate frame (BitVec.ofNat 256 size) false true with
    | .reverted data => return .reverted data
    | .invalid => return .invalid
    | .returned (some pointer,frame) =>
      let memory ← expand frame.memory pointer.toNat 32
      let memory := writeBytes memory pointer.toNat (wordBytes (BitVec.ofNat 256 length))
      let mut frame := {frame with memory}
      if long then
        let (base,next) ← fixedSlot frame [slot]
        frame := next
        let count := (length + 31) / 32
        if count > 2048 then .error "MIR storage work limit of 2048 words" else
        for i in List.range count do
          let (word,next) := storageRead cfg frame (base + BitVec.ofNat 256 i)
          let destination := pointer + BitVec.ofNat 256 (32 + 32*i)
          let memory ← expand next.memory destination.toNat 32
          frame := {next with memory := writeBytes memory destination.toNat (wordBytes word)}
        return .returned (some pointer,frame)
      else
        let destination := pointer + 32
        let memory ← expand frame.memory destination.toNat 32
        let memory := writeBytes memory destination.toNat (wordBytes (header &&& (~~~(255 : Word))))
        return .returned (some pointer,{frame with memory})
    | _ => .error "MIR allocation result missing"

/-- Literal bytes have no memory input; object words are read after scratch traffic. -/
def storeStorageBytes (cfg : Config) (frame : Frame) (slot : Word)
    (object : Word) (literal : Option (List Byte)) : Step := do
  let (header,frame) := storageRead cfg frame slot
  match bytesHeader header with
  | .error data => return .reverted data
  | .ok (oldLong,oldLength) =>
    let (length,frame) ← match literal with
      | some bytes => pure (bytes.length,frame)
      | none => do
        let memory ← expand frame.memory object.toNat 32
        pure ((readWord memory object.toNat).toNat,{frame with memory})
    if length + 31 ≥ 2^256 then return .reverted (.panic 0x11) else
    let mut frame := frame
    let count := if length < 32 then 0 else (length + 31) / 32
    if oldLong && oldLength > length then
      match ← clearStorageWords cfg frame slot (BitVec.ofNat 256 count) (BitVec.ofNat 256 ((oldLength+31)/32)) with
      | .returned (_,next) => frame := next
      | .invalid => return .invalid
      | .reverted data => return .reverted data
    if length < 32 then
      let (word,next) ← match literal with
        | some bytes => pure (readWord bytes 0,frame)
        | none => do
          let address := object + 32
          let memory ← expand frame.memory address.toNat 32
          pure (readWord memory address.toNat,{frame with memory})
      if cfg.environment.isStatic then return .invalid else
      let mask : Word := if length = 0 then 0 else ~~~(BitVec.ofNat 256 (2^(8*(32-length))-1))
      let next ← storageWrite cfg next slot ((word &&& mask) ||| BitVec.ofNat 256 (2*length))
      return .returned (none,next)
    else
      if cfg.environment.isStatic then return .invalid else
      frame ← storageWrite cfg frame slot (BitVec.ofNat 256 (2*length+1))
      let (base,next) ← fixedSlot frame [slot]
      frame := next
      if count > 2048 then .error "MIR storage work limit of 2048 words" else
      for i in List.range count do
        let (word,next) ← match literal with
          | some bytes => pure (readWord bytes (32*i),frame)
          | none => do
            let address := object + BitVec.ofNat 256 (32+32*i)
            let memory ← expand frame.memory address.toNat 32
            pure (readWord memory address.toNat,{frame with memory})
        let keep := min 32 (length - 32*i)
        let mask : Word := ~~~(BitVec.ofNat 256 (2^(8*(32-keep))-1))
        frame ← storageWrite cfg next (base + BitVec.ofNat 256 i) (word &&& mask)
      return .returned (none,frame)

end SolarTV.MIR.State
