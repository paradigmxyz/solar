/-
The kernel replay of the published theorem's constant closure (scripts/certify, step 6b), run as
  lake env lean --run tools/certify/replay_closure.lean MODULE THEOREM STATEMENT OUT [K N]
in the solidity-lean checkout after MODULE is built, with the toolchain's lean. It imports MODULE
with every olean part (private ones included) and checks:
* THEOREM is a theorem whose type is exactly the constant STATEMENT;
* THEOREM's closure, every constant its type and value mention and theirs, down to Init, with each
  inductive's whole mutual block, constructors and recursors, and each constructor's and recursor's
  inductive, holds no unsafe or partial constant (the kernel replay would skip it) and no axiom
  beyond propext, Classical.choice and Quot.sound (sorryAx, Lean.ofReduceBool, Lean.trustCompiler
  and every declared one fail it);
* the kernel accepts every constant of the closure, replayed from an empty environment, so each is
  checked against only constants of the closure, and each stored constructor and recursor equals the
  one the kernel generates for its inductive.
Nothing is skipped for its module or package: Init, Std, Lean and Mathlib constants of the closure
are checked like this package's own. It writes OUT.plan, the closure, one "NAME GROUP KIND" line per
constant sorted by name (GROUP keeps an inductive block together), and OUT.proofs, the theorems whose
proofs it checked, sorted. With K N (0 <= K < N) it is shard K of N: the closure's groups are dealt
round-robin to shards in OUT.plan's order, and a shard checks every constant but the proofs of other
shards' theorems, which it takes as axioms of the same type, whose types it checks. A sharded replay
counts only when every shard of the same plan passes and their OUT.proofs are an exact disjoint
union of the plan's theorems (tools/certify/replay.py closure). It prints CLOSURE, AXIOMS and SHARD,
and REPLAYED only on success; any failure exits 1.
For the negative tests only, --tamper NAME DONOR gives theorem NAME theorem DONOR's proof, and
--omit NAME leaves NAME out of the closure; the driver passes neither. Each adds a defect and none
can remove one.
-/
import Lean

open Lean

def allowedAxioms : List Name := [``propext, ``Classical.choice, ``Quot.sound]

-- Constants the kernel reaches without a mention in a term: the types of literals and what it
-- unfolds a string literal into. Seeding them keeps a literal's type from being unknown.
def kernelSeeds : List Name :=
  [``Nat, ``String, ``String.ofList, ``Char, ``Char.ofNat, ``List.cons, ``List.nil, ``Eq]

def fail (msg : String) : IO UInt32 := do
  IO.eprintln s!"replay_closure: {msg}"
  return 1

def kindOf : ConstantInfo → String
  | .axiomInfo _ => "axiom" | .defnInfo _ => "def" | .thmInfo _ => "theorem"
  | .opaqueInfo _ => "opaque" | .quotInfo _ => "quot" | .inductInfo _ => "inductive"
  | .ctorInfo _ => "ctor" | .recInfo _ => "rec"

/-- The constants a constant needs present to be checked, or generated with it by the kernel. -/
def neighbours (env : Environment) (ci : ConstantInfo) : Array Name := Id.run do
  let mut out := ci.getUsedConstantsAsSet.toArray
  match ci with
  | .inductInfo v =>
    for n in v.all do
      out := out.push n
      out := out.push (Name.str n "rec")
      if let some (.inductInfo w) := env.find? n then out := out ++ w.ctors.toArray
  | .ctorInfo v => out := out.push v.induct
  | .recInfo v => out := out ++ v.all.toArray
  | _ => pure ()
  return out

/-- An inductive block's constants share the group of the block's first inductive. -/
def groupOf (env : Environment) (ci : ConstantInfo) : Name :=
  let first (n : Name) : Name :=
    match env.find? n with
    | some (.inductInfo v) => v.all.headD n
    | _ => n
  match ci with
  | .inductInfo v => v.all.headD ci.name
  | .ctorInfo v => first v.induct
  | .recInfo v => first (v.all.headD ci.name)
  | _ => ci.name

partial def testFlags (args : List String) (tamper : Option (Name × Name)) (omitted : Option Name) :
    Option (Name × Name) × Option Name × List String :=
  match args with
  | "--tamper" :: n :: d :: rest => testFlags rest (some (n.toName, d.toName)) omitted
  | "--omit" :: n :: rest => testFlags rest tamper (some n.toName)
  | _ => (tamper, omitted, args)

def usage : String :=
  "usage: replay_closure [--tamper NAME DONOR] [--omit NAME] MODULE THEOREM STATEMENT OUT [K N]"

