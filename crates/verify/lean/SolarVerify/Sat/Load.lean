import Lean
import SolarVerify.Sat.Lrat
/-!
Reading a DIMACS formula and a textual LRAT proof from files into terms the kernel checks.

Nothing here is trusted: a wrong term only makes `check` return `false`. Long lists and the proof
become balanced trees of appends and `Proof.seq`, so the terms stay shallow.
-/
namespace SolarVerify.Sat.Load
open Lean Elab Command Meta

/-- The literal code of a nonzero DIMACS literal. -/
def litOfInt (i : Int) : Nat := if i > 0 then 2 * i.toNat else 2 * i.natAbs + 1

def ints (line : String) : Except String (List Int) :=
  (line.splitOn " ").filter (· ≠ "") |>.mapM fun w =>
    match w.toInt? with
    | some i => .ok i
    | none => .error s!"not a number: {w}"

/-- The clauses of a DIMACS file. -/
def parseDimacs (s : String) : Except String (Array (List Nat)) := do
  let mut out := #[]
  let mut cur : List Nat := []
  for line in s.splitOn "\n" do
    let line := line.trimAscii.toString
    if line.isEmpty || line.startsWith "c" || line.startsWith "p" then continue
    for i in ← ints line do
      if i == 0 then
        out := out.push cur.reverse
        cur := []
      else
        cur := litOfInt i :: cur
  return out

/-- The addition steps of a textual LRAT proof; deletions are skipped and RAT hints rejected. -/
def parseLrat (s : String) : Except String (Array Step) := do
  let mut out := #[]
  for line in s.splitOn "\n" do
    let line := line.trimAscii.toString
    if line.isEmpty || line.startsWith "c" then continue
    let ws := (line.splitOn " ").filter (· ≠ "")
    if ws.length ≥ 2 && ws[1]! == "d" then continue
    let xs ← ints line
    match xs with
    | [] => continue
    | id :: rest =>
      let literals := rest.takeWhile (· ≠ 0)
      let hints := (rest.dropWhile (· ≠ 0)).drop 1 |>.takeWhile (· ≠ 0)
      if hints.any (· < 0) then throw s!"step {id}: RAT steps are not supported"
      out := out.push { id := id.toNat, clause := literals.map litOfInt, hints := hints.map Int.toNat }
  return out

def listTy (ty : Expr) : Expr := mkApp (mkConst ``List [0]) ty

/-- A list literal, as a balanced tree of appends above 64 elements. -/
partial def mkList (ty : Expr) (xs : Array Expr) (lo hi : Nat) : Expr :=
  if hi - lo ≤ 64 then
    (List.range (hi - lo)).foldr (init := mkApp (mkConst ``List.nil [0]) ty) fun i acc =>
      mkApp3 (mkConst ``List.cons [0]) ty xs[lo + i]! acc
  else
    let mid := (lo + hi) / 2
    mkApp3 (mkConst ``List.append [0]) ty (mkList ty xs lo mid) (mkList ty xs mid hi)

def mkNatList (xs : List Nat) : Expr :=
  let a := xs.toArray.map mkRawNatLit
  mkList (mkConst ``Nat) a 0 a.size

def mkStep (s : Step) : Expr :=
  mkApp (mkConst ``Proof.step) <|
    mkApp3 (mkConst ``Step.mk) (mkRawNatLit s.id) (mkNatList s.clause) (mkNatList s.hints)

partial def mkProof (ss : Array Step) (lo hi : Nat) : Expr :=
  if hi - lo == 1 then mkStep ss[lo]!
  else
    let mid := (lo + hi) / 2
    mkApp2 (mkConst ``Proof.seq) (mkProof ss lo mid) (mkProof ss mid hi)

def addDef (n : Name) (ty v : Expr) : CoreM Unit :=
  addDecl <| .defnDecl (mkDefinitionValEx n [] ty v (.regular 0) .safe [])

