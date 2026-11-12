import EvmRules.Word
import SolarTV.Outcome
import SolarTV.Word

/-! Pure typed MIR operations and ordered switch dispatch.
A case stores an SSA value ID. Reading from an arbitrary total environment models
defined MIR operands, including nonconstant cases. The interpreter tests them in
source order and enters the first matching successor from the current block. -/

namespace SolarTV.MIR

open EvmRules

inductive Op where
  | add | sub | mul | div | sdiv | mod | smod | exp | and | or | xor
  | shl | shr | sar

/-- Semantic arithmetic wraps at the operand width, as in lower-integers. Division and
remainder by zero give zero, as the EVM opcodes do. Shifts take the amount first, in EVM
pop order, and shift out every bit once it reaches the width; an arithmetic shift then
fills with the sign bit. The signed operations read the sign bit at the operand width. -/
def eval {width : Nat} : Op → BitVec width → BitVec width → BitVec width
  | .add => fun a b => a + b
  | .sub => fun a b => a - b
  | .mul => fun a b => a * b
  | .div => Word.binary .div
  | .sdiv => Word.binary .sdiv
  | .mod => Word.binary .mod
  | .smod => Word.binary .smod
  | .exp => Word.binary .exp
  | .and => fun a b => a &&& b
  | .or => fun a b => a ||| b
  | .xor => fun a b => a ^^^ b
  | .shl => fun shift value => value <<< shift.toNat
  | .shr => fun shift value => value >>> shift.toNat
  | .sar => fun shift value => value.sshiftRight shift.toNat

/-- Bitwise complement at the operand width. -/
def not {width : Nat} (a : BitVec width) : BitVec width := ~~~a

abbrev unary := @Word.unary
abbrev shift := @Word.shift

/-- MIR `select` takes a normalized i1: the first arm when it is 1. -/
def select {width : Nat} (condition : BitVec 1) (yes no : BitVec width) : BitVec width :=
  if condition = 0 then no else yes

inductive WordBinary where
  | byte | signextend

def wordBinary : WordBinary → EvmRules.Word → EvmRules.Word → EvmRules.Word
  | .byte => Evm.byte
  | .signextend => Evm.signextend

inductive Ternary where
  | addmod | mulmod

def ternary : Ternary → EvmRules.Word → EvmRules.Word → EvmRules.Word → EvmRules.Word
  | .addmod => Evm.addmod
  | .mulmod => Evm.mulmod

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
  | add | sub | mul | div | wrappingDiv | rem

/-- Lean's overflow predicates check the unbounded result against the declared
unsigned or two's-complement range. A failure is the Panic(0x11) emitted by
lower-arithmetic; successful results retain the declared width. -/
def checked {width : Nat} (op : CheckedOp) (signed : Bool)
    (a b : BitVec width) : Outcome (BitVec width) :=
  match op with
  | .add => if (if signed then a.saddOverflow b else a.uaddOverflow b) then
      .reverted (.panic 0x11) else .returned (a + b)
  | .sub => if (if signed then a.ssubOverflow b else a.usubOverflow b) then
      .reverted (.panic 0x11) else .returned (a - b)
  | .mul => if (if signed then a.smulOverflow b else a.umulOverflow b) then
      .reverted (.panic 0x11) else .returned (a * b)
  | .div =>
    if b == 0 then .reverted (.panic 0x12)
    else if signed && a == BitVec.intMin width && b == -1 then .reverted (.panic 0x11)
    else .returned (if signed then a.sdiv b else a / b)
  | .wrappingDiv =>
    if b == 0 then .reverted (.panic 0x12)
    else .returned (if signed then a.sdiv b else a / b)
  | .rem =>
    if b == 0 then .reverted (.panic 0x12)
    else .returned (if signed then Word.binary .smod a b else Word.binary .mod a b)

def checkedPower {width : Nat} (signed : Bool) (base : BitVec width)
    (exponent : Nat) : Outcome (BitVec width) :=
  if _h : exponent = 0 then .returned 1
  else if exponent = 1 then .returned base
  else
    Outcome.bind (checked .mul signed base base) fun square =>
      Outcome.bind (checkedPower signed square (exponent / 2)) fun half =>
        if exponent % 2 = 0 then .returned half else checked .mul signed half base
termination_by exponent
decreasing_by exact Nat.div_lt_self (Nat.pos_of_ne_zero _h) (by decide)

/-- Semantic checks fail synchronously before any later instruction runs. -/
def guard (fails : Bool) (failure : RevertData) (next : Outcome α) : Outcome α :=
  if fails then .reverted failure else next

end SolarTV.MIR
