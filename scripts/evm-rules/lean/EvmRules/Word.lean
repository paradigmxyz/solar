import Std.Tactic.BVDecide

/-!
# EVM word semantics

The meaning of every word operation the rule checker reads, transcribed from the
[Ethereum execution specifications](https://github.com/ethereum/execution-specs/tree/master/src/ethereum/forks/cancun/vm/instructions).
Words are `BitVec 256` with wrapping arithmetic. Operands are in MIR / EVM pop order, so
shifts take the amount before the value. Rule theorems are stated over these definitions,
and the checker's regression tests evaluate them against the independent integer evaluator
in `evm_rules/expr.py`. Memory, storage, calls, gas and control flow are outside this model.
-/

namespace EvmRules

/-- A 256-bit EVM word. -/
abbrev Word := BitVec 256

namespace Evm

def add (a b : Word) : Word := a + b

def sub (a b : Word) : Word := a - b

def mul (a b : Word) : Word := a * b

/-- `DIV`. Lean's unsigned division already returns zero for a zero divisor. -/
def div (a b : Word) : Word := a / b

/-- `MOD`: zero for a zero divisor, where Lean's `%` keeps the dividend. -/
def mod (a b : Word) : Word := bif b == 0 then 0 else a % b

/-- `SDIV`, truncating toward zero. `BitVec.sdiv` returns zero for a zero divisor, and
`-2²⁵⁵ / -1` wraps to `-2²⁵⁵`, both as the EVM does. -/
def sdiv (a b : Word) : Word := a.sdiv b

/-- `SMOD`: the remainder takes the dividend's sign; zero for a zero divisor. -/
def smod (a b : Word) : Word := bif b == 0 then 0 else a.srem b

/-- `ADDMOD`: the sum is not reduced modulo 2²⁵⁶ before the modulus. -/
def addmod (a b n : Word) : Word :=
  bif n == 0 then 0 else ((a.setWidth 512 + b.setWidth 512) % n.setWidth 512).setWidth 256

/-- `MULMOD`: the product is not reduced modulo 2²⁵⁶ before the modulus. -/
def mulmod (a b n : Word) : Word :=
  bif n == 0 then 0 else ((a.setWidth 512 * b.setWidth 512) % n.setWidth 512).setWidth 256

/-- `EXP`, modulo 2²⁵⁶. -/
def exp (a b : Word) : Word := a ^ b.toNat

def and (a b : Word) : Word := a &&& b

def or (a b : Word) : Word := a ||| b

def xor (a b : Word) : Word := a ^^^ b

def not (a : Word) : Word := ~~~a

/-- `SHL`: zero once the shift reaches the word width. -/
def shl (shift value : Word) : Word := bif shift.ult 256 then value <<< shift else 0

/-- `SHR`: zero once the shift reaches the word width. -/
def shr (shift value : Word) : Word := bif shift.ult 256 then value >>> shift else 0

/-- `SAR`: every bit takes the sign once the shift reaches the word width. -/
def sar (shift value : Word) : Word :=
  bif shift.ult 256 then value.sshiftRight' shift else bif value.msb then -1 else 0

def lt (a b : Word) : Word := bif a.ult b then 1 else 0

def gt (a b : Word) : Word := bif b.ult a then 1 else 0

def slt (a b : Word) : Word := bif a.slt b then 1 else 0

def sgt (a b : Word) : Word := bif b.slt a then 1 else 0

def eq (a b : Word) : Word := bif a == b then 1 else 0

/-- MIR `ne`, the word form of an inequality. -/
def ne (a b : Word) : Word := bif a == b then 0 else 1

def iszero (a : Word) : Word := bif a == 0 then 1 else 0

/-- MIR `select`: the first arm for every nonzero condition word. -/
def select (c a b : Word) : Word := bif c == 0 then b else a

/-- `BYTE`: byte `i` counted from the most significant end, and zero from 32 on. -/
def byte (i v : Word) : Word := bif i.ult 32 then (v >>> ((31 - i) * 8)) &&& 255 else 0

/-- `SIGNEXTEND`: extend the sign of the low `i + 1` bytes. Every index from 31 on is the
identity, however large. One case per byte keeps the definition free of variable shifts. -/
def signextend (i v : Word) : Word :=
  bif i == 0 then (v.setWidth 8).signExtend 256 else
  bif i == 1 then (v.setWidth 16).signExtend 256 else
  bif i == 2 then (v.setWidth 24).signExtend 256 else
  bif i == 3 then (v.setWidth 32).signExtend 256 else
  bif i == 4 then (v.setWidth 40).signExtend 256 else
  bif i == 5 then (v.setWidth 48).signExtend 256 else
  bif i == 6 then (v.setWidth 56).signExtend 256 else
  bif i == 7 then (v.setWidth 64).signExtend 256 else
  bif i == 8 then (v.setWidth 72).signExtend 256 else
  bif i == 9 then (v.setWidth 80).signExtend 256 else
  bif i == 10 then (v.setWidth 88).signExtend 256 else
  bif i == 11 then (v.setWidth 96).signExtend 256 else
  bif i == 12 then (v.setWidth 104).signExtend 256 else
  bif i == 13 then (v.setWidth 112).signExtend 256 else
  bif i == 14 then (v.setWidth 120).signExtend 256 else
  bif i == 15 then (v.setWidth 128).signExtend 256 else
  bif i == 16 then (v.setWidth 136).signExtend 256 else
  bif i == 17 then (v.setWidth 144).signExtend 256 else
  bif i == 18 then (v.setWidth 152).signExtend 256 else
  bif i == 19 then (v.setWidth 160).signExtend 256 else
  bif i == 20 then (v.setWidth 168).signExtend 256 else
  bif i == 21 then (v.setWidth 176).signExtend 256 else
  bif i == 22 then (v.setWidth 184).signExtend 256 else
  bif i == 23 then (v.setWidth 192).signExtend 256 else
  bif i == 24 then (v.setWidth 200).signExtend 256 else
  bif i == 25 then (v.setWidth 208).signExtend 256 else
  bif i == 26 then (v.setWidth 216).signExtend 256 else
  bif i == 27 then (v.setWidth 224).signExtend 256 else
  bif i == 28 then (v.setWidth 232).signExtend 256 else
  bif i == 29 then (v.setWidth 240).signExtend 256 else
  bif i == 30 then (v.setWidth 248).signExtend 256 else
  v

/-- `CLZ` (EIP-7939): the number of leading zero bits, 256 for zero. -/
def clz (v : Word) : Word := v.clz

/-- `ADDRESS`: the executing account, zero-extended. -/
def address (self : BitVec 160) : Word := self.setWidth 256

/-- `BALANCE` reads the snapshot balance of the account in the word's low 160 bits. -/
def balance (balances : BitVec 160 → Word) (account : Word) : Word :=
  balances (account.setWidth 160)

/-- `SELFBALANCE`: the snapshot balance of the executing account. -/
def selfbalance (balances : BitVec 160 → Word) (self : BitVec 160) : Word := balances self

end Evm

end EvmRules
