import SolarTV.MIR.State.Allocation
import SolarTV.Keccak

/-! Static packed ABI semantics with exact word-store footprints. Narrow scalar
groups may share one word with adjacent literals; otherwise literal chunks and
scalars retain their own unaligned word stores. Scratch selection uses the end
of every store, not just the hashed length. Padding and active memory stay live.
No compiler pass, backend image or exported store recipe supplies this model. -/

namespace SolarTV.MIR.State
open EvmRules SolarTV.Execution

structure PackedPiece where
  bytes : List Byte
  scalar : Bool

structure PackedStores where
  offset : Nat := 0
  maxEnd : Nat := 0
  writes : List (Nat × Word) := []

def packedPieces (parts : List PackedInput) (args : List Word) : Except String (List PackedPiece) := do
  if parts.length > 2048 then .error "packed part limit of 2048" else
  let mut pieces := []
  let mut total := 0
  for part in parts do
    let (bytes,scalar) ← match part with
      | .literal data => pure (data,false)
      | .scalar index size fixed => do
        if size = 0 || size > 32 then .error "packed scalar width" else
        let word ← required args[index]? "packed scalar operand"
        let data := wordBytes word
        pure ((if fixed then data.take size else data.drop (32-size)),true)
      | .bytes .. => .error "packed reference needs typed values"
    total := total+bytes.length
    if total > 65536 then .error "packed bytes exceed 64 KiB" else
      pieces := {bytes,scalar} :: pieces
  return pieces.reverse

def packedEmit (plan : PackedStores) (bytes : List Byte) : PackedStores :=
  let count := (bytes.length+31)/32
  let writes := (List.range count).map (fun index => (plan.offset+32*index,readWord bytes (32*index)))
  {offset := plan.offset+bytes.length
   maxEnd := if bytes.isEmpty then plan.maxEnd else max plan.maxEnd (plan.offset+32*count)
   writes := writes.reverse ++ plan.writes}

def packedFlush (plan : PackedStores) (pending : List PackedPiece) : PackedStores :=
  let pending := pending.reverse
  if pending.length ≥ 2 && pending.any (·.scalar) then
    packedEmit plan (pending.flatMap (·.bytes))
  else pending.foldl (fun plan piece => packedEmit plan piece.bytes) plan

def packedStorePlan (pieces : List PackedPiece) : PackedStores :=
  let (plan,pending,_) := pieces.foldl (fun (plan,pending,length) piece =>
    let size := piece.bytes.length
    if size > 32 || piece.scalar && size == 32 then
      (packedEmit (packedFlush plan pending) piece.bytes,[],0)
    else
      let (plan,pending,length) := if length+size > 32 then
        (packedFlush plan pending,[],0) else (plan,pending,length)
      (plan,piece::pending,length+size)) (({} : PackedStores),[],0)
  packedFlush plan pending

def abiPacked (frame : Frame) (parts : List PackedInput) (hash : Bool) (args : List Word) : Step := do
  let pieces ← packedPieces parts args
  let aligned := pieces.all (fun piece => piece.bytes.length % 32 == 0)
  let plan := packedStorePlan pieces
  let (base,frame) ← if hash then
      if plan.maxEnd ≤ 64 then pure (0,frame) else do
        let memory ← expand frame.memory 64 32
        pure (readWord memory 64,{frame with memory})
    else do
      match ← allocate frame (BitVec.ofNat 256 ((plan.offset+63)/32*32)) false false with
      | .returned (some pointer,frame) =>
        let memory ← expand frame.memory pointer.toNat 32
        pure (pointer,{frame with memory := writeBytes memory pointer.toNat (wordBytes (BitVec.ofNat 256 plan.offset))})
      | _ => .error "packed allocation precondition"
  let data := base + if hash then 0 else 32
  let mut memory := frame.memory
  for (offset,word) in plan.writes.reverse do
    let address := data+BitVec.ofNat 256 offset
    memory ← expand memory address.toNat 32
    memory := writeBytes memory address.toNat (wordBytes word)
  if !hash && !aligned then
    let address := data+BitVec.ofNat 256 plan.offset
    memory ← expand memory address.toNat 32
    memory := writeBytes memory address.toNat (wordBytes 0)
  if hash then
    memory ← expand memory data.toNat plan.offset
    return .returned (some (Keccak.hash (readBytes memory data.toNat plan.offset)),{frame with memory})
  else return .returned (some base,{frame with memory})

end SolarTV.MIR.State
