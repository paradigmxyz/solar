/-
The SMT-LIB fragment solar-verify writes, as a deep embedding with its denotation.

This file is the meaning of every verification condition `solar-verify --emit-lean` exports:
see docs/VERIFY_LEAN.md. Format version 1.

Sorts are SMT-LIB's `Bool`, `Int`, `(_ BitVec w)` and `(Array I E)`, and records: solar-verify
declares each struct as a datatype with one constructor and no recursion, which the export
writes as the product of its field sorts. Terms are untyped: `Term.eval` infers each sort and
returns `none` on an ill-sorted term.
An environment interprets every declared function symbol (constants are 0-ary symbols) and the
values SMT-LIB leaves unspecified: integer `div` and `mod` by zero.

Evaluation reduces in the kernel except at quantifiers and equality of arrays, which decide with
classical logic, so `decide` in the kernel evaluates any term free of them.
-/
namespace SolarVerify.Smt

/-- A sort. A record is the product of its field sorts. -/
inductive Srt where
  | bool
  | int
  | bv (w : Nat)
  | array (idx elem : Srt)
  | tup (fields : List Srt)
  deriving Repr, Inhabited

mutual
/-- Decidable equality of sorts, written out so that the kernel reduces it. -/
def Srt.decEq : (a b : Srt) → Decidable (a = b)
  | .bool, .bool => isTrue rfl
  | .int, .int => isTrue rfl
  | .bv w, .bv v =>
    if h : w = v then isTrue (h ▸ rfl) else isFalse (fun e => by injection e; contradiction)
  | .array i e, .array j f =>
    match Srt.decEq i j, Srt.decEq e f with
    | isTrue h1, isTrue h2 => isTrue (h1 ▸ h2 ▸ rfl)
    | isFalse h, _ => isFalse (fun x => by injection x; contradiction)
    | _, isFalse h => isFalse (fun x => by injection x; contradiction)
  | .tup fs, .tup gs =>
    match Srt.decEqList fs gs with
    | isTrue h => isTrue (h ▸ rfl)
    | isFalse h => isFalse (fun x => by injection x; contradiction)
  | .bool, .int | .bool, .bv _ | .bool, .array _ _ | .bool, .tup _
  | .int, .bool | .int, .bv _ | .int, .array _ _ | .int, .tup _
  | .bv _, .bool | .bv _, .int | .bv _, .array _ _ | .bv _, .tup _
  | .array _ _, .bool | .array _ _, .int | .array _ _, .bv _ | .array _ _, .tup _
  | .tup _, .bool | .tup _, .int | .tup _, .bv _ | .tup _, .array _ _ =>
    isFalse (fun h => by cases h)
def Srt.decEqList : (a b : List Srt) → Decidable (a = b)
  | [], [] => isTrue rfl
  | x :: xs, y :: ys =>
    match Srt.decEq x y, Srt.decEqList xs ys with
    | isTrue h1, isTrue h2 => isTrue (h1 ▸ h2 ▸ rfl)
    | isFalse h, _ => isFalse (fun e => by injection e; contradiction)
    | _, isFalse h => isFalse (fun e => by injection e; contradiction)
  | [], _ :: _ => isFalse (fun h => by cases h)
  | _ :: _, [] => isFalse (fun h => by cases h)
end

instance : DecidableEq Srt := Srt.decEq

mutual
/-- The values of a sort. An array is a total function, as in SMT-LIB's theory of arrays. -/
def Srt.Den : Srt → Type
  | .bool => Bool
  | .int => Int
  | .bv w => BitVec w
  | .array i e => Srt.Den i → Srt.Den e
  | .tup fs => Srt.DenList fs
/-- The values of a list of sorts, one each. -/
def Srt.DenList : List Srt → Type
  | [] => Unit
  | s :: ss => Srt.Den s × Srt.DenList ss
end

mutual
/-- A value of every sort, for building interpretations. -/
def Srt.default : (s : Srt) → s.Den
  | .bool => (false : Bool)
  | .int => (0 : Int)
  | .bv w => (0 : BitVec w)
  | .array _ e => fun _ => Srt.default e
  | .tup fs => Srt.defaultList fs
/-- A value of every list of sorts. -/
def Srt.defaultList : (ss : List Srt) → Srt.DenList ss
  | [] => ()
  | s :: ss => (Srt.default s, Srt.defaultList ss)
end

/-- A value with its sort. -/
abbrev Val := (s : Srt) × s.Den

