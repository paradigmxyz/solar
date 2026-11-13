import SolarTV.EVMIR.Precompiles.Field

/-! Short-Weierstrass arithmetic in homogeneous coordinates (x/z,y/z). Scalar
multiplication uses doubling and addition without intermediate inversions. The
point at infinity has z=0. Interface modules separately enforce canonical field
encodings, curve membership and the required subgroup checks. -/

namespace SolarTV.Precompiles

structure Point (K : Type) where
  x : K
  y : K
  z : K

def infinity (f : Arithmetic K) : Point K := ⟨f.one,f.one,f.zero⟩
def affine (f : Arithmetic K) (x y : K) : Point K := ⟨x,y,f.one⟩
def isInfinity (f : Arithmetic K) (p : Point K) : Bool := f.eq p.z f.zero
def negate (f : Arithmetic K) (p : Point K) : Point K := {p with y := f.neg p.y}
def normalize (f : Arithmetic K) (p : Point K) : Option (K × K) :=
  if isInfinity f p then none else
    let inverse := f.inv p.z
    some (f.mul p.x inverse, f.mul p.y inverse)
def onCurve (f : Arithmetic K) (a b : K) (p : Point K) : Bool :=
  isInfinity f p || f.eq (f.mul (f.square p.y) p.z)
    (f.add (f.add (f.mul (f.square p.x) p.x) (f.mul a (f.mul p.x (f.square p.z))))
      (f.mul b (f.mul (f.square p.z) p.z)))

def double (f : Arithmetic K) (a : K) (p : Point K) : Point K :=
  if isInfinity f p || f.eq p.y f.zero then infinity f else
    let w := f.add (f.mul (f.nat 3) (f.square p.x)) (f.mul a (f.square p.z))
    let s := f.mul p.y p.z
    let b := f.mul (f.mul p.x p.y) s
    let h := f.sub (f.square w) (f.mul (f.nat 8) b)
    ⟨f.mul (f.nat 2) (f.mul h s),
      f.sub (f.mul w (f.sub (f.mul (f.nat 4) b) h))
        (f.mul (f.nat 8) (f.mul (f.square p.y) (f.square s))),
      f.mul (f.nat 8) (f.mul s (f.square s))⟩

def add (f : Arithmetic K) (a : K) (p q : Point K) : Point K :=
  if isInfinity f p then q else if isInfinity f q then p else
    let u1 := f.mul q.y p.z; let u2 := f.mul p.y q.z
    let v1 := f.mul q.x p.z; let v2 := f.mul p.x q.z
    if f.eq v1 v2 then
      if f.eq u1 u2 then double f a p else infinity f
    else
      let u := f.sub u1 u2; let v := f.sub v1 v2
      let v2v2 := f.mul (f.square v) v2
      let v3 := f.mul (f.square v) v
      let z := f.mul p.z q.z
      let h := f.sub (f.sub (f.mul (f.square u) z) v3) (f.mul (f.nat 2) v2v2)
      ⟨f.mul v h, f.sub (f.mul u (f.sub v2v2 h)) (f.mul v3 u2), f.mul v3 z⟩

def multiplyLoop (f : Arithmetic K) (a : K) : Nat → Nat → Point K → Point K → Point K
  | 0, _, _, result => result
  | fuel + 1, scalar, point, result =>
    if scalar = 0 then result else
      multiplyLoop f a fuel (scalar/2) (double f a point)
        (if scalar%2 = 1 then add f a result point else result)
def multiply (f : Arithmetic K) (a : K) (point : Point K) (scalar : Nat) : Point K :=
  multiplyLoop f a (scalar.log2+1) scalar point (infinity f)

def sqrt (p n : Nat) : Option Nat :=
  let root := powMod n ((p+1)/4) p
  if root*root%p = n%p then some root else none

end SolarTV.Precompiles