/-- `sat_load n "f.cnf" "f.lrat"` defines `n.cnf : Cnf` and `n.proof : Proof` from the files,
relative to the current file. -/
elab "sat_load " n:ident cnf:str lrat:str : command => do
  let dir := (System.FilePath.mk (← getFileName)).parent.getD "."
  let cs ← IO.FS.readFile (dir / cnf.getString)
  let ps ← IO.FS.readFile (dir / lrat.getString)
  let clauses ← match parseDimacs cs with | .ok c => pure c | .error e => throwError e
  let steps ← match parseLrat ps with | .ok c => pure c | .error e => throwError e
  if steps.isEmpty then throwError "the proof adds no clauses"
  let base := (← getCurrNamespace) ++ n.getId
  let cnfVal := mkList (listTy (mkConst ``Nat)) (clauses.map (mkNatList ·)) 0 clauses.size
  liftCoreM do
    addDef (base ++ `cnf) (mkConst ``Cnf) cnfVal
    addDef (base ++ `proof) (mkConst ``Proof) (mkProof steps 0 steps.size)
  logInfo m!"{clauses.size} clauses, {steps.size} proof steps"


/-! ## One declaration per step -/

def natE (n : Nat) : Expr := mkRawNatLit n
def boolTrue : Expr := mkConst ``Bool.true
def eqTrue (b : Expr) : Expr := mkApp3 (mkConst ``Eq [1]) (mkConst ``Bool) b boolTrue
def reflTrue : Expr := mkApp2 (mkConst ``Eq.refl [1]) (mkConst ``Bool) boolTrue
def clauseListTy : Expr := listTy (mkConst ``Clause)
def consE (ty x xs : Expr) : Expr := mkApp3 (mkConst ``List.cons [0]) ty x xs
def nilE (ty : Expr) : Expr := mkApp (mkConst ``List.nil [0]) ty

/-- The formula as a balanced tree of appends over chunks of at most 16 clauses, each subtree its
own definition `base.cnfN` so that the kernel reads its type rather than inferring it again in
every lemma. Returns the subtree's constant and, for each of its clauses, a function giving the
proof that the clause is a member. -/
partial def cnfTree (base : Name) (cs : Array Expr) (lo hi : Nat) :
    StateT Nat CoreM (Expr × Array (Unit → Expr)) := do
  let idx ← modifyGet fun n => (n, n + 1)
  let name := base ++ Name.mkSimple s!"cnf{idx}"
  let k := mkConst name
  if hi - lo ≤ 16 then
    let n := hi - lo
    let mut acc : Array Expr := #[nilE (mkConst ``Clause)]
    for j in [0:n] do
      acc := acc.push (consE (mkConst ``Clause) cs[hi - 1 - j]! acc.back!)
    let suffixes := acc.reverse  -- suffixes[j] is the list from lo + j
    liftM (addDef name clauseListTy suffixes[0]! : CoreM Unit)
    let mems := (List.range n).toArray.map fun j => fun (_ : Unit) => Id.run do
      let mut p := mkApp2 (mkConst ``SolarVerify.Sat.mem_head) cs[lo + j]! suffixes[j + 1]!
      for k' in [0:j] do
        let i := j - 1 - k'
        p := mkApp4 (mkConst ``SolarVerify.Sat.mem_tail) cs[lo + j]! cs[lo + i]! suffixes[i + 1]! p
      return p
    return (k, mems)
  else
    let mid := (lo + hi) / 2
    let (l, ml) ← cnfTree base cs lo mid
    let (r, mr) ← cnfTree base cs mid hi
    liftM (addDef name clauseListTy (mkApp3 (mkConst ``List.append [0]) (mkConst ``Clause) l r) : CoreM Unit)
    let ml' := (List.range ml.size).toArray.map fun i => fun (_ : Unit) =>
      mkApp4 (mkConst ``SolarVerify.Sat.mem_append_left) cs[lo + i]! l r (ml[i]! ())
    let mr' := (List.range mr.size).toArray.map fun i => fun (_ : Unit) =>
      mkApp4 (mkConst ``SolarVerify.Sat.mem_append_right) cs[mid + i]! l r (mr[i]! ())
    return (k, ml' ++ mr')

def addThm (n : Name) (ty v : Expr) : CoreM Unit :=
  addDecl <| .thmDecl { name := n, levelParams := [], type := ty, value := v }

/-- `∀ a, Cnf.eval a F = true → Clause.eval a c = true` -/
def stepTy (F c : Expr) : Expr :=
  mkForall `a .default (mkConst ``Assignment) <|
    mkForall `h .default (eqTrue (mkApp2 (mkConst ``Cnf.eval) (.bvar 0) F)) <|
      eqTrue (mkApp2 (mkConst ``Clause.eval) (.bvar 1) c)

