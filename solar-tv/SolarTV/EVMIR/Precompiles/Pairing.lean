import SolarTV.EVMIR.Precompiles.Curve

/-! Optimal-Ate pairing products over explicit degree-12 polynomial fields.
Miller lines retain numerator/denominator pairs, avoiding an inversion per line.
Final exponentiation compares their powers, not an unchecked foreign result.
BN254 includes its two Frobenius correction lines. BLS12-381 uses the absolute
loop parameter: inversion of the whole product for the negative parameter does
not change the product-equals-one predicate exposed by either precompile.

The homogeneous line and group formulas follow EVMYulLean's alt_bn128 reference
and the pairing definitions in EIP-197 and EIP-2537. Infinity pairs contribute
one. Decoders must enforce curve and subgroup membership before calling here.
https://eips.ethereum.org/EIPS/eip-197
https://eips.ethereum.org/EIPS/eip-2537 -/

namespace SolarTV.Precompiles.Pairing

def line (f : Arithmetic F12) (p q t : Point F12) : F12 × F12 :=
  let numerator := f.sub (f.mul q.y p.z) (f.mul p.y q.z)
  let denominator := f.sub (f.mul q.x p.z) (f.mul p.x q.z)
  let dx := f.sub (f.mul t.x p.z) (f.mul p.x t.z)
  let dy := f.sub (f.mul t.y p.z) (f.mul p.y t.z)
  if !f.eq denominator f.zero then
    (f.sub (f.mul numerator dx) (f.mul denominator dy), f.mul denominator (f.mul t.z p.z))
  else if f.eq numerator f.zero then
    let numerator := f.mul (f.nat 3) (f.square p.x)
    let denominator := f.mul (f.nat 2) (f.mul p.y p.z)
    (f.sub (f.mul numerator dx) (f.mul denominator dy), f.mul denominator (f.mul t.z p.z))
  else (dx, f.mul p.z t.z)

def cast (f : Arithmetic F12) (p : Point Nat) : Point F12 :=
  ⟨f.nat p.x, f.nat p.y, f.nat p.z⟩

def twist (p : Nat) (bls : Bool) (q : Point F2) : Point F12 :=
  let f := fp12 p (if bls then 2 else 18) (if bls then 2 else 82)
  let w := polynomial 1 1
  let x := lift2 p (if bls then 1 else 9) q.x
  let y := lift2 p (if bls then 1 else 9) q.y
  let z := lift2 p (if bls then 1 else 9) q.z
  if bls then ⟨f.mul x w, y, f.mul z (f.pow w 3)⟩
  else ⟨f.mul x (f.pow w 2), f.mul y (f.pow w 3), z⟩

def miller (p : Nat) (bls : Bool) (g1 : Point Nat) (g2 : Point F2) : F12 × F12 := Id.run do
  let f := fp12 p (if bls then 2 else 18) (if bls then 2 else 82)
  if isInfinity (fp p) g1 || isInfinity (fp2 p) g2 then return (f.one,f.one)
  let q := twist p bls g2
  let t := cast f g1
  let count := if bls then 0xd201000000010000 else 29793968203157093288
  let mut r := q
  let mut numerator := f.one
  let mut denominator := f.one
  for j in List.range count.log2 do
    let bit := count.log2-1-j
    let l := line f r r t
    numerator := f.mul (f.square numerator) l.1
    denominator := f.mul (f.square denominator) l.2
    r := double f f.zero r
    if (count >>> bit)%2 = 1 then
      let l := line f r q t
      numerator := f.mul numerator l.1
      denominator := f.mul denominator l.2
      r := add f f.zero r q
  if !bls then
    let q1 := Point.mk (f.pow q.x p) (f.pow q.y p) (f.pow q.z p)
    let q2 := Point.mk (f.pow q1.x p) (f.neg (f.pow q1.y p)) (f.pow q1.z p)
    let l := line f r q1 t
    numerator := f.mul numerator l.1; denominator := f.mul denominator l.2
    r := add f f.zero r q1
    let l := line f r q2 t
    numerator := f.mul numerator l.1; denominator := f.mul denominator l.2
  return (numerator,denominator)

def check (p order : Nat) (bls : Bool) (pairs : List (Point Nat × Point F2)) : Bool :=
  let f := fp12 p (if bls then 2 else 18) (if bls then 2 else 82)
  let product := pairs.foldl (fun acc pair =>
    let (n,d) := miller p bls pair.1 pair.2
    (f.mul acc.1 n, f.mul acc.2 d)) (f.one,f.one)
  !f.eq product.2 f.zero &&
    (f.eq product.1 product.2 || f.eq (f.pow product.1 ((p^12-1)/order)) (f.pow product.2 ((p^12-1)/order)))

end SolarTV.Precompiles.Pairing
