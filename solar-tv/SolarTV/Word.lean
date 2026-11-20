import EvmRules.Word

/-! Width-polymorphic scalar semantics shared by typed MIR and scheduled EVM IR.
Division by zero follows the EVM, shifts saturate instead of reducing modulo the
width, and exponentiation uses squaring so a word-sized exponent needs logarithmic
work. This file has no environment, heap or gas assumptions. -/

namespace SolarTV.Word

def power {width : Nat} (base : BitVec width) (exponent : Nat) : BitVec width :=
  if _h : exponent = 0 then 1
  else
    let half := power (base * base) (exponent / 2)
    if exponent % 2 = 0 then half else base * half
termination_by exponent
decreasing_by exact Nat.div_lt_self (Nat.pos_of_ne_zero _h) (by decide)

@[simp] theorem power_zero (base : BitVec width) : power base 0 = 1 := by simp [power]
@[simp] theorem power_one (base : BitVec width) : power base 1 = base := by simp [power]

inductive Binary where
  | add | sub | mul | div | sdiv | mod | smod | exp | and | or | xor

def binary {width : Nat} : Binary → BitVec width → BitVec width → BitVec width
  | .add => fun a b => a + b
  | .sub => fun a b => a - b
  | .mul => fun a b => a * b
  | .div => fun a b => a / b
  | .sdiv => fun a b => a.sdiv b
  | .mod => fun a b => if b = 0 then 0 else a % b
  | .smod => fun a b => if b = 0 then 0 else a.srem b
  | .exp => fun a b => power a b.toNat
  | .and => fun a b => a &&& b
  | .or => fun a b => a ||| b
  | .xor => fun a b => a ^^^ b

inductive Unary where
  | not | clz

def unary {width : Nat} : Unary → BitVec width → BitVec width
  | .not => fun a => ~~~a
  | .clz => fun a => a.clz

inductive Shift where
  | shl | shr | sar

/-- Arguments use MIR textual order: value, then shift. -/
def shift {width : Nat} (op : Shift) (value amount : BitVec width) : BitVec width :=
  if amount.toNat ≥ width then
    match op with | .shl | .shr => 0 | .sar => if value.msb then -1 else 0
  else match op with
    | .shl => value <<< amount.toNat
    | .shr => value >>> amount.toNat
    | .sar => value.sshiftRight amount.toNat

end SolarTV.Word
