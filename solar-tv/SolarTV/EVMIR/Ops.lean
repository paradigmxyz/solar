import EvmRules.Word

/-! The scheduled EVM IR branch fragment. Stack top is the head.
Block references stand for valid resolved destinations. We compare the outgoing
edge and live stack for executions with sufficient gas, before either successor
executes. Fallthrough denotes a block, independent of its byte offset. -/

namespace SolarTV.EVM

open EvmRules

inductive Op where
  | eq | sub | iszero | calldatasize | returndatasize | push (value : Word)

structure Context where
  calldataSize : Word
  returndataSize : Word

abbrev Stack := List Word

def step (ctx : Context) : Op → Stack → Option Stack
  | .eq, a :: b :: rest => some (Evm.eq a b :: rest)
  | .sub, a :: b :: rest => some (Evm.sub a b :: rest)
  | .iszero, a :: rest => some (Evm.iszero a :: rest)
  | .calldatasize, rest => some (ctx.calldataSize :: rest)
  | .returndatasize, rest => some (ctx.returndataSize :: rest)
  | .push value, rest => some (value :: rest)
  | _, _ => none

/-- Execute the window, ending at JUMPI. Missing operands give stack underflow. -/
def run (ctx : Context) (fallthrough : Word) : List Op → Stack → Option (Word × Stack)
  | [], dst :: c :: rest => some ((if c = 0 then fallthrough else dst), rest)
  | [], _ => none
  | op :: ops, stack => (step ctx op stack).bind (run ctx fallthrough ops)

/-- Display an outgoing edge in the two-exit CLI region. -/
def describe : Option (Word × Stack) → String
  | none => "underflow"
  | some (dst, stack) =>
    let edge := if dst = 0 then "fallthrough" else "bb1"
    s!"{edge}, stack={stack.map BitVec.toNat}"

end SolarTV.EVM
