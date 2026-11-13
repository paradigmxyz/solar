import SolarTV.MIR.State.Values
import SolarTV.EVMIR.State.Types
import Init.Data.List.Monadic
import SolarTV.State.ByteMemory
import SolarTV.State.Context

/-! Structured typed SSA for direct MIR execution, including immutable products. The compiler's parser and
operation schema supply IDs, types and operation identities; the importer does
not lower MIR or synthesize backend addresses. Phis remain edge-selected and
simultaneous, and blocks retain their CFG, including cycles. -/

namespace SolarTV.MIR.State
open EvmRules SolarTV.Execution

inductive Atom where
  | constant (value : Scalar)
  | argument (index : Nat)
  | local (id : Nat)
  | undefined

structure Operand where
  atom : Atom
  ty : ValueType

inductive MemoryOp where
  | load | store | storeByte | copy | hash | size | zero
  | calldataLoad | calldataSize | calldataCopy | returnDataSize | returnDataCopy | returnDataBytes

inductive SlotOp where
  | mapping | bytesKey | arrayData | arrayElement (stride : Nat)

inductive StorageBytesOp where
  | validate | load | store | literal (bytes : List Byte) | clear

inductive ArrayElement where
  | uint (bits : Nat) | sint (bits : Nat) | fixedBytes (bits : Nat) | bytes

inductive MessageOp where
  | raw (opcode : Nat)
  | address (opcode : Nat) (hasValue : Bool)

inductive StorageObjectOp where
  | bytes (op : StorageBytesOp)
  | array (element : ArrayElement) (variants : Option Nat)

inductive ContextOp where
  | environment (opcode : Nat)
  | heapFloor (function : String)
  | frameAddress (function : String) (offset : Nat)
  | constructorRange (isEnd : Bool)
  | dataSize (id addend : Nat) (aligned : Bool)
  | immutableRead (id : Nat)
  | immutableStore (id : Nat)
  | library (id : Nat)
  | gasSample

inductive AbiRule where
  | any | uint (bits : Nat) | left (bits : Nat) | sint (bits : Nat) | boolean | enumeration (bound : Nat)
  deriving BEq

inductive PackedInput where
  | literal (bytes : List SolarTV.Execution.Byte)
  | scalar (index size : Nat) (fixedBytes : Bool)

inductive AbiShape where
  | scalar (rule : AbiRule) (function : Bool) (width : Nat)
  | bytes
  | array (element : AbiShape) (length : Option Nat)
  | tuple (fields : List AbiShape)

inductive AbiEncodeShape where
  | word (cleanup : AbiRule)
  | function
  | bytes (location : Nat)
  | array (element : AbiEncodeShape) (length : Option Nat) (location : Nat) (blockCopy : Bool)
  | tuple (fields : List AbiEncodeShape)

inductive AbiOp where
  | encode (mode : Nat) (hasBase hasSelector : Bool) (functions : List Bool)
  | decode (rule : Option AbiRule) (function calldata : Bool)
  | decodeTree (shapes : List AbiShape) (calldata : Bool)
  | encodeTree (mode : Nat) (hasBase hasSelector : Bool) (shapes : List AbiEncodeShape) (memorySlices : List Bool)
  | validate (rule : AbiRule)
  | packed (parts : List PackedInput) (hash : Bool)

structure FunctionPlacement where
  heapFloor : Option EvmRules.Word := none
  frameBase : Option EvmRules.Word := none

structure Layout where
  code : Option (List SolarTV.Execution.Byte) := none
  functions : List (String × FunctionPlacement) := []
  constructor : Option (Nat × Nat) := none
  data : List (Nat × (Nat × Nat)) := []
  immutableSlots : List (Nat × Nat) := []
  immutableValues : List (Nat × EvmRules.Word) := []
  libraries : List (Nat × EvmRules.Word) := []
  gasBefore : Option Nat := none

inductive Operation where
  | binary (op : MIR.Op) (width : Nat)
  | comparison (op : MIR.Comparison) (width : Nat)
  | unary (op : Word.Unary) (width : Nat)
  | shift (op : Word.Shift) (width : Nat)
  | wordBinary (op : MIR.WordBinary)
  | ternary (op : MIR.Ternary)
  | word | pointerAdd | lowBits (width : Nat) | signExtend (source target : Nat)
  | notEqual | select
  | checked (op : MIR.CheckedOp) (width : Nat) (signed : Bool)
  | checkedPower (width : Nat) (signed : Bool)
  | check (zero : Bool) (panic : Option Word)
  | memory (op : MemoryOp)
  | storage (transient write : Bool)
  | context (op : ContextOp)
  | abi (op : AbiOp)
  | log (topics : Nat)
  | call (callee : Nat)
  | freePointer
  | allocate (zeroed checked : Bool)
  | external (op : MessageOp)
  | insertValue (ty : ValueType) (index : Nat)
  | extractValue (ty : ValueType) (index : Nat)
  | makeSlice (location : Nat)
  | slot (op : SlotOp)
  | object (op : StorageObjectOp)
  | dataCopy (bytes : List Byte) (offset : Nat)
  | frame (write : Bool) (address : Word) (ty : ValueType)

