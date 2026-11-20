import SolarTV.EVMIR.Precompiles.Pairing

/-! Canonical encodings and arithmetic for EIP-2537. ADD checks curve membership
but intentionally does not require the prime-order subgroup; MSM and pairing do.
Fp elements occupy 64 bytes, scalars 32, and Fp2 coefficients are (real,imaginary).
Empty MSM/pairing inputs are invalid. Scalar multiplication accepts all 256 bits.
https://eips.ethereum.org/EIPS/eip-2537 -/

namespace SolarTV.Precompiles.BLS12381
open EVM.State

def p : Nat := 0x1a0111ea397fe69a4b1ba7b6434bacd764774b84f38512bf6730d2a0f6b0f6241eabfffeb153ffffb9feffffffffaaab
def order : Nat := 0x73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001
def f := fp p
def f2 := fp2 p
def generator1 : Point Nat := affine f
  3685416753713387016781088315183077757961620795782546409894578378688607592378376318836054947676345821548104185464507
  1339506544944476473020471379941921221584933875938349620426543736416511423956333506472724655353366534992391756441569
def generator2 : Point F2 := affine f2
  (352701069587466618187139116011060144890029952792775240219908644239793785735715026873347600343865175952761926303160,
   3059144344244213709971259814753781636986470325476647558659373206291635324768958432433509563104347017837885763365758)
  (1985150602287291935568054521177171638300868978215655730859378665066344726373823718423869104263333984641494340347905,
   927553665492332455747201965776037880757740193453592970025027978793976877002675564980949289727957565575433344219582)
def decode1 (input : List Byte) (offset : Nat) (subgroup : Bool) : Option (Point Nat) :=
  let x := readBE input offset 64; let y := readBE input (offset+64) 64
  if x ≥ p || y ≥ p then none
  else if x = 0 && y = 0 then some (infinity f)
  else let point := affine f x y
    if onCurve f 0 4 point && (!subgroup || isInfinity f (multiply f 0 point order)) then some point else none
def decode2 (input : List Byte) (offset : Nat) (subgroup : Bool) : Option (Point F2) :=
  let xr := readBE input offset 64; let xi := readBE input (offset+64) 64
  let yr := readBE input (offset+128) 64; let yi := readBE input (offset+192) 64
  if xi ≥ p || xr ≥ p || yi ≥ p || yr ≥ p then none
  else if xi = 0 && xr = 0 && yi = 0 && yr = 0 then some (infinity f2)
  else let point := affine f2 (xr,xi) (yr,yi)
    if onCurve f2 f2.zero (4,4) point &&
        (!subgroup || isInfinity f2 (multiply f2 f2.zero point order)) then some point else none
def encode1 (point : Point Nat) : List Byte :=
  match normalize f point with
  | none => List.replicate 128 0
  | some (x,y) => be 64 x ++ be 64 y
def encode2 (point : Point F2) : List Byte :=
  match normalize f2 point with
  | none => List.replicate 256 0
  | some (x,y) => be 64 x.1 ++ be 64 x.2 ++ be 64 y.1 ++ be 64 y.2
def addition1 (input : List Byte) : Option (List Byte) := do
  if input.length != 256 then none else
    let a ← decode1 input 0 false; let b ← decode1 input 128 false
    return encode1 (add f 0 a b)
def addition2 (input : List Byte) : Option (List Byte) := do
  if input.length != 512 then none else
    let a ← decode2 input 0 false; let b ← decode2 input 256 false
    return encode2 (add f2 f2.zero a b)
def msm1 (input : List Byte) : Option (List Byte) := do
  if input.isEmpty || input.length%160 != 0 then none else
    let result ← (List.range (input.length/160)).foldlM (fun acc i => do
      let point ← decode1 input (160*i) true
      return add f 0 acc (multiply f 0 point (readBE input (160*i+128) 32))) (infinity f)
    return encode1 result
def msm2 (input : List Byte) : Option (List Byte) := do
  if input.isEmpty || input.length%288 != 0 then none else
    let result ← (List.range (input.length/288)).foldlM (fun acc i => do
      let point ← decode2 input (288*i) true
      return add f2 f2.zero acc (multiply f2 f2.zero point (readBE input (288*i+256) 32))) (infinity f2)
    return encode2 result
def pairing (input : List Byte) : Option (List Byte) := do
  if input.isEmpty || input.length%384 != 0 then none else
    let pairs ← (List.range (input.length/384)).mapM fun i => do
      let a ← decode1 input (384*i) true
      let b ← decode2 input (384*i+128) true
      return (a,b)
    return be 32 (if Pairing.check p order true pairs then 1 else 0)

def sqrt2 (n : F2) : Option F2 := do
  if n.2 = 0 then
    match sqrt p n.1 with
    | some x => return (x,0)
    | none => let y ← sqrt p (f.neg n.1); return (0,y)
  else
    let norm ← sqrt p (f.add (f.square n.1) (f.square n.2))
    let half := (p+1)/2
    let x ← match sqrt p (f.mul (f.add n.1 norm) half) with
      | some x => some x
      | none => sqrt p (f.mul (f.sub n.1 norm) half)
    if x = 0 then none else
      let candidate := (x, f.div n.2 (f.mul 2 x))
      if f2.eq (f2.square candidate) n then some candidate else none

end SolarTV.Precompiles.BLS12381
