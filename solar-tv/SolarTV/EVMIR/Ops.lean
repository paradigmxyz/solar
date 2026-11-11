import EvmRules.Word
import SolarTV.Word

/-! Scheduled EVM IR word operations, bounded stacks and terminal outcomes.
Stack top is the head. The pure `step` gives operand semantics; `execute` adds
stack checks and preserves the first exception. Static transfers expose the
outgoing edge and live stack before the successor executes. Block references
stand for valid resolved destinations, independent of physical byte offsets. -/

namespace SolarTV.EVM

open EvmRules

inductive Op where
  | eq | sub | add | mul | and | or | xor | iszero | not
  | div | sdiv | mod | smod | exp | addmod | mulmod | signextend
  | lt | gt | slt | sgt | byte | shl | shr | sar | clz
  | calldatasize | returndatasize | push (value : Word)
  | pop | dup (index : Fin 256) | swap (index : Fin 256)
  | exchange (first second : Fin 32)

structure Context where
  calldataSize : Word
  returndataSize : Word

abbrev Stack := List Word

def step (ctx : Context) : Op → Stack → Option Stack
  | .add, a :: b :: rest => some (Evm.add a b :: rest)
  | .mul, a :: b :: rest => some (Evm.mul a b :: rest)
  | .and, a :: b :: rest => some (Evm.and a b :: rest)
  | .or, a :: b :: rest => some (Evm.or a b :: rest)
  | .xor, a :: b :: rest => some (Evm.xor a b :: rest)
  | .not, a :: rest => some (Evm.not a :: rest)
  | .div, a :: b :: rest => some (Evm.div a b :: rest)
  | .sdiv, a :: b :: rest => some (Evm.sdiv a b :: rest)
  | .mod, a :: b :: rest => some (Evm.mod a b :: rest)
  | .smod, a :: b :: rest => some (Evm.smod a b :: rest)
  | .exp, a :: b :: rest => some (Word.power a b.toNat :: rest)
  | .addmod, a :: b :: n :: rest => some (Evm.addmod a b n :: rest)
  | .mulmod, a :: b :: n :: rest => some (Evm.mulmod a b n :: rest)
  | .signextend, a :: b :: rest => some (Evm.signextend a b :: rest)
  | .byte, a :: b :: rest => some (Evm.byte a b :: rest)
  | .shl, a :: b :: rest => some (Evm.shl a b :: rest)
  | .shr, a :: b :: rest => some (Evm.shr a b :: rest)
  | .sar, a :: b :: rest => some (Evm.sar a b :: rest)
  | .lt, a :: b :: rest => some (Evm.lt a b :: rest)
  | .gt, a :: b :: rest => some (Evm.gt a b :: rest)
  | .slt, a :: b :: rest => some (Evm.slt a b :: rest)
  | .sgt, a :: b :: rest => some (Evm.sgt a b :: rest)
  | .clz, a :: rest => some (Evm.clz a :: rest)
  | .exchange first second, stack => do
    let a ← stack[first.val]?
    let b ← stack[second.val]?
    pure ((stack.set first.val b).set second.val a)
  | .pop, _ :: rest => some rest
  | .dup index, stack => (stack[index.val]?).map (fun value => value :: stack)
  | .swap index, a :: rest => (rest[index.val]?).map (fun value => value :: rest.set index.val a)
  | .eq, a :: b :: rest => some (Evm.eq a b :: rest)
  | .sub, a :: b :: rest => some (Evm.sub a b :: rest)
  | .iszero, a :: rest => some (Evm.iszero a :: rest)
  | .calldatasize, rest => some (ctx.calldataSize :: rest)
  | .returndatasize, rest => some (ctx.returndataSize :: rest)
  | .push value, rest => some (value :: rest)
  | _, _ => none

/-! The bounded machine follows EVMYulLean 047f630, Data/Stack.lean and
EVM/Semantics.lean:450-471. The stack head is the next operand, underflow is
checked before executing an opcode, and the resulting height cannot exceed 1024.
Gas and memory expansion remain outside this fragment. Return/revert terminals
are admitted only when the importer proves the output length is zero. -/

inductive Failure where
  | underflow | overflow | invalid
  deriving DecidableEq

inductive Execution where
  | edge (destination : Word) (stack : Stack)
  | success | reverted
  | failed (reason : Failure)
  deriving DecidableEq

inductive Terminal where
  | jump (destination : Word)
  | branch (destination fallthrough : Word)
  | branchStop (destination : Word)
  | stop | returnEmpty | revertEmpty | invalid

/-- Internal DUP/SWAP indices are zero-based; the importer accepts DUP1..16/SWAP1..16. -/
def stepChecked (ctx : Context) (op : Op) (stack : Stack) : Except Failure Stack :=
  if stack.length > 1024 then .error .overflow
  else match step ctx op stack with
    | none => .error .underflow
    | some next => if next.length > 1024 then .error .overflow else .ok next

/-- Static transfers include the label PUSH and its peak height. Labels are valid,
resolved references supplied by the parser. A conditional transfer consumes its
condition only after the label push succeeds. -/
def finish (terminal : Terminal) (stack : Stack) : Execution :=
  if stack.length > 1024 then .failed .overflow
  else match terminal with
    | .stop => .success
    | .invalid => .failed .invalid
    | .returnEmpty => match stack with
      | _ :: _ :: _ => .success
      | _ => .failed .underflow
    | .revertEmpty => match stack with
      | _ :: _ :: _ => .reverted
      | _ => .failed .underflow
    | .jump destination =>
      if stack.length + 1 > 1024 then .failed .overflow else .edge destination stack
    | .branch destination fallthrough =>
      if stack.length + 1 > 1024 then .failed .overflow
      else match stack with
        | condition :: rest => .edge (if condition = 0 then fallthrough else destination) rest
        | _ => .failed .underflow
    | .branchStop destination =>
      if stack.length + 1 > 1024 then .failed .overflow
      else match stack with
        | condition :: rest => if condition = 0 then .success else .edge destination rest
        | _ => .failed .underflow

/-- Execute one block window, preserving the first exception. Stack values are
observable at an outgoing edge; terminal halts discard the remaining stack. -/
def execute (ctx : Context) : List Op → Terminal → Stack → Execution
  | [], terminal, stack => finish terminal stack
  | op :: ops, terminal, stack => match stepChecked ctx op stack with
    | .error reason => .failed reason
    | .ok next => execute ctx ops terminal next

/-- Used only for solver witness discovery. Exact execution equality is replayed
in the kernel before a counterexample is reported. -/
def code : Execution → List Word
  | .edge destination stack => 0 :: destination :: stack
  | .success => [1]
  | .reverted => [3]
  | .failed .underflow => [4]
  | .failed .overflow => [5]
  | .failed .invalid => [6]

@[simp] theorem code_ite (condition : Prop) [Decidable condition] (yes no : Execution) :
    code (if condition then yes else no) = if condition then code yes else code no := by
  split <;> rfl

@[simp] theorem code_cond (condition : Bool) (yes no : Execution) :
    code (bif condition then yes else no) = bif condition then code yes else code no := by
  cases condition <;> rfl

def describe : Execution → String
  | .edge destination stack =>
    let edge := if destination = 0 then "fallthrough" else s!"bb{destination.toNat}"
    s!"{edge}, stack={stack.map BitVec.toNat}"
  | .success => "success(empty)"
  | .reverted => "revert(empty)"
  | .failed .underflow => "exception(stack-underflow)"
  | .failed .overflow => "exception(stack-overflow)"
  | .failed .invalid => "exception(invalid-instruction)"

end SolarTV.EVM