structure Instruction where
  result : Option Nat
  ty : ValueType
  opcode : Option Nat
  operation : Operation
  operands : List Operand

structure Phi where
  result : Nat
  ty : ValueType
  incoming : List (Nat × Operand)

inductive Terminator where
  | jump (target : Nat)
  | branch (condition : Operand) (yes no : Nat)
  | switch (value : Operand) (fallback : Nat) (cases : List (Operand × Nat))
  | returned (values : List Operand)
  | returndata (offset size : Operand)
  | reverted (offset size : Operand)
  | tailCall (callee : Nat) (args : List Operand)
  | stop | invalid

structure Block where
  phis : List Phi
  instructions : List Instruction
  terminator : Terminator

/-- Typed bodies retain their own SSA namespaces; calls share only execution state. -/
structure Function where
  params : List ValueType
  returns : List ValueType
  blocks : List Block

abbrev Functions := List (Nat × Function)

/-- An explicit control position, independent of physical EVM stack or frame placement. -/
structure Cursor where
  function : Nat
  block : Nat := 0
  next : Nat := 0
  entered : Bool := false
  predecessor : Option Nat := none

structure Config where
  fork : Fork
  environment : World.Environment
  context : Execution.Block
  available : List Nat
  entry : Bool
  callDepth : Nat := 128
  externalConfig : Option EVM.State.Config := none
  creationGas : List Nat := []
  layout : Layout := {}

structure Frame where
  world : Journal
  memory : List Byte
  arguments : List Value
  locals : List (Nat × Value) := []
  returnData : List Byte := []
  creationIndex : Nat := 0

/-- A caller continuation owns its SSA snapshot, not a private copy of execution memory. -/
structure Continuation where
  cursor : Cursor
  frame : Frame
  result : Option Nat
  ty : ValueType

inductive Completion where
  | returned (values : List Word)
  | success (data : List Byte)
  | reverted (data : List Byte)
  | invalid

structure Result where
  completion : Completion
  world : Journal
  memory : List Byte
  returnData : List Byte := []
  creationIndex : Nat := 0

abbrev Run := Except String Result
abbrev Step := Except String (Outcome (Option Word × Frame))

def narrow (width : Nat) (value : Word) : Word :=
  if width = 256 then value else (value.setWidth width).setWidth 256

@[simp] theorem narrow_full (value : Word) : narrow 256 value = value := by
  simp [narrow]

def required (value : Option α) (reason : String) : Except String α :=
  match value with | some value => .ok value | none => .error reason

def readValue (frame : Frame) (operand : Operand) : Except String Value := do
  let value ← match operand.atom with
    | .constant value => .ok (.scalar value)
    | .argument index => required frame.arguments[index]? "missing MIR argument"
    | .local id => required ((frame.locals.find? (·.1 == id)).map Prod.snd) "undefined MIR local"
    | .undefined => .ok .undefined
  if !value.fits 34 operand.ty then .error "MIR operand type mismatch" else return value

def read (frame : Frame) (operand : Operand) : Except String Word := do
  (← readValue frame operand).word

def put (locals : List (Nat × α)) (id : Nat) (value : α) : List (Nat × α) :=
  (id,value) :: locals.filter (·.1 != id)

def enter (predecessor : Option Nat) (phis : List Phi) (frame : Frame) : Except String Frame := do
  -- Read every incoming value from the old environment before assigning any phi.
  let values ← phis.mapM fun phi => do
    let some predecessor := predecessor | .error "phi at MIR entry"
    let incoming := phi.incoming.filter (·.1 == predecessor)
    let [(_,operand)] := incoming | .error "ambiguous or missing MIR phi edge"
    let value ← readValue frame operand
    if !(phi.ty.same operand.ty) then .error "MIR phi type mismatch" else return (phi.result,value)
  return {frame with locals := values.foldl (fun locals (id,value) => put locals id value) frame.locals}

def failureData : RevertData → List Byte
  | .empty => []
  | .panic code => [0x4e,0x48,0x7b,0x71] ++ wordBytes code

end SolarTV.MIR.State
