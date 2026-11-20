import SolarTV.EVMIR.State.Types

/-! CREATE and CREATE2 address derivation, collision and deployment rules.
RLP is encoded directly for the creator's twenty-byte address and a 64-bit nonce.
Initcode is executed by the same nested evaluator as ordinary calls. A creation
checkpoint is taken after the creator's nonce bump and destination warming, so
constructor rollback does not undo either caller-side effect.
https://eips.ethereum.org/EIPS/eip-684
https://eips.ethereum.org/EIPS/eip-1014
https://eips.ethereum.org/EIPS/eip-2681
https://eips.ethereum.org/EIPS/eip-3860 -/

namespace SolarTV.EVM.State
open EvmRules

def addressBytes (address : Address) : List Byte := (wordBytes (address.setWidth 256)).drop 12

def nonceBytes (nonce : Nat) : List Byte :=
  ((wordBytes (BitVec.ofNat 256 nonce)).dropWhile (fun b => b == 0))

def encodedNonce (nonce : Nat) : List Byte :=
  if nonce = 0 then [0x80]
  else if nonce < 128 then [BitVec.ofNat 8 nonce]
  else let bytes := nonceBytes nonce
       BitVec.ofNat 8 (128 + bytes.length) :: bytes

def createAddress (creator : Address) (nonce : Nat) : Address :=
  let payload := [0x94] ++ addressBytes creator ++ encodedNonce nonce
  (Keccak.hash (BitVec.ofNat 8 (192 + payload.length) :: payload)).setWidth 160

def create2Address (creator : Address) (salt : Word) (initcode : List Byte) : Address :=
  (Keccak.hash ([0xff] ++ addressBytes creator ++ wordBytes salt ++ wordBytes (Keccak.hash initcode))).setWidth 160

def collision (world : Journal) (address : Address) : Bool :=
  let old := account world address
  old.nonce != 0 || !old.code.isEmpty

def validDeployedCode (fork : Fork) (code : List Byte) : Bool :=
  (!fork.atLeast .spuriousDragon || code.length ≤ 24576) &&
    (!fork.atLeast .london || code[0]?.getD 0 != 0xef)

end SolarTV.EVM.State
