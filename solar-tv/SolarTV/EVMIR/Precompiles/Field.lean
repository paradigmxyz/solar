import SolarTV.EVMIR.Precompiles.Bytes

/-! Executable finite-field arithmetic for the curve precompiles. Prime fields
use canonical natural residues; quadratic extensions use i² = -1. The degree-12
extension is a polynomial quotient, with coefficients in ascending order. Its
modulus is x¹² - c*x⁶ + d (BN254: c=18,d=82; BLS12-381: c=2,d=2).
All loops have explicit structural bounds. Inversion at zero is zero; callers
must check the nonzero/curve conditions required by their interface. -/

namespace SolarTV.Precompiles

structure Arithmetic (K : Type) where
  zero : K
  one : K
  nat : Nat → K
  add : K → K → K
  neg : K → K
  mul : K → K → K
  inv : K → K
  eq : K → K → Bool

def Arithmetic.sub (f : Arithmetic K) (a b : K) : K := f.add a (f.neg b)
def Arithmetic.square (f : Arithmetic K) (a : K) : K := f.mul a a
def Arithmetic.div (f : Arithmetic K) (a b : K) : K := f.mul a (f.inv b)

def powerLoop (mul : K → K → K) : Nat → K → Nat → K → K
  | 0, _, _, acc => acc
  | fuel + 1, base, exponent, acc =>
    if exponent = 0 then acc else
      powerLoop mul fuel (mul base base) (exponent / 2)
        (if exponent % 2 = 1 then mul acc base else acc)

def Arithmetic.pow (f : Arithmetic K) (a : K) (n : Nat) : K :=
  powerLoop f.mul (n.log2 + 1) a n f.one

def fp (p : Nat) : Arithmetic Nat :=
  ⟨0, 1, (· % p), (fun a b => (a+b)%p), (fun a => (p-a%p)%p),
    (fun a b => a*b%p), (fun a => powMod a (p-2) p), (· == ·)⟩

abbrev F2 := Nat × Nat

def fp2 (p : Nat) : Arithmetic F2 :=
  let f := fp p
  ⟨(0,0), (1,0), (fun n => (n%p,0)),
    (fun a b => (f.add a.1 b.1, f.add a.2 b.2)),
    (fun a => (f.neg a.1, f.neg a.2)),
    (fun a b => (f.sub (f.mul a.1 b.1) (f.mul a.2 b.2),
      f.add (f.mul a.1 b.2) (f.mul a.2 b.1))),
    (fun a => let d := f.inv (f.add (f.square a.1) (f.square a.2))
      (f.mul a.1 d, f.neg (f.mul a.2 d))), (· == ·)⟩

abbrev F12 := List Nat

def polynomial (index value : Nat) : F12 :=
  (List.range 12).map (fun i => if i = index then value else 0)

def mul12 (p c d : Nat) (a b : F12) : F12 := Id.run do
  let mut v := (List.range 23).map (fun k =>
    ((List.range 12).foldl (fun sum i =>
      if i ≤ k && k-i < 12 then sum + a[i]?.getD 0 * b[k-i]?.getD 0 else sum) 0) % p)
  for j in List.range 11 do
    let i := 22-j
    let high := v[i]?.getD 0
    v := v.set (i-6) ((v[i-6]?.getD 0 + c*high)%p)
    v := v.set (i-12) ((v[i-12]?.getD 0 + p - (d*high)%p)%p)
  return v.take 12

def fp12 (p c d : Nat) : Arithmetic F12 :=
  let mul := mul12 p c d
  let one := polynomial 0 1
  ⟨polynomial 0 0, one, (fun n => polynomial 0 (n%p)),
    (fun a b => (List.range 12).map (fun i => (a[i]?.getD 0 + b[i]?.getD 0)%p)),
    (fun a => a.map (fun x => (p-x%p)%p)), mul,
    (fun a => powerLoop mul ((p^12-2).log2+1) a (p^12-2) one), (· == ·)⟩

def lift2 (p c : Nat) (a : F2) : F12 :=
  (polynomial 0 ((a.1 + p - (c*a.2)%p)%p)).set 6 a.2

end SolarTV.Precompiles
