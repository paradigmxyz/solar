/-
The certify driver's checker of a published statement (scripts/certify step 6b), run as
  lake env lean --run tools/certify/check_final.lean NAME PROOF_MODULE
in the solidity-lean checkout after NAME.Statement and NAME.Final are built. It imports only Lean
itself and reads the compiled environments, so no declaration, alias, notation or macro of the
proof module reaches it. It fails (exit 1) unless:
* Certified.NAME.Final.final is a theorem of module NAME.Final whose type is exactly the constant
  Certified.NAME.Final.Statement;
* Certified.NAME.Final.Statement is a definition of module NAME.Statement, and NAME.Statement's
  imports do not reach the proof module (nor NAME.Final);
* the axioms final depends on are among propext, Classical.choice and Quot.sound, and not none of
  them, and the statement is not True (its value is printed as a STATEMENT line);
* given HEX, the runtime bytecode this run's solar emitted (lowercase hex), the fresh-byte gate:
  Certified.NAME.Final.runtimeBytes is a definition of module NAME.Statement whose value is a list
  literal of exactly HEX's bytes, the statement is a conjunction whose first conjunct is an equality
  of a `List Nat` with that constant (final.py's `bound` or `runtime_is_fresh`, which the kernel
  decides), and its second conjunct names `runtimeBytes` or `code`; `code` is the statement
  module's own, built from `runtimeBytes` or read back by the first conjunct, and every certified
  shape the claim applies (Shapes.Certified, Functional, Compiled, OpenFunctional, Generic.Expected)
  takes `code` (Expected: `runtimeBytes`) as its bytecode, so the claim is about those bytes.
It prints "MODULE M PATH replay|trusted IMPORTS" for every module of final's closure, marking for
replay each outside the toolchain and the upstream git-pinned lake packages (Mathlib and its
dependencies); the driver replays
them through the kernel with the toolchain's leanchecker, since an olean can hold a declaration the
kernel never checked.
-/
import Lean

open Lean

def allowedAxioms : List Name := [``propext, ``Classical.choice, ``Quot.sound]

def fail (msg : String) : IO UInt32 := do
  IO.eprintln s!"check_final: {msg}"
  return 1

/-- The closure of a module, one line per module, imports first:
"MODULE NAME PATH replay|trusted IMPORTS...". Trusted without replay: the toolchain's own library and
the upstream lake packages the manifest pins by git revision (Mathlib and its dependencies, listed
below). Every other module (this package's, its path dependencies under vendor/, EthLib, and any
olean found elsewhere) is marked for replay, wherever its olean is. -/
def printClosure (env : Environment) : IO Unit := do
  let sysLib ← IO.FS.realPath ((← findSysroot) / "lib" / "lean")
  -- CERTIFY_TRUSTED_ROOTS="DIR=Root,Root;DIR=Root;..." comes from the driver, which resolves each
  -- upstream package's directory before the proof builds; a module is trusted only when its olean
  -- is under one of these directories and its name under one of that package's roots. Without the
  -- variable nothing outside the toolchain is trusted.
  let spec := (← IO.getEnv "CERTIFY_TRUSTED_ROOTS").getD ""
  let pkgs : List (String × List String) := (spec.splitOn ";").filterMap fun e =>
    match e.splitOn "=" with
    | [d, rs] => if d.isEmpty then none else some (d, rs.splitOn ",")
    | _ => none
  let under (d f : String) : Bool := f.startsWith (d ++ "/")
  for m in env.header.moduleNames, d in env.header.moduleData do
    let path := (← IO.FS.realPath (← findOLean m)).toString
    let root := m.getRoot.toString
    let trusted := under sysLib.toString path ||
      pkgs.any (fun (dir, roots) => under dir path && roots.contains root)
    let imports := d.imports.toList.map (·.module.toString)
    IO.println s!"MODULE {m} {path} {if trusted then "trusted" else "replay"} {" ".intercalate imports}"

