import SolarTV.EVMIR.State.Types

/-! Byte encodings and bounded binary exponentiation shared by the precompiles.
Reads pad on the right, as the EVM precompile interfaces require; outputs have
an exact caller-specified width and preserve leading zero bytes. -/

namespace SolarTV.Precompiles
open EVM.State

def readBE (bytes : List Byte) (offset count : Nat) : Nat :=
  (readBytes bytes offset count).foldl (fun n b => n * 256 + b.toNat) 0

def readLE (bytes : List Byte) (offset count : Nat) : Nat :=
  (readBytes bytes offset count).reverse.foldl (fun n b => n * 256 + b.toNat) 0

def be (width n : Nat) : List Byte :=
  (List.range width).map (fun i => BitVec.ofNat 8 (n >>> (8 * (width - 1 - i))))

def le (width n : Nat) : List Byte := (be width n).reverse

def powLoop (modulus : Nat) : Nat → Nat → Nat → Nat → Nat
  | 0, _, _, acc => acc
  | fuel + 1, base, exponent, acc =>
    if exponent = 0 then acc else
      powLoop modulus fuel (base * base % modulus) (exponent / 2)
        (if exponent % 2 = 1 then acc * base % modulus else acc)

def powMod (base exponent modulus : Nat) : Nat :=
  if modulus = 0 then 0 else powLoop modulus (exponent.log2 + 1) (base % modulus) exponent (1 % modulus)

end SolarTV.Precompiles
