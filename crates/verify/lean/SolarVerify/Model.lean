import SolarVerify.Smt
/-!
Interpretations from finite models: a value for each constant, and for each function a table
of argument values with a default. Tests use them to evaluate exported terms in the kernel, and
they can replay a solver's counterexample. Nothing an obligation means depends on this file.
-/
namespace SolarVerify.Model
open SolarVerify.Smt

/-- An Int value. -/
def int (i : Int) : Val := ⟨.int, show Srt.Den .int from i⟩

/-- A Bool value. -/
def bool (b : Bool) : Val := ⟨.bool, show Srt.Den .bool from b⟩

/-- A bitvector value of width `w` and value `n mod 2^w`. -/
def bv (w n : Nat) : Val := ⟨.bv w, show Srt.Den (.bv w) from BitVec.ofNat w n⟩

/-- A record value. -/
def tup (fields : List Val) : Val := ⟨.tup (sortsOf fields), pack fields⟩

/-- A value at sort `s`, or the sort's default when it has another sort. -/
def valAt (s : Srt) (v : Val) : s.Den := (v.as s).getD s.default

/-- Whether two values are equal (arrays never compare equal here). -/
def same (a b : Val) : Bool :=
  match b.as a.1 with
  | some x => match a with
    | ⟨.array _ _, _⟩ => false
    | ⟨s, y⟩ => firstOrderEq s y x
  | none => false
where
  /-- Equality of first-order values, which the kernel reduces. -/
  firstOrderEq : (s : Srt) → s.Den → s.Den → Bool
    | .bool, a, b => @decide _ (Bool.decEq a b)
    | .int, a, b => @decide _ (Int.decEq a b)
    | .bv w, a, b => @decide _ ((inferInstance : DecidableEq (BitVec w)) a b)
    | .array _ _, _, _ => false
    | .tup fs, a, b => tupEq fs a b
  tupEq : (ss : List Srt) → Srt.DenList ss → Srt.DenList ss → Bool
    | [], _, _ => true
    | s :: ss, (a, as), (b, bs) => firstOrderEq s a b && tupEq ss as bs

/-- An array of sort `(Array idx elem)`: `entries` over the default `d`, the first entry for a
key winning. -/
def array (idx elem : Srt) (d : Val) (entries : List (Val × Val)) : Val :=
  ⟨.array idx elem, fun k =>
    match entries.find? (fun e => same e.1 ⟨idx, k⟩) with
    | some e => valAt elem e.2
    | none => valAt elem d⟩

/-- A function: the value of the first row whose arguments equal the call's, else `d`. -/
def table (d : Val) (rows : List (List Val × Val)) (args : List Val) : Val :=
  match rows.find? (fun r => r.1.length == args.length && (r.1.zip args).all (fun p => same p.1 p.2)) with
  | some r => r.2
  | none => d

/-- The interpretation giving symbol `f` the function `fns[f]`, and quotients and remainders by
zero the value 0. -/
def env (fns : List (List Val → Val)) : Env where
  fn f args ret xs := match fns[f]? with
    | some g => valAt ret (g (Srt.unpack args xs))
    | none => ret.default
  divZero _ := 0
  modZero _ := 0

end SolarVerify.Model
