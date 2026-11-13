import SolarTV.MIR.State.StorageArrays
import SolarTV.MIR.State.Aggregates
import SolarTV.MIR.State.Frames
import SolarTV.MIR.State.Layout
import SolarTV.MIR.State.Abi
import SolarTV.MIR.State.AbiRequire
import SolarTV.MIR.State.PackedRefs
import SolarTV.Keccak

/-! Direct MIR scalar and state-operation semantics. Typed arithmetic reuses MIR's
width-polymorphic definitions. Memory is byte-addressed and storage is qualified
by its owner in the shared journal. Static violations are exceptional halts.
This layer assigns no physical stack, frame placement, implicit FMP setup or EVM
gas cost. Memory/work bounds are model errors, not program outcomes. -/

namespace SolarTV.MIR.State
open EvmRules SolarTV.Execution

def value (frame : Frame) (word : Word) : Step := .ok (.returned (some word,frame))
def effect (frame : Frame) : Step := .ok (.returned (none,frame))

private def checkedResult (frame : Frame) (result : Outcome (BitVec width)) : Step :=
  .ok (match result with
    | .returned v => .returned (some (v.setWidth 256),frame)
    | .reverted data => .reverted data
    | .invalid => .invalid)

/-- Copy the volatile return buffer without clearing the object's reserved padding. -/
def returnDataBytes (frame : Frame) : Step := do
  let length := frame.returnData.length
  if length + 63 ≥ 2^256 then return .reverted (.panic 0x41) else
  match ← allocate frame (BitVec.ofNat 256 ((length+63)/32*32)) false false with
  | .returned (some pointer,frame) =>
    let memory ← expand frame.memory pointer.toNat 32
    let memory := writeBytes memory pointer.toNat (wordBytes (BitVec.ofNat 256 length))
    let start := pointer + 32
    let memory ← expand memory start.toNat length
    value {frame with memory := writeBytes memory start.toNat frame.returnData} pointer
  | .reverted data => return .reverted data
  | .invalid => return .invalid
  | _ => .error "MIR returndata allocation result missing"

def memoryOperation (cfg : Config) (frame : Frame) (op : MemoryOp) (args : List Word) : Step := do
  match op,args with
  | .returnDataBytes,[] => returnDataBytes frame
  | .size,[] => value frame (BitVec.ofNat 256 frame.memory.length)
  | .returnDataSize,[] => value frame (BitVec.ofNat 256 frame.returnData.length)
  | .returnDataCopy,[destination,source,count] =>
    if source.toNat + count.toNat > frame.returnData.length then return .invalid
    else
      let memory ← expand frame.memory destination.toNat count.toNat
      effect {frame with memory := writeBytes memory destination.toNat (readBytes frame.returnData source.toNat count.toNat)}
  | .calldataSize,[] => value frame (BitVec.ofNat 256 cfg.environment.input.length)
  | .calldataLoad,[offset] => value frame (readWord cfg.environment.input offset.toNat)
  | .load,[offset] =>
    let memory ← expand frame.memory offset.toNat 32
    value {frame with memory} (readWord memory offset.toNat)
  | .store,[offset,word] | .storeByte,[offset,word] =>
    let bytes := match op with | .storeByte => [word.setWidth 8] | _ => wordBytes word
    let memory ← expand frame.memory offset.toNat bytes.length
    effect {frame with memory := writeBytes memory offset.toNat bytes}
  | .copy,[destination,source,size] | .calldataCopy,[destination,source,size] =>
    let memory ← expand frame.memory destination.toNat size.toNat
    let memory ← match op with | .copy => expand memory source.toNat size.toNat | _ => pure memory
    let sourceBytes := match op with | .copy => memory | _ => cfg.environment.input
    effect {frame with memory := writeBytes memory destination.toNat (readBytes sourceBytes source.toNat size.toNat)}
  | .zero,[offset,size] =>
    let memory ← expand frame.memory offset.toNat size.toNat
    effect {frame with memory := writeBytes memory offset.toNat (List.replicate size.toNat 0)}
  | .hash,[offset,size] =>
    let memory ← expand frame.memory offset.toNat size.toNat
    value {frame with memory} (Keccak.hash (readBytes memory offset.toNat size.toNat))
  | _,_ => .error "MIR memory operand count"