mutual
/-- SMT-LIB equality of two values of one sort: structural on records, extensional (classical)
on arrays. -/
noncomputable def Srt.beq : (s : Srt) → s.Den → s.Den → Bool
  | .bool, a, b => @decide _ (Bool.decEq a b)
  | .int, a, b => @decide _ (Int.decEq a b)
  | .bv w, a, b => @decide _ ((inferInstance : DecidableEq (BitVec w)) a b)
  | .array i e, a, b =>
    @decide (∀ k : Srt.Den i, Srt.beq e (a k) (b k) = true) (Classical.propDecidable _)
  | .tup fs, a, b => Srt.beqList fs a b
noncomputable def Srt.beqList : (ss : List Srt) → Srt.DenList ss → Srt.DenList ss → Bool
  | [], _, _ => true
  | s :: ss, (a, as), (b, bs) => Srt.beq s a b && Srt.beqList ss as bs
end

/-- A value's view at a sort it may have. -/
def Val.as (v : Val) (s : Srt) : Option s.Den :=
  if h : v.1 = s then some (h ▸ v.2) else none

/-- A list of values as the values of a list of sorts. -/
def Val.asList : List Val → (ss : List Srt) → Option (Srt.DenList ss)
  | [], [] => some ()
  | v :: vs, s :: ss => do
    let x ← v.as s
    let xs ← Val.asList vs ss
    pure (x, xs)
  | _, _ => none

/-- The values of a record, with their sorts. -/
def Srt.unpack : (ss : List Srt) → Srt.DenList ss → List Val
  | [], () => []
  | s :: ss, (x, xs) => ⟨s, x⟩ :: Srt.unpack ss xs

/-- Field `i` of a record. -/
def Srt.field : (ss : List Srt) → Srt.DenList ss → Nat → Option Val
  | [], _, _ => none
  | s :: _, (x, _), 0 => some ⟨s, x⟩
  | _ :: ss, (_, xs), n + 1 => Srt.field ss xs n

/-- The bitvector a value holds, with its width. -/
def Val.bv? : Val → Option ((w : Nat) × BitVec w)
  | ⟨.bv w, x⟩ => some ⟨w, x⟩
  | _ => none

/-- The Bool a value holds. -/
def boolOf (o : Option Val) : Option Bool := o.bind (fun v => v.as .bool)

/-- The built-in operators, by their SMT-LIB names. The n-ary and chainable ones take as many
arguments as SMT-LIB allows. -/
inductive Op where
  -- Core.
  | not | and | or | xor | implies | eq | distinct | ite
  -- Ints. `sub` with one argument is negation, as SMT-LIB's `-`.
  | add | sub | mul | div | mod | abs | lt | le | gt | ge
  -- Arrays.
  | select | store
  -- Conversions.
  | bv2nat | int2bv (w : Nat)
  -- Bitvectors.
  | bvadd | bvsub | bvmul | bvudiv | bvurem | bvsdiv | bvsrem | bvsmod | bvneg
  | bvand | bvor | bvxor | bvnot | bvshl | bvlshr | bvashr
  | bvult | bvule | bvugt | bvuge | bvslt | bvsle | bvsgt | bvsge
  | concat | extract (hi lo : Nat) | zeroExtend (n : Nat) | signExtend (n : Nat)
  deriving Repr, DecidableEq, Inhabited

/-- A term. Function symbols are numbered by their declaration in the script; bound variables
are de Bruijn indices, 0 the last binder of the innermost quantifier. -/
inductive Term where
  | bool (b : Bool)
  | int (i : Int)
  /-- The bitvector literal of width `w` and value `n mod 2^w`. -/
  | bv (w n : Nat)
  /-- Declared function symbol `f` applied to its arguments (none for a constant). -/
  | fn (f : Nat) (args : List Term)
  /-- Bound variable `i`. -/
  | var (i : Nat)
  | app (op : Op) (args : List Term)
  /-- `((as const (Array idx E)) v)`. -/
  | constArray (idx : Srt) (v : Term)
  /-- The constructor of a record. -/
  | mk (fields : List Term)
  /-- Field `i` of a record. -/
  | proj (i : Nat) (t : Term)
  | «forall» (binders : List Srt) (body : Term)
  | «exists» (binders : List Srt) (body : Term)
  deriving Repr, Inhabited

/-- A declared function symbol: its name (for people), argument sorts and result sort. -/
structure Decl where
  name : String
  args : List Srt
  ret : Srt
  deriving Repr, Inhabited

/-- An interpretation: a function for every symbol number at every signature (a term uses only
the one its declaration gives), and integer `div` and `mod` by zero, which SMT-LIB leaves
unspecified. -/
structure Env where
  fn : Nat → (args : List Srt) → (ret : Srt) → Srt.DenList args → ret.Den
  divZero : Int → Int
  modZero : Int → Int

