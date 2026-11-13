import SolarTV.MIR.State.Allocation
import SolarTV.Keccak

/-! Direct scalar ABI operations. Static tuple encoding preserves allocation and
selector/header store order, including dirty padding. Decoding validates the
signed input length and word canonicality before normalization. Markers are
admitted only with decoder provenance, checked by the structural importer and
independently by replay. Packed encodings retain full-word writes and the trailing
zero word; hashes admit a merged word or whole-word parts. Recursive shapes use the separate AbiDecode and AbiEncode models. Physical
call-return buffers remain unsupported, not lowered by the checker or replaced
with an oracle. -/

namespace SolarTV.MIR.State
open EvmRules SolarTV.Execution

def abiCanonical (rule : AbiRule) (word : Word) : Bool :=
  match rule with
  | .any => true
  | .uint bits => word == narrow bits word
  | .left bits => word &&& BitVec.ofNat 256 (2^(256-bits)-1) == 0
  | .sint bits => word == (word.setWidth bits).signExtend 256
  | .boolean => word.toNat ≤ 1
  | .enumeration bound => word.toNat < bound

/-- Write the statically shaped tuple in order; only function words need normalization. -/
def abiWriteWords (memory : List Byte) (start : Word) : List (Word × Bool) → Except String (List Byte)
  | [] => .ok memory
  | (word,function)::rest => do
    let word := if function then word <<< 64 else word
    let memory ← expand memory start.toNat 32
    abiWriteWords (writeBytes memory start.toNat (wordBytes word)) (start+32) rest

def abiEncode (frame : Frame) (mode : Nat) (hasBase hasSelector : Bool)
    (functions : List Bool) (args : List Word) : Except String (Value × Frame) := do
  let (base,args) ← if hasBase then do
      let base ← required args[0]? "missing ABI encoding base"
      pure (some base,args.drop 1)
    else pure (none,args)
  let (selector,args) ← if hasSelector then do
      let selector ← required args[0]? "missing ABI selector"
      pure (some selector,args.drop 1)
    else pure (none,args)
  if args.length != functions.length || mode > 2 then .error "ABI encode shape" else
  let total := args.length*32 + if hasSelector then 4 else 0
  let header := mode == 1 || mode == 0 && !hasSelector
  let (pointer,frame) ← if mode == 2 then do
      match base with
      | some pointer => pure (pointer,frame)
      | none => do
        let memory ← expand frame.memory 64 32
        pure (readWord memory 64,{frame with memory})
    else do
      let size := (total+31)/32*32 + if header then 32 else 0
      match ← allocate frame (BitVec.ofNat 256 size) false false with
      | .returned (some pointer,frame) => pure (pointer,frame)
      | _ => .error "ABI encoding allocation precondition"
  let mut memory := frame.memory
  if header then
    memory ← expand memory pointer.toNat 32
    memory := writeBytes memory pointer.toNat (wordBytes (BitVec.ofNat 256 total))
  let data := pointer + if header then 32 else 0
  if let some selector := selector then
    memory ← expand memory data.toNat 32
    memory := writeBytes memory data.toNat (wordBytes selector)
  let start := data + if hasSelector then 4 else 0
  memory ← abiWriteWords memory start (args.zip functions)
  let result := if mode == 1 then Value.scalar (Scalar.ofWord 256 pointer) else
    .aggregate [.scalar (Scalar.ofWord 256 data),.scalar (Scalar.ofWord 256 (BitVec.ofNat 256 total))]
  return (result,{frame with memory})

def abiDecode (cfg : Config) (frame : Frame) (rule : Option AbiRule)
    (function calldata : Bool) (input : Value) : Step := do
  let (base,length,frame) ← match input with
    | .scalar object => do
      if calldata then .error "calldata decode needs a slice" else
      let memory ← expand frame.memory object.word.toNat 32
      pure (object.word+32,readWord memory object.word.toNat,{frame with memory})
    | .aggregate [base,length] => pure (← base.word,← length.word,frame)
    | _ => .error "undefined ABI decode input"
  let head := if rule.isSome then 32 else 0
  if length.toNat ≥ 2^255 || length.toNat < head then return .reverted .empty else
  match rule with
  | none => return .returned (none,frame)
  | some rule =>
    let memory ← if calldata then pure frame.memory else expand frame.memory base.toNat 32
    let word := readWord (if calldata then cfg.environment.input else memory) base.toNat
    if !abiCanonical rule word then return .reverted .empty else
      return .returned (some (if function then word >>> 64 else word),{frame with memory})

def packedBytes (parts : List PackedInput) (args : List Word) : Except String (List Byte × Bool) := do
  let mut bytes := []
  let mut aligned := true
  for part in parts do
    match part with
    | .literal data => bytes := bytes ++ data; aligned := aligned && data.length % 32 == 0
    | .scalar index size fixed =>
      if size = 0 || size > 32 then .error "packed scalar width" else
      let word ← required args[index]? "packed scalar operand"
      let data := wordBytes word
      bytes := bytes ++ (if fixed then data.take size else data.drop (32-size))
      aligned := aligned && size == 32
    if bytes.length > 65536 then .error "packed bytes exceed 64 KiB" else pure ()
  return (bytes,aligned)

def abiPacked (frame : Frame) (parts : List PackedInput) (hash : Bool) (args : List Word) : Step := do
  let (bytes,aligned) ← packedBytes parts args
  if hash && bytes.length > 32 && !aligned then .error "packed hash requires one merged word or whole-word parts" else
  let (base,frame) ← if hash then
      if bytes.length ≤ 64 then pure (0,frame) else do
        let memory ← expand frame.memory 64 32
        pure (readWord memory 64,{frame with memory})
    else do
      match ← allocate frame (BitVec.ofNat 256 ((bytes.length+63)/32*32)) false false with
      | .returned (some pointer,frame) =>
        let memory ← expand frame.memory pointer.toNat 32
        pure (pointer,{frame with memory := writeBytes memory pointer.toNat (wordBytes (BitVec.ofNat 256 bytes.length))})
      | _ => .error "packed allocation precondition"
  let data := base + if hash then 0 else 32
  let mut memory := frame.memory
  for index in List.range ((bytes.length+31)/32) do
    let address := data + BitVec.ofNat 256 (32*index)
    memory ← expand memory address.toNat 32
    memory := writeBytes memory address.toNat (wordBytes (readWord bytes (32*index)))
  if !hash && !aligned then
    let address := data + BitVec.ofNat 256 bytes.length
    memory ← expand memory address.toNat 32
    memory := writeBytes memory address.toNat (wordBytes 0)
  if hash then
    memory ← expand memory data.toNat bytes.length
    return .returned (some (Keccak.hash (readBytes memory data.toNat bytes.length)),{frame with memory})
  else return .returned (some base,{frame with memory})

end SolarTV.MIR.State