def main (args : List String) : IO UInt32 := do
  initSearchPath (← findSysroot)
  let (tamper, omitted, args) := testFlags args none none
  let (modS, thmS, stmtS, out, shard) ← match args with
    | [m, t, s, o] => pure (m, t, s, o, none)
    | [m, t, s, o, k, n] =>
      match k.toNat?, n.toNat? with
      | some k, some n =>
        if k < n then pure (m, t, s, o, some (k, n))
        else throw <| IO.userError s!"shard {k} is not below {n}"
      | _, _ => throw <| IO.userError s!"shard {k} {n} is not two numbers"
    | _ => throw <| IO.userError usage
  let thm := thmS.toName
  let stmt := stmtS.toName
  let env ← importModules #[{ module := modS.toName }] {} (loadExts := false)
  let some root := env.find? thm | fail s!"{modS} has no constant {thm}"
  let .thmInfo rootThm := root | fail s!"{thm} is not a theorem"
  unless rootThm.type == .const stmt [] do
    return ← fail s!"the type of {thm} is not exactly {stmt}: {rootThm.type}"
  -- The closure, by a work list from the theorem and the kernel's seeds.
  let mut seen : NameSet := {}
  let mut todo : Array Name := #[thm] ++ kernelSeeds.toArray
  let mut closure : Std.HashMap Name ConstantInfo := {}
  while todo.size > 0 do
    let n := todo.back!
    todo := todo.pop
    if seen.contains n then continue
    seen := seen.insert n
    let some ci := env.find? n
      | return ← fail s!"{n}, needed by the closure of {thm}, is not in the environment"
    if ci.isUnsafe || ci.isPartial then
      return ← fail s!"{n} in the closure of {thm} is unsafe or partial: the kernel replay would skip it"
    closure := closure.insert n ci
    for m in neighbours env ci do
      unless seen.contains m do todo := todo.push m
  if let some (n, d) := tamper then
    let some (.thmInfo t) := closure[n]? | return ← fail s!"--tamper: {n} is not a theorem of the closure"
    let some (.thmInfo dt) := env.find? d | return ← fail s!"--tamper: {d} is not a theorem"
    closure := closure.insert n (.thmInfo { t with value := dt.value })
    IO.println s!"TEST tampered {n} with the proof of {d}"
  if let some n := omitted then
    unless closure.contains n do return ← fail s!"--omit: {n} is not in the closure"
    closure := closure.erase n
    IO.println s!"TEST omitted {n}"
  -- Sorted by printed name, the order the accounting (replay.py closure) checks.
  let names := closure.toArray.map (·.1) |>.qsort fun a b => a.toString < b.toString
  let groups := names.map fun n => groupOf env closure[n]!
  IO.FS.writeFile (out ++ ".plan") <| String.join <| (names.zip groups).toList.map fun (n, g) =>
    s!"{n} {g} {kindOf closure[n]!}\n"
  -- Groups in plan order, each numbered by its first appearance.
  let mut idx : Std.HashMap Name Nat := {}
  for g in groups do
    unless idx.contains g do idx := idx.insert g idx.size
  IO.println s!"CLOSURE {names.size} constants, {idx.size} groups"
  let axs := names.filter fun n => closure[n]! matches .axiomInfo _
  IO.println s!"AXIOMS {axs.toList}"
  let bad := axs.toList.filter fun a => !allowedAxioms.contains a
  unless bad.isEmpty do
    return ← fail s!"the closure of {thm} holds axioms beyond propext, Classical.choice and Quot.sound: {bad}"
  -- A shard checks every constant of the closure except the proofs of other shards' theorems:
  -- those it takes as axioms of the same name, type and universe parameters, whose types the kernel
  -- still checks here. Every theorem's proof is checked by exactly one shard.
  let (k, nshards) := shard.getD (0, 1)
  let mine (g : Name) : Bool := (idx[g]?.getD 0) % nshards == k
  let mut check : Std.HashMap Name ConstantInfo := {}
  let mut proofs : Array Name := #[]
  for n in names, g in groups do
    let ci := closure[n]!
    match ci with
    | .thmInfo t =>
      if mine g then
        check := check.insert n ci
        proofs := proofs.push n
      else
        check := check.insert n (.axiomInfo { toConstantVal := t.toConstantVal, isUnsafe := false })
    | _ => check := check.insert n ci
  IO.FS.writeFile (out ++ ".proofs") <| String.join <| proofs.toList.map (s!"{·}\n")
  IO.println s!"SHARD {k} {nshards} {proofs.size} theorem proofs to check"
  let kenv := (← mkEmptyEnvironment).toKernelEnv
  let replayed ← try kenv.replay check catch e => return ← fail s!"the kernel rejects the closure of {thm}: {e}"
  for n in names do
    unless replayed.find? n |>.isSome do
      return ← fail s!"{n} of the closure is not in the replayed environment"
  if mine (groupOf env root) then
    let some (.thmInfo t) := replayed.find? thm
      | return ← fail s!"{thm} is not a theorem of the replayed environment"
    unless t.type == .const stmt [] do
      return ← fail s!"the replayed type of {thm} is not exactly {stmt}"
  IO.println s!"REPLAYED {names.size} constants, {proofs.size} theorem proofs, of the closure of {thm} (shard {k} of {nshards})"
  return 0
