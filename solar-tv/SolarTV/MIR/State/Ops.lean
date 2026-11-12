import SolarTV.MIR.State.IR
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

def memoryOperation (cfg : Config) (frame : Frame) (op : MemoryOp) (args : List Word) : Step := do
  match op,args with
  | .size,[] => value frame (BitVec.ofNat 256 frame.memory.length)
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
  | .hash,[offset,size] =>
    let memory ← expand frame.memory offset.toNat size.toNat
    value {frame with memory} (Keccak.hash (readBytes memory offset.toNat size.toNat))
  | _,_ => .error "MIR memory operand count"

def storageOperation (cfg : Config) (frame : Frame) (transient write : Bool) (args : List Word) : Step := do
  let owner := cfg.environment.address
  let old := account frame.world owner
  let slots := if transient then old.transient else old.storage
  match write,args with
  | false,[key] => value frame (slotsRead slots key)
  | true,[key,word] =>
    if cfg.environment.isStatic then return .invalid
    let updated := slotsWrite slots key word
    let world := putAccount frame.world
      (if transient then {old with transient := updated} else {old with storage := updated})
    effect {frame with world}
  | _,_ => .error "MIR storage operand count"

def contextOperation (cfg : Config) (frame : Frame) (opcode : Nat) (args : List Word) : Step :=
  match opcode,args with
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

def operation (cfg : Config) (frame : Frame) (op : Operation) (args : List Word) : Step := do
  match op,args with
  | .binary op width,[a,b] => value frame ((MIR.eval op (a.setWidth width) (b.setWidth width)).setWidth 256)
  | .comparison op width,[a,b] => value frame ((MIR.compare op (a.setWidth width) (b.setWidth width)).setWidth 256)
  | .unary op width,[a] => value frame ((MIR.unary op (a.setWidth width)).setWidth 256)
  | .shift op width,[amount,v] => value frame ((MIR.shift op (v.setWidth width) (amount.setWidth width)).setWidth 256)
  | .wordBinary op,[a,b] => value frame (MIR.wordBinary op a b)
  | .ternary op,[a,b,c] => value frame (MIR.ternary op a b c)
  | .word,[a] => value frame a
  | .pointerAdd,[pointer,offset] => value frame (pointer + offset)
  | .lowBits width,[a] => value frame (narrow width a)
  | .signExtend source target,[a] => value frame (((a.setWidth source).signExtend target).setWidth 256)
  | .notEqual,[a,b] => value frame (if a = b then 0 else 1)
  | .select,[c,a,b] => value frame (if c = 0 then b else a)
  | .checked op width signed,[a,b] => checkedResult frame (MIR.checked op signed (a.setWidth width) (b.setWidth width))
  | .checkedPower width signed,[a,b] => checkedResult frame (MIR.checkedPower signed (a.setWidth width) b.toNat)
  | .check zero panic,[condition] =>
    if (condition == 0) == zero then
      return .reverted (match panic with | none => .empty | some code => .panic code)
    else effect frame
  | .memory op,args => memoryOperation cfg frame op args
  | .storage transient write,args => storageOperation cfg frame transient write args
  | .environment opcode,args => contextOperation cfg frame opcode args
  | .log count,offset :: size :: topics =>
    if count != topics.length then .error "MIR log topic count" else
    if cfg.environment.isStatic then return .invalid else
      let memory ← expand frame.memory offset.toNat size.toNat
      effect {frame with memory, world := {frame.world with logs := frame.world.logs ++
        [⟨cfg.environment.address,topics,readBytes memory offset.toNat size.toNat⟩]}}
  | _,_ => .error "MIR operation operand count"

def instruction (cfg : Config) (frame : Frame) (inst : Instruction) : Step := do
  if inst.opcode.any (fun opcode => !cfg.available.contains opcode) then return .invalid
  let args ← inst.operands.mapM (read frame)
  let result ← operation cfg frame inst.operation args
  match result with
  | .returned (word,next) =>
    match inst.result,word with
    | none,none => effect next
    | some id,some word =>
      if inst.width = 0 || inst.width > 256 then .error "unmodeled MIR result type" else
        effect {next with locals := put next.locals id (narrow inst.width word)}
    | _,_ => .error "MIR result arity mismatch"
  | .reverted data => return .reverted data
  | .invalid => return .invalid

end SolarTV.MIR.State
