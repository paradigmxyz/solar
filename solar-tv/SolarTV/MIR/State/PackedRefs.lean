import SolarTV.MIR.State.PackedArrays
import SolarTV.MIR.State.StorageBytes

/-! Packed byte references resolve headers before reserving output and read each
payload when its copy occurs. Copies retain aliases with earlier writes. Storage
keeps full dirty words, short-header validation and hashing scratch. Hashes that
borrow a memory object's restored header have no final memory write. This model
uses declared MIR layouts and the byte memory, never compiler lowering output. -/

namespace SolarTV.MIR.State
open EvmRules SolarTV.Execution

inductive PackedReference where
  | static (piece : PackedPiece)
  | bytes (location : Nat) (source : Word) (length : Nat) (object : Bool)
  | storage (slot header : Word) (long : Bool) (length : Nat)
  | array (value : PackedArray)

def PackedReference.length : PackedReference → Nat
  | .static piece => piece.bytes.length
  | .bytes _ _ length _ | .storage _ _ _ length => length
  | .array value => value.bytes

def packedArgument (args : List Value) (index : Nat) : AbiM Value :=
  match args[index]? with
  | none => throw (.unsupported "packed operand")
  | some value => pure value

def packedWord (value : Value) : AbiM Word :=
  match value.word with
  | .error reason => throw (.unsupported reason)
  | .ok word => pure word

def packedPrepare (cfg : Config) (parts : List PackedInput) (args : List Value) : AbiM (List PackedReference × Nat) := do
  if parts.length > 2048 then throw (.unsupported "packed part limit of 2048")
  let mut result := []
  let mut total := 0
  for part in parts do
    let piece ← match part with
      | .literal bytes => pure (.static ⟨bytes,false⟩)
      | .scalar index size fixed => do
        let word ← packedWord (← packedArgument args index)
        match packedPieces [.scalar 0 size fixed] [word] with
        | .ok [piece] => pure (.static piece)
        | .error reason => throw (.unsupported reason)
        | _ => throw (.unsupported "packed scalar layout")
      | .bytes index location => do
        let input ← packedArgument args index
        if location == 3 then
          let slot ← packedWord input
          let state ← get
          let (header,frame) := storageRead cfg state.frame slot
          set {state with frame}
          match bytesHeader header with
          | .error data => throw (.reverted data)
          | .ok (long,length) => pure (.storage slot header long length)
        else
          match input with
          | .scalar pointer =>
            if location != 0 then throw (.unsupported "packed byte representation")
            let length ← abiInputWord cfg false pointer.word
            pure (.bytes 0 (pointer.word+32) length.toNat true)
          | .aggregate [source,length] => do
            let source ← packedWord source
            let length ← packedWord length
            pure (.bytes location source length.toNat false)
          | _ => throw (.unsupported "packed byte slice")
      | .array index element object location length stride wordwise => do
        let input ← packedArgument args index
        let array ← preparePackedArray cfg input element object location length stride wordwise
        pure (.array array)
    total := total+piece.length
    if total > 65536 then throw (.unsupported "packed bytes exceed 64 KiB")
    result := piece::result
  return (result.reverse,total)

def packedStaticSegment (base : Word) (pieces : List PackedPiece) : AbiM Word := do
  let plan := packedStorePlan pieces
  for (offset,word) in plan.writes.reverse do
    abiStore (base+BitVec.ofNat 256 offset) word
  return base+BitVec.ofNat 256 plan.offset

def packedBorrow (pieces : List PackedReference) : Option (List Byte × Word × Nat) := do
  let last ← pieces.getLast?
  let .bytes 0 source length true := last | none
  let leading ← (pieces.dropLast).mapM fun piece => match piece with
    | .static part => some part.bytes
    | _ => none
  let leading := leading.flatten
  if leading.length > 32 then none else some (leading,source,length)

def packedReferenceProgram (cfg : Config) (parts : List PackedInput) (hash : Bool) (args : List Value) : AbiM Word := do
  let (pieces,total) ← packedPrepare cfg parts args
  let aligned := pieces.all fun piece => match piece with
    | .static piece => piece.bytes.length % 32 == 0
    | .array _ => true
    | _ => false
  if hash then
    if let some (leading,source,length) := packedBorrow pieces then
      return ← abiMemory fun memory => do
        let memory ← expand memory source.toNat length
        return (Keccak.hash (leading ++ readBytes memory source.toNat length),memory)
  let base ← if hash then abiInputWord cfg false 64 else do
    let base ← abiReserve (BitVec.ofNat 256 ((total+63)/32*32)) false
    abiStore base (BitVec.ofNat 256 total)
    pure base
  let start := base + if hash then 0 else 32
  let mut cursor := start
  let mut pending := []
  for piece in pieces do
    match piece with
    | .static part => pending := part::pending
    | _ =>
      cursor ← packedStaticSegment cursor pending.reverse
      pending := []
      match piece with
      | .bytes location source length _ => abiCopyFrom cfg location cursor source (BitVec.ofNat 256 length)
      | .storage slot header long length =>
        if long then
          let state ← get
          match fixedSlot state.frame [slot] with
          | .error reason => throw (.unsupported reason)
          | .ok (parent,frame) =>
            set {state with frame}
            for index in List.range ((length+31)/32) do
              let state ← get
              let (word,frame) := storageRead cfg state.frame (parent+BitVec.ofNat 256 index)
              set {state with frame}
              abiStore (cursor+BitVec.ofNat 256 (index*32)) word
        else abiStore cursor (header &&& ~~~(255 : Word))
      | .array array => writePackedArray 33 cfg array cursor true
      | .static _ => pure ()
      cursor := cursor+BitVec.ofNat 256 piece.length
  let last ← packedStaticSegment cursor pending.reverse
  if !hash && !aligned then abiStore last 0
  if hash then
    abiMemory fun memory => do
      let memory ← expand memory start.toNat total
      return (Keccak.hash (readBytes memory start.toNat total),memory)
  else pure base

def abiPackedValues (cfg : Config) (frame : Frame) (parts : List PackedInput) (hash : Bool) (args : List Value) : Step := do
  if parts.all (fun part => match part with | .bytes .. | .array .. => false | _ => true) then
    abiPacked frame parts hash (← args.mapM Value.word)
  else
    match (packedReferenceProgram cfg parts hash args).run ⟨frame,2048⟩ with
    | .error (.unsupported reason) => .error reason
    | .error (.reverted data) => .ok (.reverted data)
    | .error .invalid => .ok .invalid
    | .ok (word,state) => .ok (.returned (some word,state.frame))

end SolarTV.MIR.State