def storageOperation (cfg : Config) (frame : Frame) (transient write : Bool) (args : List Word) : Step := do
  let owner := cfg.environment.address
  let old := account frame.world owner
  let slots := if transient then old.transient else old.storage
  match write,args with
  | false,[key] => value (if transient then frame else
      {frame with world := warmSlot frame.world owner key}) (slotsRead slots key)
  | true,[key,word] =>
    if cfg.environment.isStatic then return .invalid
    let updated := slotsWrite slots key word
    let world := putAccount (if transient then frame.world else warmSlot frame.world owner key)
      (if transient then {old with transient := updated} else {old with storage := updated})
    effect {frame with world}
  | _,_ => .error "MIR storage operand count"

def contextOperation (cfg : Config) (frame : Frame) (opcode : Nat) (args : List Word) : Step :=
  match opcode,args with
  | 0x38,[] => do
    let code ← required cfg.layout.code "missing explicit MIR code image"
    value frame (BitVec.ofNat 256 code.length)
  | 0x39,[destination,offset,count] => do
    let code ← required cfg.layout.code "missing explicit MIR code image"
    let memory ← expand frame.memory destination.toNat count.toNat
    effect {frame with memory := writeBytes memory destination.toNat (readBytes code offset.toNat count.toNat)}
  | 0x31,[argument] | 0x3b,[argument] | 0x3f,[argument] =>
    let target := argument.setWidth 160
    if opcode != 0x31 && target == cfg.environment.address then
      .error "MIR root runtime code is unknown" else
    let result := if opcode = 0x31 then (account frame.world target).balance
      else if opcode = 0x3b then BitVec.ofNat 256 (account frame.world target).code.length
      else if frame.world.accounts.any (fun a => a.address == target) then Keccak.hash (account frame.world target).code else 0
    value {frame with world := warmAddress frame.world target} result
  | 0x3c,[argument,destination,offset,count] => do
    let target := argument.setWidth 160
    if target == cfg.environment.address then .error "MIR root runtime code is unknown" else
    let memory ← expand frame.memory destination.toNat count.toNat
    let bytes := readBytes (account frame.world target).code offset.toNat count.toNat
    effect {frame with memory := writeBytes memory destination.toNat bytes,world := warmAddress frame.world target}
  | 0x30,[] => value frame (cfg.environment.address.setWidth 256)
  | 0x32,[] => value frame (cfg.environment.origin.setWidth 256)
  | 0x33,[] => value frame (cfg.environment.caller.setWidth 256)
  | 0x34,[] => value frame cfg.environment.value
  | 0x3a,[] => value frame cfg.environment.gasPrice
  | 0x47,[] => value frame (account frame.world cfg.environment.address).balance
  | 0x40,[argument] | 0x49,[argument] => value frame (Execution.environmentValue cfg.fork cfg.context opcode argument)
  | 0x41,[] | 0x42,[] | 0x43,[] | 0x44,[] | 0x45,[] | 0x46,[] | 0x48,[] | 0x4a,[] | 0x4b,[] =>
    value frame (Execution.environmentValue cfg.fork cfg.context opcode 0)
  | _,_ => .error "unmodeled MIR context read"

