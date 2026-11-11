import EvmRules.Word
import SolarTV.Outcome

/-! Pure typed MIR operations and ordered switch dispatch.
A case stores an SSA value ID. Reading from an arbitrary total environment models
defined MIR operands, including nonconstant cases. The interpreter tests them in
source order and enters the first matching successor from the current block. -/

namespace SolarTV.MIR

open EvmRules

inductive Op where
  | add | sub | mul | and | or | xor

/-- Semantic arithmetic wraps at the operand width, as in lower-integers. -/
def eval {width : Nat} : Op → BitVec width → BitVec width → BitVec width
  | .add => fun a b => a + b
  | .sub => fun a b => a - b
  | .mul => fun a b => a * b
  | .and => fun a b => a &&& b
  | .or => fun a b => a ||| b
  | .xor => fun a b => a ^^^ b

/-- Ordered switch selection, also used when compiling a pair obligation. -/
abbrev choose {width : Nat} (key test : BitVec width) (yes no : α) := if test = key then yes else no

abbrev Case := Nat × Nat -- (SSA value ID, destination block)

def dispatch (read : Nat → EvmRules.Word) (key : EvmRules.Word) (fallback : Nat) :
    List Case → Nat
  | [] => fallback
  | (id, dst) :: rest => choose key (read id) dst (dispatch read key fallback rest)

/-- Phi operands are read in the incoming environment and selected by predecessor.
An absent predecessor is undefined; the importer requires exactly one match. -/
def phi {width : Nat} (predecessor : Nat) : List (Nat × BitVec width) → Option (BitVec width)
  | [] => none
  | (block, value) :: rest =>
    if predecessor = block then some value else phi predecessor rest

inductive Cast where
  | trunc | zext | sext

/-- Cast widths are checked by the compiler verifier before import.
Truncation keeps the low bits, including for i1. Extension preserves either the
unsigned value or the source width's sign bit. Neither cast tests for nonzero. -/
def cast {sourceWidth : Nat} (kind : Cast) (to : Nat) (value : BitVec sourceWidth) : BitVec to :=
  match kind with
  | .trunc | .zext => value.setWidth to
  | .sext => value.signExtend to

inductive Comparison where
  | eq | ne | lt | gt | slt | sgt

/-- Comparisons produce exactly 0 or 1. Signed comparisons use the operand
width's sign bit, matching the sign extensions materialized by lower-integers. -/
def compare {width : Nat} (kind : Comparison) (a b : BitVec width) : BitVec 1 :=
  let condition := match kind with
    | .eq => a == b
    | .ne => a != b
    | .lt => a.ult b
    | .gt => b.ult a
    | .slt => a.slt b
    | .sgt => b.slt a
  if condition then 1 else 0

/-- jumpi takes a normalized i1, so its only possible inputs are 0 and 1. -/
abbrev branch (condition : BitVec 1) (yes no : α) := if condition = 1 then yes else no

inductive CheckedOp where
  | add | sub | mul

/-- Lean's overflow predicates check the unbounded result against the declared
unsigned or two's-complement range. A failure is the Panic(0x11) emitted by
lower-arithmetic; successful results retain the declared width. -/
def checked {width : Nat} (op : CheckedOp) (signed : Bool)
    (a b : BitVec width) : Outcome (BitVec width) :=
  let overflow := match op with
    | .add => if signed then a.saddOverflow b else a.uaddOverflow b
    | .sub => if signed then a.ssubOverflow b else a.usubOverflow b
    | .mul => if signed then a.smulOverflow b else a.umulOverflow b
  if overflow then .reverted (.panic 0x11)
  else .returned (match op with
    | .add => a + b
    | .sub => a - b
    | .mul => a * b)

/-- Semantic checks fail synchronously before any later instruction runs. -/
def guard (fails : Bool) (failure : RevertData) (next : Outcome α) : Outcome α :=
  if fails then .reverted failure else next

end SolarTV.MIR