/-- The natural number a literal denotes (`OfNat.ofNat _ (lit n) _` or a raw literal). -/
partial def natLit? (e : Expr) : Option Nat :=
  match e.consumeMData with
  | .lit (.natVal n) => some n
  | e => if e.isAppOfArity ``OfNat.ofNat 3 then natLit? (e.getArg! 1) else none

/-- The numbers of a `List Nat` literal: a chain of `List.cons` ending in `List.nil`, with the
local definitions a long literal elaborates to (`have y := [..]; a :: .. :: y`) substituted. -/
partial def natList? (e : Expr) : Option (List Nat) :=
  let e := e.consumeMData
  if let .letE _ _ v b _ := e then natList? (b.instantiate1 v)
  else if e.isAppOfArity ``List.nil 1 then some []
  else if e.isAppOfArity ``List.cons 3 then do
    let h ← natLit? (e.getArg! 1)
    let t ← natList? (e.getArg! 2)
    pure (h :: t)
  else none

/-- The bytes of a lowercase hex string, or none. -/
def hexBytes? (s : String) : Option (List Nat) :=
  let digit (c : Char) : Option Nat :=
    if '0' ≤ c ∧ c ≤ '9' then some (c.toNat - 48)
    else if 'a' ≤ c ∧ c ≤ 'f' then some (c.toNat - 87) else none
  let rec go : List Char → Option (List Nat)
    | [] => some []
    | a :: b :: rest => do pure ((16 * (← digit a) + (← digit b)) :: (← go rest))
    | [_] => none
  go s.toList

/-- `f` is the function `g` or its eta expansion `fun x => g x`. -/
def isFn (f : Expr) (g : Name) : Bool :=
  let f := f.consumeMData
  f.isConstOf g || match f with
    | .lam _ _ b _ =>
      let b := b.consumeMData
      b.isAppOfArity g 1 && (b.getArg! 0).consumeMData == .bvar 0
    | _ => false

/-- `v` is `⟨(rb.map UInt8.ofNat).toArray⟩`: the code of the constant byte list `rb`. -/
def codeOfBytes (v : Expr) (rb : Name) : Bool := Id.run do
  let v := v.consumeMData
  unless v.isAppOfArity `ByteArray.mk 1 do return false
  let a := (v.getArg! 0).consumeMData
  unless a.isAppOfArity `List.toArray 2 do return false
  let m := (a.getArg! 1).consumeMData
  return m.isAppOfArity `List.map 4 && isFn (m.getArg! 2) `UInt8.ofNat &&
    (m.getArg! 3).consumeMData.isConstOf rb

/-- `e` is `codeC.data.toList.map (·.toNat)`: the bytes of the constant `codeC`. -/
def bytesOfCode (e : Expr) (codeC : Name) : Bool := Id.run do
  let e := e.consumeMData
  unless e.isAppOfArity `List.map 4 && isFn (e.getArg! 2) `UInt8.toNat do return false
  let l := (e.getArg! 3).consumeMData
  unless l.isAppOfArity `Array.toList 2 do return false
  let d := (l.getArg! 1).consumeMData
  if d.isAppOfArity `ByteArray.data 1 then return (d.getArg! 0).consumeMData.isConstOf codeC
  if let .proj `ByteArray 0 b := d then return b.consumeMData.isConstOf codeC
  return false

/-- The certified shapes a published claim may apply, with the index of their code argument
(`true`: the argument is the byte list, as `Generic.Expected`'s). -/
def shapeCodeArg : List (Name × Nat × Bool) :=
  [(`SolidCore.Refinement.Shapes.Certified, 2, false),
   (`SolidCore.Refinement.Shapes.Functional, 2, false),
   (`SolidCore.Refinement.Shapes.Compiled, 2, false),
   (`SolidCore.Refinement.Shapes.OpenFunctional, 2, false),
   (`SolidCore.Verify.Generic.Expected, 3, true)]

