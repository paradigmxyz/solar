import SolarTV.EVMIR.Precompiles.SHA256
import SolarTV.EVMIR.Precompiles.RIPEMD160
import SolarTV.EVMIR.Precompiles.Blake2F
import SolarTV.EVMIR.Precompiles.Signatures
import SolarTV.EVMIR.Precompiles.BN254
import SolarTV.EVMIR.Precompiles.Maps
import SolarTV.EVMIR.Precompiles.KZG
import SolarTV.EVMIR.Precompiles.Gas
import SolarTV.EVMIR.State.Decode

/-! Complete precompile dispatch for the world's admitted Istanbul–Osaka forks.
Activation is shared with the bytecode evaluator; inactive addresses execute
ordinary account code. Invalid encodings/proofs burn the child budget, whereas
invalid ECDSA signatures return successful empty output. Gas is charged before
expensive crypto. Explicit model work limits are errors, never EVM failures.
Every cryptographic result is computed in Lean from executable arithmetic.
https://eips.ethereum.org/EIPS/eip-7823 -/

namespace SolarTV.Precompiles
open EVM.State

def modexp (input : List Byte) : Except String (List Byte) := do
  let base := readBE input 0 32
  let exponent := readBE input 32 32
  let modulus := readBE input 64 32
  if base = 0 && modulus = 0 then return []
  if base > 65536 || exponent > 65536 || modulus > 65536 then
    .error "ModExp model byte limit of 64 KiB"
  else
    let value := powMod (readBE input 96 base) (readBE input (96+base) exponent)
      (readBE input (96+base+exponent) modulus)
    return be modulus value

def compute (address : Nat) (input : List Byte) : Except String (Option (List Byte)) :=
  match address with
  | 1 => .ok (some (Signatures.recover input))
  | 2 => .ok (some (SHA256.hash input))
  | 3 => .ok (some (List.replicate 12 0 ++ RIPEMD160.hash input))
  | 4 => .ok (some input)
  | 5 => some <$> modexp input
  | 6 => .ok (BN254.addition input)
  | 7 => .ok (BN254.multiplication input)
  | 8 => .ok (BN254.pairing input)
  | 9 =>
    if input.length != 213 || input[212]?.getD 0 > 1 then .ok none
    else if readBE input 0 4 > 4096 then .error "BLAKE2F model limit of 4096 rounds"
    else .ok (some (Blake2F.compress input))
  | 10 => .ok (KZG.evaluate input)
  | 11 => .ok (BLS12381.addition1 input)
  | 12 => .ok (BLS12381.msm1 input)
  | 13 => .ok (BLS12381.addition2 input)
  | 14 => .ok (BLS12381.msm2 input)
  | 15 => .ok (BLS12381.pairing input)
  | 16 => .ok (BLS12381.map1 input)
  | 17 => .ok (BLS12381.map2 input)
  | 256 => .ok (some (Signatures.verify input))
  | _ => .error "unrecognized active precompile"

def execute (fork : Fork) (address : Address) (input : List Byte) (gas : Nat) :
    Except String (Stop × List Byte × Nat) := do
  if fork = .amsterdam then .error "Amsterdam precompile pricing is not modeled" else
  if !precompile fork address then .error "inactive precompile must execute account code" else
    -- EIP-152 validates the compression input length before reading rounds.
    if address = 9 && input.length != 213 then return (.exception,[],0)
    -- MSM/pairing validate their repeated-record layout before charging gas.
    if (address = 12 && (input.isEmpty || input.length%160 != 0)) ||
        (address = 14 && (input.isEmpty || input.length%288 != 0)) ||
        (address = 15 && (input.isEmpty || input.length%384 != 0)) then
      return (.exception,[],0)
    if address = 5 && fork.atLeast .osaka && gas < 500 then return (.outOfGas,[],0)
    let cost := price fork address.toNat input
    if address = 5 && fork.atLeast .osaka &&
        (readBE input 0 32 > 1024 || readBE input 32 32 > 1024 || readBE input 64 32 > 1024) then
      return (.exception,[],0)
    if gas < cost then return (.outOfGas,[],0)
    let output ← compute address.toNat input
    match output with
    | none => return (.exception,[],0)
    | some bytes => return (.success,bytes,gas-cost)

end SolarTV.Precompiles
