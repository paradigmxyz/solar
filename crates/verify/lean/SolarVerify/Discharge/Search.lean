/-
The search that closes a shallow obligation: hypotheses over integers, Booleans as propositions,
arrays as functions and quantifiers over the integers, and the goal `False`. It only builds
proof terms; the kernel checks every one, so a bug here can make a proof fail, never make a
false statement proved.

1. `vc_defs`: definitions `x = t` and `g ↔ φ` of local symbols are substituted away;
2. `vc_norm`: negations are pushed in, implications become disjunctions (which `omega` reads),
   constants fold;
3. the search splits conjunctions, existentials (whose witnesses it names) and disjunctions with
   quantifiers, instantiates universal hypotheses at the witnesses and at the arguments that
   match their array reads (E-matching on one-variable triggers), splits conditionals and
   propositional atoms, adds congruence facts for array reads, and runs `omega` (linear integer
   arithmetic, with literal divisors) on each case.
-/
import Lean
import SolarVerify.Discharge.Lemmas

open Lean Meta Elab Tactic

namespace SolarVerify.Discharge

/-- Replaces one hypothesis `h : a ↔ b` whose side is a local proposition by `propext h : a = b`,
so that `subst_vars` can eliminate it, and repeats on the new context until none is left. -/
partial def iffToEq : TacticM Unit := withMainContext do
  for ldecl in (← getLCtx) do
    if ldecl.isImplementationDetail then continue
    let ty ← instantiateMVars ldecl.type
    if let some (a, b) := ty.iff? then
      if a.isFVar || b.isFVar then
        let pf ← mkAppM `propext #[ldecl.toExpr]
        let r ← (← getMainGoal).replace ldecl.fvarId pf
        replaceMainGoal [r.mvarId]
        iffToEq
        return

/-- Whether `e` has a quantifier: an `∃`, or a `∀` over the integers (the only sort the verifier
quantifies over). -/
partial def hasQuantifier (e : Expr) : Bool :=
  match e with
  | .forallE _ d b _ => d == mkConst `Int || hasQuantifier d || hasQuantifier b
  | .app f a => (e.isAppOfArity `Exists 2) || hasQuantifier f || hasQuantifier a
  | .lam _ t b _ => hasQuantifier t || hasQuantifier b
  | .mdata _ b => hasQuantifier b
  | _ => false

/-- The prefix of the names of the witnesses `splitHyps` takes out of existentials. -/
def witnessPrefix : String := "vc_w"

/-- Splits one hypothesis that is a conjunction or an existential, naming the witness of an
existential `vc_w…`, and repeats until none is left. -/
partial def splitHyps : TacticM Unit := withMainContext do
  for ldecl in (← getLCtx) do
    if ldecl.isImplementationDetail then continue
    let ty ← instantiateMVars ldecl.type
    let isEx := ty.isAppOfArity `Exists 2
    let quantOr := ty.isAppOfArity `Or 2 && hasQuantifier ty
    if ty.isAppOfArity `And 2 || isEx || quantOr then
      let gs ← (← getMainGoal).cases ldecl.fvarId
      let mut out := []
      for sg in gs do
        let mut g := sg.mvarId
        if isEx then
          if let some (Expr.fvar w) := (sg.fields[0]? : Option Expr) then
            g ← g.rename w (← mkFreshUserName (Name.mkSimple witnessPrefix))
        out := out ++ [g]
      replaceMainGoal out
      -- each case of a disjunction is a goal of its own: split them all
      let gs ← getGoals
      let mut done := []
      for g in gs do
        setGoals [g]
        splitHyps
        done := done ++ (← getGoals)
      setGoals done
      return

/-- The witnesses `splitHyps` named. -/
def witnesses : TacticM (Array Expr) := withMainContext do
  let mut out := #[]
  for ldecl in (← getLCtx) do
    if ldecl.isImplementationDetail then continue
    if !(ldecl.userName.toString.startsWith witnessPrefix) then continue
    if (← instantiateMVars ldecl.type) == mkConst `Int then out := out.push ldecl.toExpr
  return out

/-- The subterms `f a₁ … aₙ` of `e` whose head `f` is a local, with no loose bound variables. -/
partial def localApps (e : Expr) (acc : Array Expr := #[]) : Array Expr :=
  let acc := if e.isApp && e.getAppFn.isFVar && !e.hasLooseBVars then acc.push e else acc
  match e with
  | .app f a => localApps a (localApps f acc)
  | .lam _ t b _ | .forallE _ t b _ => localApps b (localApps t acc)
  | .letE _ t v b _ => localApps b (localApps v (localApps t acc))
  | .mdata _ b | .proj _ _ b => localApps b acc
  | _ => acc

/-- The triggers of a quantifier body for its variable `x` (the bound variable `depth`): the local
functions `f` the body applies to `x`, with the arity and the argument position. -/
partial def triggers (e : Expr) (depth : Nat := 0) (acc : Array (FVarId × Nat × Nat) := #[]) :
    Array (FVarId × Nat × Nat) :=
  let acc := if e.isApp && e.getAppFn.isFVar then
      let args := e.getAppArgs
      (List.range args.size).foldl (init := acc) fun acc i =>
        if args[i]! == .bvar depth then acc.push (e.getAppFn.fvarId!, args.size, i) else acc
    else acc
  match e with
  | .app f a => triggers a depth (triggers f depth acc)
  | .lam _ t b _ | .forallE _ t b _ => triggers b (depth + 1) (triggers t depth acc)
  | .letE _ t v b _ => triggers b (depth + 1) (triggers v depth (triggers t depth acc))
  | .mdata _ b | .proj _ _ b => triggers b depth acc
  | _ => acc

/-- Instantiates each hypothesis `∀ x : Int, P x` at each witness in `ws`, and at each `t` such
that `P` applies a local function `f` to `x` and the context applies `f` to `t` in the same place
(E-matching on single-variable triggers). Returns whether it added an instance. -/
def instantiate (ws : Array Expr) (seen : Std.HashSet (Expr × Expr)) :
    TacticM (Bool × Std.HashSet (Expr × Expr)) := withMainContext do
  let mut seen := seen
  let mut ground : Array Expr := #[]
  for ldecl in (← getLCtx) do
    if !ldecl.isImplementationDetail then
      ground := localApps (← instantiateMVars ldecl.type) ground
  let hyps := (← getLCtx).foldl (init := #[]) fun acc d =>
    if d.isImplementationDetail then acc else acc.push d
  let mut added := false
  for ldecl in hyps do
    let ty ← instantiateMVars ldecl.type
    if let .forallE _ d body _ := ty then
      if d == mkConst `Int && (← isProp ty) then
        let mut ts := ws
        for (f, n, i) in triggers body do
          for e in ground do
            if e.getAppFn == .fvar f && e.getAppNumArgs == n then
              let t := e.getArg! i
              if !(ts.contains t) then ts := ts.push t
        for t in ts do
          -- each quantifier once per term, however later steps rewrite its instance
          if seen.contains (ty, t) then continue
          seen := seen.insert (ty, t)
          let v := mkApp ldecl.toExpr t
          let vty ← instantiateMVars (← whnfR (← inferType v))
          -- an instance the context already has adds nothing
          let known := (← getLCtx).any fun d => !d.isImplementationDetail && d.type == vty
          if known then continue
          let g ← getMainGoal
          let (_, g') ← (← g.assert (← mkFreshUserName `inst) vty v).intro1P
          replaceMainGoal [g']
          added := true
  return (added, seen)

/-- Clears the hypotheses with quantifiers, which `omega` cannot use. -/
def clearQuantified : TacticM Unit := withMainContext do
  let mut g ← getMainGoal
  for ldecl in (← getLCtx) do
    if ldecl.isImplementationDetail then continue
    let ty ← instantiateMVars ldecl.type
    if (← isProp ty) && hasQuantifier ty then
      g ← g.tryClear ldecl.fvarId
  replaceMainGoal [g]

/-- The condition of the first `if c then _ else _` in the goal or a hypothesis whose condition
has no loose bound variables. -/
def findIteCond : TacticM (Option Expr) := withMainContext do
  let mut tys := #[← getMainTarget]
  for ldecl in (← getLCtx) do
    if !ldecl.isImplementationDetail then tys := tys.push ldecl.type
  for ty in tys do
    let ty ← instantiateMVars ty
    if let some e := ty.find? (fun e => (e.isAppOfArity `ite 5 || e.isAppOfArity ``SolarVerify.Discharge.qite 4) && !(e.getArg! 1).hasLooseBVars) then
      return some (e.getArg! 1)
  return none

/-- A propositional atom of the goal or a hypothesis: a local proposition (an SMT `Bool`
constant), or a local function applied to integers that returns one (a `Bool` array read), with
no loose bound variables. `omega` reads neither, so the search cases on them. -/
def findPropAtom : TacticM (Option Expr) := withMainContext do
  let mut tys := #[← getMainTarget]
  for ldecl in (← getLCtx) do
    if !ldecl.isImplementationDetail then tys := tys.push (← instantiateMVars ldecl.type)
  for ty in tys do
    for e in localApps ty do
      if !(← whnfR (← inferType e)).isForall && (← isProp e) then return some e
  for ldecl in (← getLCtx) do
    if ldecl.isImplementationDetail then continue
    if (← instantiateMVars ldecl.type).isProp then
      for ty in tys do
        if ty.containsFVar ldecl.fvarId then return some ldecl.toExpr
  return none

/-- The applications `f a₁ … aₙ` (`n ≤ 3`, not partial) of a local function `f` to integers in the
goal and the hypotheses, grouped by `f`. -/
def applications : TacticM (Std.HashMap FVarId (Array (Array Expr))) := withMainContext do
  let mut out : Std.HashMap FVarId (Array (Array Expr)) := {}
  let mut tys := #[← getMainTarget]
  for ldecl in (← getLCtx) do
    if !ldecl.isImplementationDetail then tys := tys.push (← instantiateMVars ldecl.type)
  for ty in tys do
    for e in localApps ty do
      let f := e.getAppFn.fvarId!
      let args := e.getAppArgs
      if args.size == 0 || args.size > 3 then continue
      if (← whnfR (← inferType e)).isForall then continue
      let mut ok := true
      for a in args do
        if (← inferType a) != mkConst `Int then ok := false
      if !ok then continue
      let prev := out.getD f #[]
      if !(prev.any (· == args)) then out := out.insert f (prev.push args)
  return out

/-- Adds, for each two applications of the same local function, that they are equal or their
arguments differ. -/
def congruence : TacticM Unit := withMainContext do
  for (f, apps) in (← applications) do
    for i in [0:apps.size] do
      for j in [i+1:apps.size] do
        let (a, b) := (apps[i]!, apps[j]!)
        let lemma := match a.size with
          | 1 => `SolarVerify.Discharge.congr1
          | 2 => `SolarVerify.Discharge.congr2
          | _ => `SolarVerify.Discharge.congr3
        let pf ← mkAppM lemma (#[mkFVar f] ++ a ++ b)
        let g ← getMainGoal
        let (_, g') ← (← g.assert (← mkFreshUserName `cong) (← inferType pf) pf).intro1P
        replaceMainGoal [g']

end SolarVerify.Discharge

open SolarVerify.Discharge

/-- For each two applications of the same array or function, adds that they are equal or their
arguments differ. -/
elab "vc_congr" : tactic => congruence

/-- Substitutes away the definitions `x = t` and `g ↔ φ` of local symbols. -/
elab "vc_defs" : tactic => do
  iffToEq
  evalTactic (← `(tactic| subst_vars))

/-- Pushes negations in, turns implications into disjunctions and Booleans into propositions,
and folds constants. -/
macro "vc_norm" : tactic => `(tactic| simp -failIfUnchanged only [Classical.not_forall, not_exists, not_imp,
  Classical.not_and_iff_not_or_not, not_or, SolarVerify.Discharge.imp_iff_not_or, Classical.not_not, ge_iff_le,
  gt_iff_lt, true_and, and_true, false_and, and_false, true_or, or_true, false_or, or_false,
  not_true_eq_false, not_false_eq_true, eq_self_iff_true, iff_self, true_implies, false_implies,
  implies_true, Bool.and_eq_true, Bool.or_eq_true, Bool.not_eq_true', Bool.not_eq_eq_eq_not,
  Bool.not_true, Bool.not_false, decide_eq_true_eq, SolarVerify.Discharge.decide_any_eq_true, SolarVerify.Discharge.qdec_eq_true, SolarVerify.Discharge.ite_eq_qite, SolarVerify.Discharge.qite_true, SolarVerify.Discharge.qite_false, Bool.not_eq_false, Bool.and_eq_false_iff, Bool.or_eq_false_iff, decide_eq_false_iff_not, Bool.and_true,
  Bool.true_and, Bool.and_false, Bool.false_and, Bool.or_true, Bool.true_or, Bool.or_false,
  Bool.false_or, Bool.true_eq_false, Bool.false_eq_true, SolarVerify.Discharge.true_eq_iff,
  SolarVerify.Discharge.false_eq_iff] at *)

/-- Splits conjunctions and existentials among the hypotheses. -/
elab "vc_split_hyps" : tactic => splitHyps

namespace SolarVerify.Discharge

/-- Simplifies the goal and every hypothesis but the last one (the case hypothesis just
introduced, which the simp set rewrites with) by `stx`. Leaving the case hypothesis alone keeps
it: rewriting it with an earlier copy of itself would turn it into `True`. -/
def simpCase (stx : Syntax) : TacticM Unit := withMainContext do
  let some h := (← getLCtx).lastDecl | throwError "no case hypothesis"
  let { ctx, simprocs, .. } ← mkSimpContext stx (eraseLocal := false)
  let mut fvars := #[]
  for ldecl in (← getLCtx) do
    if ldecl.isImplementationDetail || ldecl.fvarId == h.fvarId then continue
    if ← isProp ldecl.type then fvars := fvars.push ldecl.fvarId
  match ← simpGoal (← getMainGoal) ctx simprocs (fvarIdsToSimp := fvars) with
  | (none, _) => replaceMainGoal []
  | (some (_, g), _) => replaceMainGoal [g]

/-- Proves a condition from the context without splitting on it: a conjunction part by part, a
`∀ x : Int` by instantiating every quantified hypothesis at a fresh `x`, an `∃ x : Int` at a
witness or an integer the context applies a function to, and the rest by `assumption` or
`omega`. A condition that guards a loop's next state restates the invariant the context already
has; proving it avoids a split whose negative case unfolds every quantifier again. -/
partial def provePos (fuel : Nat) : TacticM Unit := withMainContext do
  if fuel = 0 then throwError "vc_prove: depth exhausted"
  let t ← whnfR (← instantiateMVars (← getMainTarget))
  if t.isAppOfArity ``And 2 then
    evalTactic (← `(tactic| constructor))
    let gs ← getGoals
    for g in gs do
      setGoals [g]
      provePos (fuel - 1)
    setGoals []
  else if t.isForall && t.bindingDomain! == mkConst ``Int && !(← isProp t.bindingDomain!) then
    let (x, g) ← (← getMainGoal).intro1P
    replaceMainGoal [g]
    withMainContext do
      let mut g ← getMainGoal
      for ldecl in (← getLCtx) do
        if ldecl.isImplementationDetail then continue
        let ty ← instantiateMVars ldecl.type
        if let .forallE _ d _ _ := ty then
          if d == mkConst ``Int && (← isProp ty) then
            let v := mkApp ldecl.toExpr (.fvar x)
            let vty ← instantiateMVars (← whnfR (← inferType v))
            let (_, g') ← (← g.assert (← mkFreshUserName `inst) vty v).intro1P
            g := g'
      replaceMainGoal [g]
    evalTactic (← `(tactic| omega))
  else if t.isAppOfArity ``Exists 2 && (t.getArg! 0) == mkConst ``Int then
    let mut cands := (← witnesses)
    for ldecl in (← getLCtx) do
      if ldecl.isImplementationDetail then continue
      for e in localApps (← instantiateMVars ldecl.type) do
        for a in e.getAppArgs do
          if !a.hasLooseBVars && (← inferType a) == mkConst ``Int && !cands.contains a then
            cands := cands.push a
    for c in cands do
      let s ← saveState
      try
        let cStx ← Term.exprToSyntax c
        evalTactic (← `(tactic| refine ⟨$cStx, ?_⟩))
        provePos (fuel - 1)
        return
      catch _ => s.restore
    throwError "vc_prove: no witness"
  else
    evalTactic (← `(tactic| first | assumption | omega))

/-- Proves the goal with `provePos`. -/
elab "vc_prove_pos" : tactic => provePos 8

/-- Splits on the condition of one `if`, simplifying each case; fails when there is none. -/
def splitIte : TacticM Unit := do
  let some c ← findIteCond | throwError "no conditional"
  let hcI := mkIdent (← mkFreshUserName `hc)
  let cStx ← Term.exprToSyntax c
  -- a quantified condition the context implies: take the case where it holds, and no other
  if hasQuantifier c then
    let s ← saveState
    try
      evalTactic (← `(tactic| have $hcI : $cStx := by vc_prove_pos))
      simpCase (← `(tactic| simp only [↓reduceIte, SolarVerify.Discharge.ite_pos $hcI, SolarVerify.Discharge.qite_pos $hcI]))
      return
    catch _ => s.restore
  evalTactic (← `(tactic| by_cases $hcI : $cStx))
  let gs ← getGoals
  let mut out := []
  -- `by_cases` leaves the case where `c` holds first
  for (g, pos) in gs.zip [true, false] do
    setGoals [g]
    if pos then
      simpCase (← `(tactic| simp only [↓reduceIte, SolarVerify.Discharge.ite_pos $hcI, SolarVerify.Discharge.qite_pos $hcI]))
    else
      simpCase (← `(tactic| simp only [↓reduceIte, SolarVerify.Discharge.ite_neg $hcI, SolarVerify.Discharge.qite_neg $hcI]))
    out := out ++ (← getGoals)
  setGoals out

/-- Cases on the propositional atom `p`, rewriting it to `True` or `False` everywhere. -/
def splitProp (p : Expr) : TacticM Unit := do
  let hpI := mkIdent (← mkFreshUserName `hp)
  let pStx ← Term.exprToSyntax p
  evalTactic (← `(tactic| by_cases $hpI : $pStx))
  let gs ← getGoals
  let mut out := []
  for g in gs do
    setGoals [g]
    simpCase (← `(tactic| simp only [$hpI:ident]))
    out := out ++ (← getGoals)
  setGoals out

/-- The search: on each goal, split hypotheses, instantiate quantifiers, and split one
conditional, until nothing changes; then clear the quantified hypotheses, add congruence
facts and run `omega`. `fuel` bounds the depth, so a hopeless goal fails rather than loops. -/
partial def search (seen : Std.HashSet (Expr × Expr)) (fuel : Nat) : TacticM Unit := do
  if fuel = 0 then throwError "vc_auto: search depth exhausted"
  for g in (← getGoals) do
    setGoals [g]
    evalTactic (← `(tactic| vc_norm))
    if (← getGoals).isEmpty then continue
    splitHyps
    let gs ← getGoals
    for g in gs do
      setGoals [g]
      -- what was instantiated is per branch: a sibling's instances are not in this context
      let (added, seen) ← instantiate (← witnesses) seen
      if added then
        search seen (fuel - 1)
      else if (← findIteCond).isSome then
        splitIte
        search seen (fuel - 1)
      else if let some p ← findPropAtom then
        splitProp p
        search seen (fuel - 1)
      else
        clearQuantified
        congruence
        evalTactic (← `(tactic| omega))
  setGoals []

end SolarVerify.Discharge

/-- Closes a shallow obligation whose hypotheses are still in the goal: see the module comment. -/
elab "vc_search" : tactic => do
  evalTactic (← `(tactic| intros))
  SolarVerify.Discharge.iffToEq
  evalTactic (← `(tactic| subst_vars))
  SolarVerify.Discharge.search {} 32
