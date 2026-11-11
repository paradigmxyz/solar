import SolarTV.EVMIR.Precompiles.Pairing

/-! EIP-196/197 BN254 interfaces. ADD/MUL right-pad short inputs and ignore excess
bytes. Pairing requires an exact multiple of 192 bytes and validates G2 subgroup
membership. Wire encoding puts the imaginary Fp2 coefficient first, unlike the
internal (real,imaginary) representation. Infinity is encoded as all-zero bytes.
https://eips.ethereum.org/EIPS/eip-196
https://eips.ethereum.org/EIPS/eip-197 -/

namespace SolarTV.Precompiles.BN254
open EVM.State

def p : Nat := 21888242871839275222246405745257275088696311157297823662689037894645226208583
def order : Nat := 21888242871839275222246405745257275088548364400416034343698204186575808495617
def f := fp p
def f2 := fp2 p
def decode1 (input : List Byte) (offset : Nat) : Option (Point Nat) :=
  let x := readBE input offset 32; let y := readBE input (offset+32) 32
  if x ≥ p || y ≥ p then none
  else if x = 0 && y = 0 then some (infinity f)
  else let point := affine f x y
    if onCurve f 0 3 point then some point else none
def decode2 (input : List Byte) (offset : Nat) : Option (Point F2) :=
  let xi := readBE input offset 32; let xr := readBE input (offset+32) 32
  let yi := readBE input (offset+64) 32; let yr := readBE input (offset+96) 32
  if xi ≥ p || xr ≥ p || yi ≥ p || yr ≥ p then none
  else if xi = 0 && xr = 0 && yi = 0 && yr = 0 then some (infinity f2)
  else
    let point := affine f2 (xr,xi) (yr,yi)
    if onCurve f2 f2.zero (f2.div (3,0) (9,1)) point &&
        isInfinity f2 (multiply f2 f2.zero point order) then some point else none
def encode (point : Point Nat) : List Byte :=
  match normalize f point with
  | none => List.replicate 64 0
  | some (x,y) => be 32 x ++ be 32 y
def addition (input : List Byte) : Option (List Byte) := do
  let a ← decode1 input 0
  let b ← decode1 input 64
  return encode (add f 0 a b)
def multiplication (input : List Byte) : Option (List Byte) := do
  let a ← decode1 input 0
  return encode (multiply f 0 a (readBE input 64 32))
def pairing (input : List Byte) : Option (List Byte) := do
  if input.length%192 != 0 then none else
    let pairs ← (List.range (input.length/192)).mapM fun i => do
      let a ← decode1 input (192*i)
      let b ← decode2 input (192*i+64)
      return (a,b)
    return be 32 (if Pairing.check p order false pairs then 1 else 0)

end SolarTV.Precompiles.BN254