/-- Bound values, the innermost first. -/
abbrev Locals := List Val

section ops

/-- SMT-LIB `div`: the Euclidean quotient. -/
def intDiv (env : Env) (a b : Int) : Int := if b = 0 then env.divZero a else a.ediv b

/-- SMT-LIB `mod`: the Euclidean remainder, in `[0, |b|)`. -/
def intMod (env : Env) (a b : Int) : Int := if b = 0 then env.modZero a else a.emod b

/-- A relation over consecutive arguments, as `(< a b c)` means `a < b < c`. -/
def chain {α} (r : α → α → Bool) : α → List α → Bool
  | a, b :: rest => r a b && chain r b rest
  | _, [] => true

/-- A relation over every pair of positions. -/
def pairwise {α} (r : α → α → Bool) : List α → Bool
  | [] => true
  | a :: rest => rest.all (r a) && pairwise r rest

/-- The Ints of a list of values. -/
def ints : List Val → Option (List Int)
  | [] => some []
  | v :: vs => do let x ← v.as .int; let xs ← ints vs; pure (x :: xs)

/-- The Bools of a list of values. -/
def bools : List Val → Option (List Bool)
  | [] => some []
  | v :: vs => do let x ← v.as .bool; let xs ← bools vs; pure (x :: xs)

/-- Values of one sort: the first one's. -/
def sameSort (s : Srt) : List Val → Option (List s.Den)
  | [] => some []
  | v :: vs => do let x ← v.as s; let xs ← sameSort s vs; pure (x :: xs)

/-- Bitvectors of one width, the width of the first. -/
def bvs : List Val → Option ((w : Nat) × List (BitVec w))
  | [] => none
  | v :: vs => do
    let ⟨w, x⟩ ← v.bv?
    let xs ← sameSort (.bv w) vs
    pure ⟨w, x :: xs⟩

/-- A left fold over at least two arguments. -/
def foldl2 {α} (f : α → α → α) : List α → Option α
  | a :: b :: rest => some (rest.foldl f (f a b))
  | _ => none

/-- `=>` over at least two arguments, associating to the right. -/
def impliesChain (bs : List Bool) : Option Bool :=
  match bs.reverse with
  | last :: init@(_ :: _) => some (init.foldl (fun acc a => !a || acc) last)
  | _ => none

/-- An Int relation, chained over at least two arguments. -/
def intRel (r : Int → Int → Bool) (args : List Val) : Option Val := do
  let xs ← ints args
  match xs with
  | a :: rest@(_ :: _) => some ⟨.bool, chain r a rest⟩
  | _ => none

/-- A bitvector relation of two arguments. -/
def bvRel (r : (w : Nat) → BitVec w → BitVec w → Bool) (args : List Val) : Option Val := do
  let ⟨w, xs⟩ ← bvs args
  match xs with
  | [a, b] => some ⟨.bool, r w a b⟩
  | _ => none

/-- A left-associative bitvector operation of at least two arguments. -/
def bvFold (f : (w : Nat) → BitVec w → BitVec w → BitVec w) (args : List Val) : Option Val := do
  let ⟨w, xs⟩ ← bvs args
  let r ← foldl2 (f w) xs
  pure ⟨.bv w, r⟩

/-- A bitvector operation of two arguments. -/
def bvBin (f : (w : Nat) → BitVec w → BitVec w → BitVec w) (args : List Val) : Option Val := do
  let ⟨w, xs⟩ ← bvs args
  match xs with
  | [a, b] => some ⟨.bv w, f w a b⟩
  | _ => none

/-- A bitvector operation of one argument. -/
def bvUn (f : (w : Nat) → BitVec w → BitVec w) (args : List Val) : Option Val := do
  let ⟨w, xs⟩ ← bvs args
  match xs with
  | [a] => some ⟨.bv w, f w a⟩
  | _ => none

