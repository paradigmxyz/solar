import SolarTV.EVMIR.Precompiles.Curve

/-! ECRECOVER and EIP-7951 P256VERIFY. Both validate their scalar and point
encodings before curve arithmetic. ECRECOVER accepts high s and right-pads its
128-byte input; invalid recovery succeeds with empty output. P256VERIFY requires
exactly 160 bytes and also returns empty output for an invalid signature.
https://eips.ethereum.org/EIPS/eip-7951 -/

namespace SolarTV.Precompiles.Signatures
open EVM.State

def k1p : Nat := 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffc2f
def k1n : Nat := 0xfffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141
def k1g : Point Nat := affine (fp k1p)
  0x79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798
  0x483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8

def recover (input : List Byte) : List Byte := Id.run do
  let hash := readBE input 0 32
  let v := readBE input 32 32
  let r := readBE input 64 32
  let s := readBE input 96 32
  if (v != 27 && v != 28) || r = 0 || r ≥ k1n || s = 0 || s ≥ k1n then return []
  let f := fp k1p
  let some y := sqrt k1p (f.add (f.mul (f.square r) r) 7) | return []
  let y := if y%2 = v-27 then y else k1p-y
  let point := affine f r y
  let combined := add f 0 (multiply f 0 point s) (multiply f 0 k1g (k1n - hash%k1n))
  let some (x,y) := normalize f (multiply f 0 combined (powMod r (k1n-2) k1n)) | return []
  let digest := Keccak.hash (be 32 x ++ be 32 y)
  return be 32 ((digest.setWidth 160).toNat)

def p256p : Nat := 0xffffffff00000001000000000000000000000000ffffffffffffffffffffffff
def p256n : Nat := 0xffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551
def p256a : Nat := p256p-3
def p256b : Nat := 0x5ac635d8aa3a93e7b3ebbd55769886bc651d06b0cc53b0f63bce3c3e27d2604b
def p256g : Point Nat := affine (fp p256p)
  0x6b17d1f2e12c4247f8bce6e563a440f277037d812deb33a0f4a13945d898c296
  0x4fe342e2fe1a7f9b8ee7eb4a7c0f9e162bce33576b315ececbb6406837bf51f5

def verify (input : List Byte) : List Byte := Id.run do
  if input.length != 160 then return []
  let hash := readBE input 0 32
  let r := readBE input 32 32; let s := readBE input 64 32
  let x := readBE input 96 32; let y := readBE input 128 32
  if r = 0 || s = 0 || r ≥ p256n || s ≥ p256n || x ≥ p256p || y ≥ p256p then return []
  let f := fp p256p
  let q := affine f x y
  if !onCurve f p256a p256b q then return []
  let inverse := powMod s (p256n-2) p256n
  let point := add f p256a (multiply f p256a p256g (hash*inverse%p256n))
    (multiply f p256a q (r*inverse%p256n))
  let some (x,_) := normalize f point | return []
  return if x%p256n = r then be 32 1 else []

end SolarTV.Precompiles.Signatures
