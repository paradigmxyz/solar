import SolarTV.MIR.Ops
import Init.Data.List.Monadic
import SolarTV.State.ByteMemory
import SolarTV.State.Context

/-! Structured scalar SSA for direct MIR execution. The compiler's parser and
operation schema supply IDs, types and operation identities; the importer does
not lower MIR or synthesize backend addresses. Phis remain edge-selected and
simultaneous, and blocks retain their CFG, including cycles. -/

namespace SolarTV.MIR.State
open EvmRules SolarTV.Execution

inductive Atom where
  | constant (value : Word)
  | argument (index : Nat)
  | local (id : Nat)
  | undefined

structure Operand where
  atom : Atom
  width : Nat

inductive MemoryOp where
  | load | store | storeByte | copy | hash | size
  | calldataLoad | calldataSize | calldataCopy

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
  | environment (opcode : Nat)
  | log (topics : Nat)

structure Instruction where
  result : Option Nat
  width : Nat
  opcode : Option Nat
  operation : Operation
  operands : List Operand

structure Phi where
  result : Nat
  width : Nat
  incoming : List (Nat × Operand)

inductive Terminator where
  | jump (target : Nat)
  | branch (condition : Operand) (yes no : Nat)
  | switch (value : Operand) (fallback : Nat) (cases : List (Operand × Nat))
  | returned (values : List Operand)
  | returndata (offset size : Operand)
  | reverted (offset size : Operand)
  | stop | invalid

structure Block where
  phis : List Phi
  instructions : List Instruction
  terminator : Terminator

structure Config where
  fork : Fork
  environment : World.Environment
  context : Execution.Block
  available : List Nat
  entry : Bool

structure Frame where
  world : Journal
  memory : List Byte
  arguments : List Word
  locals : List (Nat × Word) := []

inductive Completion where
  | returned (values : List Word)
  | success (data : List Byte)
  | reverted (data : List Byte)
  | invalid

structure Result where
  completion : Completion
  world : Journal
  memory : List Byte

abbrev Run := Except String Result
abbrev Step := Except String (Outcome (Option Word × Frame))

def narrow (width : Nat) (value : Word) : Word :=
  (value.setWidth width).setWidth 256

@[simp] theorem narrow_full (value : Word) : narrow 256 value = value := by
  simp [narrow]

def required (value : Option α) (reason : String) : Except String α :=
  match value with | some value => .ok value | none => .error reason

def read (frame : Frame) (operand : Operand) : Except String Word := do
  if operand.width = 0 || operand.width > 256 then .error "unmodeled MIR operand type" else
    let value ← match operand.atom with
      | .constant value => .ok value
      | .argument index => required frame.arguments[index]? "missing MIR argument"
      | .local id => required ((frame.locals.find? (·.1 == id)).map Prod.snd) "undefined MIR local"
      | .undefined => .error "undefined MIR operand"
    if value != narrow operand.width value then .error "unclean typed MIR operand" else return value

def put (locals : List (Nat × Word)) (id : Nat) (value : Word) : List (Nat × Word) :=
  (id,value) :: locals.filter (·.1 != id)

def enter (predecessor : Option Nat) (phis : List Phi) (frame : Frame) : Except String Frame := do
  -- Read every incoming value from the old environment before assigning any phi.
  let values ← phis.mapM fun phi => do
    let some predecessor := predecessor | .error "phi at MIR entry"
    let incoming := phi.incoming.filter (·.1 == predecessor)
    let [(_,operand)] := incoming | .error "ambiguous or missing MIR phi edge"
    let value ← read frame operand
    if phi.width != operand.width then .error "MIR phi type mismatch" else return (phi.result,value)
  return {frame with locals := values.foldl (fun locals (id,value) => put locals id value) frame.locals}

def failureData : RevertData → List Byte
  | .empty => []
  | .panic code => [0x4e,0x48,0x7b,0x71] ++ wordBytes code

end SolarTV.MIR.State