def operation (cfg : Config) (frame : Frame) (op : Operation) (args : List Word) (activation : Nat := 0) : Step := do
  match op,args with
  | .binary op width,[a,b] => value frame (widenBits (MIR.eval op (wordBits width a) (wordBits width b)))
  | .comparison op width,[a,b] => value frame (widenBits (MIR.compare op (wordBits width a) (wordBits width b)))
  | .unary op width,[a] => value frame (widenBits (MIR.unary op (wordBits width a)))
  | .shift op width,[amount,v] => value frame (widenBits (MIR.shift op (wordBits width v) (wordBits width amount)))
  | .wordBinary op,[a,b] => value frame (MIR.wordBinary op a b)
  | .ternary op checked,[a,b,c] =>
    if checked && c == 0 then return .reverted (.panic 0x12)
    else value frame (MIR.ternary op a b c)
  | .word,[a] => value frame a
  | .freePointer,[] => freePointer frame
  | .allocate zeroed checked,[size] => allocate frame size zeroed checked
  | .deferredAllocate function site zeroed checked,[size] => deferredAllocate cfg frame function site size zeroed checked
  | .insertValue ..,_ | .extractValue ..,_ | .makeSlice ..,_ => .error "aggregate operation needs typed operands"
  | .object (.bytes .validate),[header] =>
    match bytesHeader header with | .ok _ => effect frame | .error data => return .reverted data
  | .object (.bytes .load),[slot] => loadStorageBytes cfg frame slot
  | .object (.bytes .store),[slot,object] => storeStorageBytes cfg frame slot object none
  | .object (.bytes (.literal bytes)),[slot] => storeStorageBytes cfg frame slot 0 (some bytes)
  | .object (.bytes .clear),[slot,first,last] => clearStorageWords cfg frame slot first last
  | .object (.array element variants),[slot] => loadStorageArray cfg frame slot element variants
  | .dataCopy bytes offset,[destination,count] =>
    if offset + count.toNat > bytes.length then .error "datacopy past its data" else
    let memory ← expand frame.memory destination.toNat count.toNat
    effect {frame with memory := writeBytes memory destination.toNat (readBytes bytes offset count.toNat)}
  | .slot .mapping,[key,slot] =>
    let (word,next) ← fixedSlot frame [key,slot]
    value next word
  | .slot .arrayData,[slot] =>
    let (word,next) ← fixedSlot frame [slot]
    value next word
  | .slot (.arrayElement stride),[slot,index] =>
    let (word,next) ← fixedSlot frame [slot]
    value next (word + index * BitVec.ofNat 256 stride)
  | .slot .bytesKey,_ => .error "bytes mapping needs typed operands"
  | .slot .namespace,[object] =>
    let (word,next) ← namespaceSlot frame object
    value next word
  | .external _,_ => .error "external instruction requires MIR machine context"
  | .call _,_ => .error "internal call requires MIR machine context"
  | .pointerAdd,[pointer,offset] => value frame (pointer + offset)
  | .lowBits width,[a] => value frame (narrow width a)
  | .signExtend source target,[a] => value frame (((a.setWidth source).signExtend target).setWidth 256)
  | .notEqual,[a,b] => value frame (if a = b then 0 else 1)
  | .select,[c,a,b] => value frame (if c = 0 then b else a)
  | .checked op width signed,[a,b] => checkedResult frame (MIR.checked op signed (wordBits width a) (wordBits width b))
  | .checkedPower width signed,[a,b] => checkedResult frame (MIR.checkedPower signed (wordBits width a) b.toNat)
  | .check zero panic message,[condition] =>
    if (condition == 0) == zero then
      return .reverted (match panic with
        | some code => .panic code
        | none => if cfg.debugReverts then
            message.map (fun bytes => RevertData.fromBytes (requireLiteral bytes)) |>.getD .empty
          else .empty)
    else effect frame
  | .memory op,args => memoryOperation cfg frame op args
  | .storage transient write,args => storageOperation cfg frame transient write args
  | .context (.environment opcode),args => contextOperation cfg frame opcode args
  | .context op,args => layoutOperation cfg frame op args activation
  | .log count,offset :: size :: topics =>
    if count != topics.length then .error "MIR log topic count" else
    if cfg.environment.isStatic then return .invalid else
      let memory ← expand frame.memory offset.toNat size.toNat
      effect {frame with memory, world := {frame.world with logs := frame.world.logs ++
        [⟨cfg.environment.address,topics,readBytes memory offset.toNat size.toNat⟩]}}
  | _,_ => .error "MIR operation operand count"

def assignValue (next : Frame) (result : Option Nat) (ty : ValueType) (value : Value) : Step := do
  let some id := result | .error "MIR result arity mismatch"
  if !value.fits 34 ty then .error "MIR result type mismatch" else
    effect {next with locals := put next.locals id value}

def assign (next : Frame) (result : Option Nat) (ty : ValueType) (word : Option Word) : Step := do
  match result,word with
  | none,none => effect next
  | some _,some word =>
    if ty.width = 0 || ty.width > 256 then .error "unmodeled MIR result type" else
      assignValue next result ty (.scalar (Scalar.ofWord ty.width word))
  | _,_ => .error "MIR result arity mismatch"

