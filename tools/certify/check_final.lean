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
  them, and the statement is not True (its value is printed as a STATEMENT line).
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

def main (args : List String) : IO UInt32 := do
  initSearchPath (← findSysroot)
  if let ["--closure", m] := args then
    printClosure (← importModules #[{ module := m.toName }] {} (loadExts := false))
    return 0
  let [name, proofMod] := args | fail "usage: check_final NAME PROOF_MODULE | check_final --closure MODULE"
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
  -- The modules this package built (their oleans under its .lake/build), for the driver to
  -- replay through the kernel (leanchecker): an olean records declarations without the kernel
  -- having to have checked them, so the types and axioms read here are only as good as that replay.
  printClosure env
  IO.println s!"check_final: {finalThm} : {stmt} (module {stmtMod}, which does not import {proofMod}); axioms {axs.toList}"
  return 0
