import SolarTV.EVMIR.State.Create

/-! Bytecode decoding for explicit-world execution. PUSH payload bytes are not
instructions or jump destinations. Availability and static prices are supplied
from the compiler's reflected opcode schema and bound into the proof input.
Pure instruction values use the separately tested word model. -/

namespace SolarTV.EVM.State
open EvmRules

def pureOp : Nat → Option EVM.Op
  | 0x01 => some .add | 0x02 => some .mul | 0x03 => some .sub
  | 0x04 => some .div | 0x05 => some .sdiv | 0x06 => some .mod | 0x07 => some .smod
  | 0x08 => some .addmod | 0x09 => some .mulmod | 0x0a => some .exp
  | 0x0b => some .signextend | 0x10 => some .lt | 0x11 => some .gt
  | 0x12 => some .slt | 0x13 => some .sgt | 0x14 => some .eq
  | 0x15 => some .iszero | 0x16 => some .and | 0x17 => some .or | 0x18 => some .xor
  | 0x19 => some .not | 0x1a => some .byte | 0x1b => some .shl | 0x1c => some .shr
  | 0x1d => some .sar | 0x1e => some .clz
  | _ => none

def jumpDestinations (fuel : Nat) (code : List Byte) (pc : Nat := 0) : List Nat :=
  match fuel with
  | 0 => []
  | fuel + 1 =>
    if pc ≥ code.length then [] else
      let byte := code[pc]?.getD 0 |>.toNat
      let next := pc + 1 + if 0x60 ≤ byte ∧ byte ≤ 0x7f then byte - 0x5f else 0
      let rest := jumpDestinations fuel code next
      if byte = 0x5b then pc :: rest else rest

def validJump (code : List Byte) (target : Word) : Bool :=
  (jumpDestinations code.length code).contains target.toNat

def pushValue (code : List Byte) (pc count : Nat) : Word :=
  (readBytes code (pc + 1) count).foldl (fun value byte => (value <<< 8) ||| byte.setWidth 256) 0

/-- Fork-specific precompile activation, shared by decoding and execution. -/
def precompile (fork : Fork) (address : Address) : Bool :=
  let n := address.toNat
  (1 ≤ n && n ≤ 4) ||
    (fork.atLeast .byzantium && 5 ≤ n && n ≤ 8) ||
    (fork.atLeast .istanbul && n = 9) ||
    (fork.atLeast .cancun && n = 10) ||
    (fork.atLeast .prague && 11 ≤ n && n ≤ 17) ||
    (fork.atLeast .osaka && n = 256)

def dead (world : Journal) (address : Address) : Bool :=
  let a := account world address
  a.nonce == 0 && a.balance == 0 && a.code.isEmpty

end SolarTV.EVM.State