def instruction (cfg : Config) (frame : Frame) (inst : Instruction) (activation : Nat := 0) : Step := do
  if inst.opcode.any (fun opcode => !cfg.available.contains opcode) then return .invalid
  match inst.operation,inst.operands with
  | .abi (.encode mode hasBase hasSelector functions),_ =>
    let args ← inst.operands.mapM (read frame)
    let (value,next) ← abiEncode frame mode hasBase hasSelector functions args
    assignValue next inst.result inst.ty value
  | .abi (.decode rule function calldata),[input] =>
    let result ← abiDecode cfg frame rule function calldata (← readValue frame input)
    match result with
    | .returned (word,next) => assign next inst.result inst.ty word
    | .reverted data => return .reverted data
    | .invalid => return .invalid
  | .abi (.require op),operands =>
    let args ← operands.mapM (readValue frame)
    abiRequirePayload cfg frame op args
  | .abi (.encodeTree mode hasBase hasSelector shapes memorySlices),operands =>
    let args ← operands.mapM (readValue frame)
    match ← abiEncodeTree cfg frame mode hasBase hasSelector shapes memorySlices args with
    | .returned (value,next) => assignValue next inst.result inst.ty value
    | .reverted data => return .reverted data
    | .invalid => return .invalid
  | .abi (.decodeTree shapes calldata),[input] =>
    match ← abiDecodeTree cfg frame shapes calldata (← readValue frame input) with
    | .returned (value,next) =>
      match value with
      | none => if inst.result.isNone then effect next else .error "ABI decode result arity"
      | some value => assignValue next inst.result inst.ty value
    | .reverted data => return .reverted data
    | .invalid => return .invalid
  | .abi (.validate rule),[input] =>
    let word ← read frame input
    if !abiCanonical rule word then .error "invalid decoder ABI evidence" else effect frame
  | .abi (.validate rule),[_,raw] =>
    let word ← read frame raw
    if !abiCanonical rule word then .error "invalid decoder ABI evidence" else effect frame
  | .abi (.packed parts hash),_ =>
    let args ← inst.operands.mapM (readValue frame)
    match ← abiPackedValues cfg frame parts hash args with
    | .returned (word,next) => assign next inst.result inst.ty word
    | .reverted data => return .reverted data
    | .invalid => return .invalid
  | .insertValue ty index,[aggregate,field] =>
    let aggregate ← readValue frame aggregate
    let value ← readValue frame field
    let updated ← insertField ty aggregate index value
    assignValue frame inst.result inst.ty updated
  | .extractValue ty index,[aggregate] =>
    let aggregate ← readValue frame aggregate
    let value ← extractField ty aggregate index
    assignValue frame inst.result inst.ty value
  | .makeSlice location,[pointer,length] =>
    let pointer ← read frame pointer
    let length ← read frame length
    if !(inst.ty.same (.slice location)) then .error "MIR slice location mismatch" else
      assignValue frame inst.result inst.ty (.aggregate [
        .scalar (Scalar.ofWord 256 pointer),.scalar (Scalar.ofWord 256 length)])
  | .slot .bytesKey,[key,slot] =>
    let key ← readValue frame key
    let slot ← read frame slot
    let (word,next) ← bytesSlot cfg frame key slot
    assign next inst.result inst.ty (some word)
  | .select,[condition,yes,no] =>
    let condition ← read frame condition
    let yes ← readValue frame yes
    let no ← readValue frame no
    assignValue frame inst.result inst.ty (if condition = 0 then no else yes)
  | .frame false address ty,[] =>
    let (value,next) ← loadFrame frame address ty
    assignValue next inst.result inst.ty value
  | .frame true address ty,[operand] =>
    let value ← readValue frame operand
    effect (← storeFrame frame address ty value)
  | .internalFrame false name offset ty,[] =>
    let address := (← activationBase cfg name activation)+BitVec.ofNat 256 offset
    let (value,next) ← loadFrame frame address ty
    assignValue next inst.result inst.ty value
  | .internalFrame true name offset ty,[operand] =>
    let address := (← activationBase cfg name activation)+BitVec.ofNat 256 offset
    let value ← readValue frame operand
    effect (← storeFrame frame address ty value)
  | _,_ =>
    let args ← inst.operands.mapM (read frame)
    let result ← operation cfg frame inst.operation args activation
    match result with
    | .returned (word,next) => assign next inst.result inst.ty word
    | .reverted data => return .reverted data
    | .invalid => return .invalid

end SolarTV.MIR.State
