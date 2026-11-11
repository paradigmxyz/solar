import SolarTV.EVMIR.Precompiles.BLS12381
import SolarTV.EVMIR.Precompiles.MapConstants

/-! EIP-2537's simplified SWU maps, rational isogenies and effective-cofactor
clearing. Exceptional SWU inputs use B/(Z*A); zero isogeny denominators map to
infinity. The output sign follows sgn0 (parity of the first nonzero coefficient),
not the lexicographic sign used by compressed KZG point encodings.
https://eips.ethereum.org/assets/eip-2537/field_to_curve -/

namespace SolarTV.Precompiles.BLS12381
open EVM.State MapConstants

def polynomialValue (f : Arithmetic K) (coefficients : List K) (x : K) : K :=
  coefficients.reverse.foldl (fun acc c => f.add (f.mul acc x) c) f.zero

def swu (f : Arithmetic K) (a b z : K) (root : K → Option K)
    (sign : K → Bool) (u : K) : Option (Point K) := do
  let zu2 := f.mul z (f.square u)
  let inverse := f.inv (f.add (f.square zu2) zu2)
  let x1 := if f.eq inverse f.zero then f.div b (f.mul z a)
    else f.mul (f.div (f.neg b) a) (f.add f.one inverse)
  let equation := fun x => f.add (f.add (f.mul (f.square x) x) (f.mul a x)) b
  let (x,y) ← match root (equation x1) with
    | some y => some (x1,y)
    | none => let x2 := f.mul zu2 x1; (fun y => (x2,y)) <$> root (equation x2)
  return affine f x (if sign u == sign y then y else f.neg y)

def isogeny (f : Arithmetic K) (xn xd yn yd : List K) (point : Point K) : Point K :=
  match normalize f point with
  | none => infinity f
  | some (x,y) =>
    let dx := polynomialValue f xd x; let dy := polynomialValue f yd x
    if f.eq dx f.zero || f.eq dy f.zero then infinity f else
      affine f (f.div (polynomialValue f xn x) dx)
        (f.div (f.mul y (polynomialValue f yn x)) dy)

def map1 (input : List Byte) : Option (List Byte) := do
  if input.length != 64 then none else
    let u := readBE input 0 64
    if u ≥ p then none else
      let point ← swu f
        0x144698a3b8e9433d693a02c96d4982b0ea985383ee66a8d8e8981aefd881ac98936f8da0e0f97f5cf428082d584c1d
        0x12e2908d11688030018b12e8753eee3b2016c1f0f24f4070a0b9c14fcef35ef55a23215a316ceaa5d1cc48e98e172be0
        11 (sqrt p) (fun n => n%2 == 1) u
      let mapped := isogeny f g1XNumerator g1XDenominator g1YNumerator g1YDenominator point
      return encode1 (multiply f 0 mapped 0xd201000000010001)

def map2 (input : List Byte) : Option (List Byte) := do
  if input.length != 128 then none else
    let a := readBE input 0 64; let b := readBE input 64 64
    if a ≥ p || b ≥ p then none else
      let point ← swu f2 (0,240) (1012,1012) (p-2,p-1) sqrt2
        (fun n => n.1%2 == 1 || (n.1 == 0 && n.2%2 == 1)) (a,b)
      let mapped := isogeny f2 g2XNumerator g2XDenominator g2YNumerator g2YDenominator point
      let cofactor := 0xbc69f08f2ee75b3584c6a0ea91b352888e2a8e9145ad7689986ff031508ffe1329c2f178731db956d82bf015d1212b02ec0ec69d7477c1ae954cbc06689f6a359894c0adebbf6b4e8020005aaa95551
      return encode2 (multiply f2 f2.zero mapped cofactor)

end SolarTV.Precompiles.BLS12381