/-- The value of an operator on the values of its arguments, or `none` when they do not have
the sorts and number it takes. Bitvector division and remainder by zero are as SMT-LIB 2.6
defines them. -/
noncomputable def Op.apply (env : Env) : Op → List Val → Option Val
  | .not, args => do
    let bs ← bools args
    match bs with | [a] => some ⟨.bool, !a⟩ | _ => none
  | .and, args => do let bs ← bools args; pure ⟨.bool, bs.all id⟩
  | .or, args => do let bs ← bools args; pure ⟨.bool, bs.any id⟩
  | .xor, args => do let bs ← bools args; let r ← foldl2 Bool.xor bs; pure ⟨.bool, r⟩
  | .implies, args => do let bs ← bools args; let r ← impliesChain bs; pure ⟨.bool, r⟩
  | .eq, args =>
    match args with
    | a :: rest@(_ :: _) => do
      let xs ← sameSort a.1 rest
      pure ⟨.bool, chain (Srt.beq a.1) a.2 xs⟩
    | _ => none
  | .distinct, args =>
    match args with
    | a :: rest@(_ :: _) => do
      let xs ← sameSort a.1 rest
      pure ⟨.bool, pairwise (fun x y => !Srt.beq a.1 x y) (a.2 :: xs)⟩
    | _ => none
  | .ite, args =>
    match args with
    | [c, t, e] => do
      let b ← boolOf (some c)
      let e' ← e.as t.1
      pure (if b then t else ⟨t.1, e'⟩)
    | _ => none
  | .add, args => do let xs ← ints args; let r ← foldl2 (· + ·) xs; pure ⟨.int, r⟩
  | .sub, args => do
    let xs ← ints args
    match xs with
    | [a] => some ⟨.int, -a⟩
    | _ => do let r ← foldl2 (· - ·) xs; pure ⟨.int, r⟩
  | .mul, args => do let xs ← ints args; let r ← foldl2 (· * ·) xs; pure ⟨.int, r⟩
  | .div, args => do let xs ← ints args; let r ← foldl2 (intDiv env) xs; pure ⟨.int, r⟩
  | .mod, args => do
    let xs ← ints args
    match xs with | [a, b] => some ⟨.int, intMod env a b⟩ | _ => none
  | .abs, args => do
    let xs ← ints args
    match xs with | [a] => some ⟨.int, (a.natAbs : Int)⟩ | _ => none
  | .lt, args => intRel (fun a b => decide (a < b)) args
  | .le, args => intRel (fun a b => decide (a ≤ b)) args
  | .gt, args => intRel (fun a b => decide (b < a)) args
  | .ge, args => intRel (fun a b => decide (b ≤ a)) args
  | .select, args =>
    match args with
    | [⟨.array i e, f⟩, k] => do let k' ← k.as i; pure ⟨e, f k'⟩
    | _ => none
  | .store, args =>
    match args with
    | [⟨.array i e, f⟩, k, v] => do
      let k' ← k.as i
      let v' ← v.as e
      pure ⟨.array i e, fun j => if Srt.beq i j k' then v' else f j⟩
    | _ => none
  | .bv2nat, args =>
    match args with
    | [⟨.bv _, x⟩] => some ⟨.int, (x.toNat : Int)⟩
    | _ => none
  | .int2bv w, args => do
    let xs ← ints args
    match xs with | [a] => some ⟨.bv w, BitVec.ofInt w a⟩ | _ => none
  | .bvadd, args => bvFold (fun _ a b => a + b) args
  | .bvsub, args => bvBin (fun _ a b => a - b) args
  | .bvmul, args => bvFold (fun _ a b => a * b) args
  | .bvudiv, args => bvBin (fun _ a b => BitVec.smtUDiv a b) args
  | .bvurem, args => bvBin (fun _ a b => BitVec.umod a b) args
  | .bvsdiv, args => bvBin (fun _ a b => BitVec.smtSDiv a b) args
  | .bvsrem, args => bvBin (fun _ a b => BitVec.srem a b) args
  | .bvsmod, args => bvBin (fun _ a b => BitVec.smod a b) args
  | .bvneg, args => bvUn (fun _ a => -a) args
  | .bvand, args => bvFold (fun _ a b => a &&& b) args
  | .bvor, args => bvFold (fun _ a b => a ||| b) args
  | .bvxor, args => bvFold (fun _ a b => a ^^^ b) args
  | .bvnot, args => bvUn (fun _ a => ~~~a) args
  | .bvshl, args => bvBin (fun _ a b => a <<< b.toNat) args
  | .bvlshr, args => bvBin (fun _ a b => a >>> b.toNat) args
  | .bvashr, args => bvBin (fun _ a b => a.sshiftRight b.toNat) args
  | .bvult, args => bvRel (fun _ a b => BitVec.ult a b) args
  | .bvule, args => bvRel (fun _ a b => BitVec.ule a b) args
  | .bvugt, args => bvRel (fun _ a b => BitVec.ult b a) args
  | .bvuge, args => bvRel (fun _ a b => BitVec.ule b a) args
  | .bvslt, args => bvRel (fun _ a b => BitVec.slt a b) args
  | .bvsle, args => bvRel (fun _ a b => BitVec.sle a b) args
  | .bvsgt, args => bvRel (fun _ a b => BitVec.slt b a) args
  | .bvsge, args => bvRel (fun _ a b => BitVec.sle b a) args
  | .concat, args =>
    match args with
    | [⟨.bv w, a⟩, ⟨.bv v, b⟩] => some ⟨.bv (w + v), BitVec.append (n := w) (m := v) a b⟩
    | _ => none
  | .extract hi lo, args =>
    match args with
    | [⟨.bv w, a⟩] =>
      if lo ≤ hi ∧ hi < w then some ⟨.bv (hi - lo + 1), a.extractLsb' lo (hi - lo + 1)⟩
      else none
    | _ => none
  | .zeroExtend n, args =>
    match args with
    | [⟨.bv w, a⟩] => some ⟨.bv (w + n), a.setWidth (w + n)⟩
    | _ => none
  | .signExtend n, args =>
    match args with
    | [⟨.bv w, a⟩] => some ⟨.bv (w + n), a.signExtend (w + n)⟩
    | _ => none

end ops

/-- The sorts of a list of values. -/
def sortsOf : List Val → List Srt
  | [] => []
  | v :: vs => v.1 :: sortsOf vs

/-- A record value from its fields. -/
def pack : (vs : List Val) → Srt.DenList (sortsOf vs)
  | [] => ()
  | v :: vs => (v.2, pack vs)

/-- Bound values after binding `xs`, of sorts `ss`: the last binder is innermost. -/
def bindAll (ss : List Srt) (xs : Srt.DenList ss) (locals : Locals) : Locals :=
  (Srt.unpack ss xs).reverse ++ locals

mutual
/-- The value of a term under an interpretation of the declared symbols `decls` and values of
its bound variables, or `none` when the term or any part of it is ill-sorted. A quantifier is
`none` unless its body is a formula at every value of its binders. -/
noncomputable def Term.eval (env : Env) (decls : List Decl) (locals : Locals) : Term → Option Val
  | .bool b => some ⟨.bool, b⟩
  | .int i => some ⟨.int, i⟩
  | .bv w n => some ⟨.bv w, BitVec.ofNat w n⟩
  | .fn f args => do
    let d ← decls[f]?
    let vs ← Term.evalList env decls locals args
    let xs ← Val.asList vs d.args
    pure ⟨d.ret, env.fn f d.args d.ret xs⟩
  | .var i => locals[i]?
  | .app op args => do
    let vs ← Term.evalList env decls locals args
    op.apply env vs
  | .constArray idx v => do
    let x ← Term.eval env decls locals v
    pure ⟨.array idx x.1, fun _ => x.2⟩
  | .mk fields => do
    let vs ← Term.evalList env decls locals fields
    pure ⟨.tup (sortsOf vs), pack vs⟩
  | .proj i t => do
    let v ← Term.eval env decls locals t
    match v with
    | ⟨.tup ss, x⟩ => Srt.field ss x i
    | _ => none
  | .forall ss body =>
    @ite _ (∀ xs : Srt.DenList ss, (boolOf (Term.eval env decls (bindAll ss xs locals) body)).isSome)
      (Classical.propDecidable _)
      (some ⟨.bool, @decide (∀ xs : Srt.DenList ss,
        boolOf (Term.eval env decls (bindAll ss xs locals) body) = some true)
        (Classical.propDecidable _)⟩)
      none
  | .exists ss body =>
    @ite _ (∀ xs : Srt.DenList ss, (boolOf (Term.eval env decls (bindAll ss xs locals) body)).isSome)
      (Classical.propDecidable _)
      (some ⟨.bool, @decide (∃ xs : Srt.DenList ss,
        boolOf (Term.eval env decls (bindAll ss xs locals) body) = some true)
        (Classical.propDecidable _)⟩)
      none
/-- The values of a list of terms. -/
noncomputable def Term.evalList (env : Env) (decls : List Decl) (locals : Locals) :
    List Term → Option (List Val)
  | [] => some []
  | t :: ts => do
    let v ← Term.eval env decls locals t
    let vs ← Term.evalList env decls locals ts
    pure (v :: vs)
end

/-- A closed formula holds: it is well-sorted and evaluates to true. -/
def Holds (env : Env) (decls : List Decl) (t : Term) : Prop :=
  boolOf (Term.eval env decls [] t) = some true

/-- The Bool a closed formula evaluates to, for tests that evaluate in the kernel. -/
noncomputable def Term.evalBool (env : Env) (decls : List Decl) (t : Term) : Option Bool :=
  boolOf (Term.eval env decls [] t)

end SolarVerify.Smt