def lamAH (F body : Expr) : Expr :=
  mkLambda `a .default (mkConst ``Assignment) <|
    mkLambda `h .default (eqTrue (mkApp2 (mkConst ``Cnf.eval) (.bvar 0) F)) body

/-- `sat_prove n "f.cnf" "f.lrat"` defines `n.cnf : Cnf` from the DIMACS file and proves
`n.unsat : n.cnf.Unsat` from the LRAT proof, one kernel-checked lemma per needed step. -/
elab "sat_prove " n:ident cnf:str lrat:str : command => do
  let dir := (System.FilePath.mk (← getFileName)).parent.getD "."
  let cs ← IO.FS.readFile (dir / cnf.getString)
  let ps ← IO.FS.readFile (dir / lrat.getString)
  let clauses ← match parseDimacs cs with | .ok c => pure c | .error e => throwError e
  let steps ← match parseLrat ps with | .ok c => pure c | .error e => throwError e
  let some last := steps.findIdx? (·.clause.isEmpty) | throwError "the proof never adds the empty clause"
  -- Keep only the steps the empty clause depends on.
  let nIn := clauses.size
  let mut byId : Std.HashMap Nat Nat := {}
  for i in [0:last + 1] do byId := byId.insert steps[i]!.id i
  let mut need : Std.HashSet Nat := {}
  let mut needIn : Std.HashSet Nat := {}
  let mut todo := [last]
  need := need.insert last
  while !todo.isEmpty do
    let i := todo.head!
    todo := todo.tail!
    for h in steps[i]!.hints do
      if h ≥ 1 && h ≤ nIn then needIn := needIn.insert h
      else match byId[h]? with
        | some j => if !need.contains j then need := need.insert j; todo := j :: todo
        | none => throwError "step {steps[i]!.id}: unknown hint {h}"
  let base := (← getCurrNamespace) ++ n.getId
  let cExprs := clauses.map mkNatList
  let ((fRoot, mems), _) ← liftCoreM <| (cnfTree base cExprs 0 nIn).run 0
  let F := mkConst (base ++ `cnf)
  liftCoreM <| addDef (base ++ `cnf) (mkConst ``Cnf) fRoot
  let lemmaOf (h : Nat) : Name :=
    if h ≤ nIn then base ++ Name.mkSimple s!"in{h}" else base ++ Name.mkSimple s!"s{h}"
  -- Input clauses the proof uses.
  for h in needIn.toArray.qsort (· < ·) do
    let c := cExprs[h - 1]!
    let v := lamAH F <| mkApp5 (mkConst ``Cnf.eval_mem) (.bvar 1) F c (.bvar 0) (mems[h - 1]! ())
    liftCoreM <| addThm (lemmaOf h) (stepTy F c) v
  -- Derived clauses, in order.
  let mut stepClause : Std.HashMap Nat Expr := {}
  let mut count : Nat := 0
  for i in [0:last + 1] do
    unless need.contains i do continue
    let s := steps[i]!
    let c := mkNatList s.clause
    stepClause := stepClause.insert s.id c
    let hcs := s.hints.toArray.map fun h => if h ≤ nIn then cExprs[h - 1]! else stepClause[h]!
    -- suffix lists of hint clauses and the AllSat proof, from the end
    let mut lst := nilE (mkConst ``Clause)
    let mut prf := mkApp (mkConst ``AllSat.nil) (.bvar 1)
    for j' in [0:hcs.size] do
      let j := hcs.size - 1 - j'
      let hp := mkApp2 (mkConst (lemmaOf s.hints[j]!)) (.bvar 1) (.bvar 0)
      prf := mkApp5 (mkConst ``AllSat.cons) (.bvar 1) hcs[j]! lst hp prf
      lst := consE (mkConst ``Clause) hcs[j]! lst
    let v := lamAH F <| mkApp5 (mkConst ``rup_clause) (.bvar 1) c lst reflTrue prf
    liftCoreM <| addThm (lemmaOf s.id) (stepTy F c) v
    count := count + 1
  let emptyLemma := mkConst (lemmaOf steps[last]!.id)
  liftCoreM <| addThm (base ++ `unsat) (mkApp (mkConst ``Cnf.Unsat) F)
    (mkApp2 (mkConst ``unsat_of_empty) F emptyLemma)
  logInfo m!"{nIn} clauses, {steps.size} proof steps, {count} needed, {needIn.size} input clauses used"

end SolarVerify.Sat.Load