/-- The code argument of every certified shape applied in `e` (syntactically). -/
partial def shapeArgs (e : Expr) (acc : Array (Bool × Expr) := #[]) : Array (Bool × Expr) :=
  let acc := match shapeCodeArg.find? (fun (n, i, _) => e.isAppOf n && e.getAppNumArgs > i) with
    | some (_, i, b) => acc.push (b, e.getArg! i)
    | none => acc
  match e with
  | .app f a => shapeArgs a (shapeArgs f acc)
  | .lam _ t b _ | .forallE _ t b _ => shapeArgs b (shapeArgs t acc)
  | .letE _ t v b _ => shapeArgs b (shapeArgs v (shapeArgs t acc))
  | .mdata _ b | .proj _ _ b => shapeArgs b acc
  | _ => acc

/-- The fresh-byte gate (none when it passes, else why not): `runtimeBytes` of the statement module
is exactly HEX's bytes as a literal, and the statement `SVAL` is `L = runtimeBytes ∧ claim` with `L`
a `List Nat` and the claim naming `runtimeBytes` or `code`. -/
def freshByteGate (senv : Environment) (ns stmtMod : Name) (sval : Expr) (hex : String) :
    Option String := Id.run do
  let rb := ns ++ `runtimeBytes
  let some want := hexBytes? hex | return some "HEX is not lowercase hex"
  if want.isEmpty then return some "HEX is empty"
  let some (.defnInfo d) := senv.find? rb | return some s!"{stmtMod} does not define {rb}"
  unless senv.getModuleIdxFor? rb == senv.getModuleIdx? stmtMod do
    return some s!"{rb} is not declared by {stmtMod}"
  let some got := natList? d.value | return some s!"{rb} is not a list literal"
  unless got == want do
    return some s!"{rb} ({got.length} bytes) is not the {want.length} bytes this run emitted"
  let v := sval.consumeMData
  unless v.isAppOfArity ``And 2 do
    return some "the statement is not a conjunction of the binding of the bytes and the claim"
  let bind := (v.getArg! 0).consumeMData
  let ty := (bind.getArg! 0).consumeMData
  unless bind.isAppOfArity ``Eq 3 && ty.isAppOfArity ``List 1 && (ty.getArg! 0).isConstOf ``Nat &&
      (bind.getArg! 2).consumeMData.isConstOf rb do
    return some s!"the statement's first conjunct is not an equality of a List Nat with {rb}"
  let used := (v.getArg! 1).getUsedConstants
  unless used.contains rb || used.contains (ns ++ `code) do
    return some s!"the statement's claim names neither {rb} nor {ns ++ `code}"
  -- The bytes the claim certifies are the bound ones: `code` is the statement module's own, built
  -- from `runtimeBytes` (pinned mode) or read back by the first conjunct (plain mode), and every
  -- certified shape the claim applies takes `code` (or, for Generic.Expected, `runtimeBytes`).
  let codeC := ns ++ `code
  let some (.defnInfo cd) := senv.find? codeC | return some s!"{stmtMod} does not define {codeC}"
  unless senv.getModuleIdxFor? codeC == senv.getModuleIdx? stmtMod do
    return some s!"{codeC} is not declared by {stmtMod}"
  unless codeOfBytes cd.value rb || bytesOfCode (bind.getArg! 1) codeC do
    return some s!"{codeC} is neither built from {rb} nor read back by the first conjunct"
  let args := shapeArgs (v.getArg! 1)
  if args.isEmpty then
    return some "the claim applies no certified shape (Shapes.Certified, Functional, Compiled, OpenFunctional or Generic.Expected) to the bytes"
  for (isBytes, a) in args do
    let want := if isBytes then rb else codeC
    unless a.consumeMData.isConstOf want do
      return some s!"a certified shape in the claim is applied to {a}, not {want}"
  return none

def main (args : List String) : IO UInt32 := do
  initSearchPath (← findSysroot)
  if let ["--closure", m] := args then
    printClosure (← importModules #[{ module := m.toName }] {} (loadExts := false))
    return 0
  let (name, proofMod, hex?) ← match args with
    | [n, p] => pure (n, p, none)
    | [n, p, h] => pure (n, p, some h)
    | _ => return ← fail "usage: check_final NAME PROOF_MODULE [HEX] | check_final --closure MODULE"
  let stmtMod := (name ++ ".Statement").toName
  let finalMod := (name ++ ".Final").toName
  let proofMod := proofMod.toName
  let ns := ("Certified." ++ name ++ ".Final").toName
  let finalThm := ns ++ `final
  let stmt := ns ++ `Statement
  -- The statement's own environment: it must not see the proof module.
  let senv ← importModules #[{ module := stmtMod }] {} (loadExts := false)
  if (senv.getModuleIdx? proofMod).isSome then
    return ← fail s!"{stmtMod} imports the proof module {proofMod}"
  if (senv.getModuleIdx? finalMod).isSome then
    return ← fail s!"{stmtMod} imports {finalMod}"
  let some sinfo := senv.find? stmt | fail s!"{stmtMod} does not define {stmt}"
  unless sinfo matches .defnInfo _ do
    return ← fail s!"{stmt} is not a definition"
  let env ← importModules #[{ module := finalMod }] {} (loadExts := false)
  let some info := env.find? finalThm | fail s!"{finalMod} does not define {finalThm}"
  let .thmInfo t := info | fail s!"{finalThm} is not a theorem"
  unless env.getModuleIdxFor? finalThm == env.getModuleIdx? finalMod do
    return ← fail s!"{finalThm} is not declared by {finalMod}"
  unless env.getModuleIdxFor? stmt == env.getModuleIdx? stmtMod do
    return ← fail s!"{stmt} is not declared by {stmtMod}"
  unless t.type == .const stmt [] do
    return ← fail s!"the type of {finalThm} is not exactly {stmt}: {t.type}"
  -- The statement's definition is the one the statement module compiled.
  let some sinfo' := env.find? stmt | fail s!"{stmt} is missing from {finalMod}'s environment"
  unless sinfo'.value? == sinfo.value? && sinfo'.type == sinfo.type do
    return ← fail s!"{stmt} differs between {stmtMod} and {finalMod}"
  let (axs, _) ← (Lean.collectAxioms finalThm : CoreM (Array Name)).toIO
    { fileName := "check_final", fileMap := default } { env }
  let bad := axs.toList.filter (fun a => !allowedAxioms.contains a)
  unless bad.isEmpty do
    return ← fail s!"{finalThm} depends on axioms beyond propext, Classical.choice and Quot.sound: {bad}"
  -- The statement is printed in full, so that the record shows what was proved. A statement that
  -- is True, or a proof of it that needs none of the three axioms, cannot be one of the published
  -- claims (each relates the exact bytecode's runs to a specification through classical
  -- reasoning), so either is refused: it is what a substituted trivial statement looks like.
  let some sval := sinfo.value? | fail s!"{stmt} has no value"
  IO.println s!"STATEMENT {stmt} := {sval}"
  if sval.consumeMData.isConstOf ``True then
    return ← fail s!"{stmt} is True"
  if axs.isEmpty then
    return ← fail s!"{finalThm} depends on no axiom at all: a published statement's proof uses propext, Classical.choice or Quot.sound"
  if let some hex := hex? then
    if let some why := freshByteGate senv ns stmtMod sval hex then
      return ← fail s!"fresh-byte gate: {why}"
    IO.println s!"FRESH {ns ++ `runtimeBytes} = the {hex.length / 2} bytes this run emitted"
  -- The modules this package built (their oleans under its .lake/build), for the driver to
  -- replay through the kernel (leanchecker): an olean records declarations without the kernel
  -- having to have checked them, so the types and axioms read here are only as good as that replay.
  printClosure env
  IO.println s!"check_final: {finalThm} : {stmt} (module {stmtMod}, which does not import {proofMod}); axioms {axs.toList}"
  return 0
