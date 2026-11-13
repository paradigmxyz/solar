/-
`vc_valid` proves an obligation `solar-verify --emit-lean` exported: `Obligation.Valid ob`, for
every interpretation `env` of the declared symbols.

1. It unfolds the obligation's definitions and evaluates `Term.eval` symbolically: on the
   exported terms every sort check decides and every `Option` is `some`, so the formula becomes a
   Boolean expression over the symbols' values `env.fn f [] s ()` (and `env.divZero`,
   `env.modZero`). A formula that does not reduce so, such as an ill-sorted one, leaves the proof
   stuck: it fails, and is never vacuous.
2. It generalizes each such value to a variable of its sort's Lean type (`Int`, `Bool`,
   `Int → Int`, …), turns the Boolean formula into propositions, and assumes the guard and the
   script's assertions.
3. The search of `SolarVerify.Discharge.Search` closes what is left.

Every step builds a proof term the kernel checks; nothing here is trusted.
-/
import Lean
import SolarVerify.Vc
import SolarVerify.Discharge.Search

open Lean Meta Elab Tactic

namespace SolarVerify.Discharge

open SolarVerify.Smt SolarVerify.Vc

/-- An SMT `ite` of one sort evaluates to a value of that sort. -/
theorem ite_val (s : Srt) (c : Bool) (a b : s.Den) :
    (if c = true then (⟨s, a⟩ : Val) else ⟨s, b⟩) = ⟨s, if c = true then a else b⟩ := by
  cases c <;> rfl

/-! Binders: a quantifier over the values of a list of sorts becomes one quantifier per sort,
over the sort's Lean type. -/

theorem forall_denList_nil {P : Srt.DenList [] → Prop} : (∀ xs, P xs) ↔ P () :=
  ⟨fun h => h (), fun h _ => h⟩

theorem forall_denList_cons {s : Srt} {ss : List Srt} {P : Srt.DenList (s :: ss) → Prop} :
    (∀ xs, P xs) ↔ ∀ (x : s.Den) (xs : Srt.DenList ss), P (x, xs) :=
  ⟨fun h x xs => h (x, xs), fun h p => h p.1 p.2⟩

theorem exists_denList_nil {P : Srt.DenList [] → Prop} : (∃ xs, P xs) ↔ P () :=
  ⟨fun ⟨_, h⟩ => h, fun h => ⟨(), h⟩⟩

theorem exists_denList_cons {s : Srt} {ss : List Srt} {P : Srt.DenList (s :: ss) → Prop} :
    (∃ xs, P xs) ↔ ∃ (x : s.Den) (xs : Srt.DenList ss), P (x, xs) :=
  ⟨fun ⟨p, h⟩ => ⟨p.1, p.2, h⟩, fun ⟨x, xs, h⟩ => ⟨(x, xs), h⟩⟩

theorem forall_den_int {P : Srt.Den .int → Prop} : (∀ x, P x) ↔ ∀ x : Int, P x := Iff.rfl
theorem forall_den_bool {P : Srt.Den .bool → Prop} : (∀ x, P x) ↔ ∀ x : Bool, P x := Iff.rfl
theorem exists_den_int {P : Srt.Den .int → Prop} : (∃ x, P x) ↔ ∃ x : Int, P x := Iff.rfl
theorem exists_den_bool {P : Srt.Den .bool → Prop} : (∃ x, P x) ↔ ∃ x : Bool, P x := Iff.rfl



/-- Extensional equality of arrays, as a `qdec` `simp` can rewrite inside. -/
theorem beq_array (i e : Srt) (a b : Srt.Den (.array i e)) :
    Srt.beq (.array i e) a b = qdec (∀ k : Srt.Den i, Srt.beq e (a k) (b k) = true) := by
  rw [Srt.beq]; rfl

/-! Quantifiers over one integer, the only ones the exporter emits so far: the binder becomes a
Lean `∀ x : Int` (`∃`), with the bound value at the head of the locals. -/

theorem eval_forall_int (env : Env) (decls : List Decl) (locals : Locals) (body : SolarVerify.Smt.Term) :
    Term.eval env decls locals (.forall [.int] body) =
      qite (∀ x : Int, (boolOf (Term.eval env decls (⟨.int, x⟩ :: locals) body)).isSome)
        (some ⟨.bool, qdec (∀ x : Int,
          boolOf (Term.eval env decls (⟨.int, x⟩ :: locals) body) = some true)⟩)
        none := by
  rw [Term.eval]
  have h1 : (∀ xs : Srt.DenList [.int],
      (boolOf (Term.eval env decls (bindAll [.int] xs locals) body)).isSome) =
      ∀ x : Int, (boolOf (Term.eval env decls (⟨.int, x⟩ :: locals) body)).isSome :=
    propext ⟨fun h x => h (x, ()), fun h p => h p.1⟩
  have h2 : (∀ xs : Srt.DenList [.int],
      boolOf (Term.eval env decls (bindAll [.int] xs locals) body) = some true) =
      ∀ x : Int, boolOf (Term.eval env decls (⟨.int, x⟩ :: locals) body) = some true :=
    propext ⟨fun h x => h (x, ()), fun h p => h p.1⟩
  rw [h1, h2]; rfl

theorem eval_exists_int (env : Env) (decls : List Decl) (locals : Locals) (body : SolarVerify.Smt.Term) :
    Term.eval env decls locals (.exists [.int] body) =
      qite (∀ x : Int, (boolOf (Term.eval env decls (⟨.int, x⟩ :: locals) body)).isSome)
        (some ⟨.bool, qdec (∃ x : Int,
          boolOf (Term.eval env decls (⟨.int, x⟩ :: locals) body) = some true)⟩)
        none := by
  rw [Term.eval]
  have h1 : (∀ xs : Srt.DenList [.int],
      (boolOf (Term.eval env decls (bindAll [.int] xs locals) body)).isSome) =
      ∀ x : Int, (boolOf (Term.eval env decls (⟨.int, x⟩ :: locals) body)).isSome :=
    propext ⟨fun h x => h (x, ()), fun h p => h p.1⟩
  have h2 : (∃ xs : Srt.DenList [.int],
      boolOf (Term.eval env decls (bindAll [.int] xs locals) body) = some true) =
      ∃ x : Int, boolOf (Term.eval env decls (⟨.int, x⟩ :: locals) body) = some true :=
    propext ⟨fun ⟨p, h⟩ => ⟨p.1, h⟩, fun ⟨x, h⟩ => ⟨(x, ()), h⟩⟩
  rw [h1, h2]; rfl
/-- A value's view at its own sort. -/
theorem as_self (s : Srt) (x : s.Den) : Val.as ⟨s, x⟩ s = some x := by
  simp [Val.as]

theorem as_int (x : Int) : Val.as ⟨.int, x⟩ .int = some x := rfl

theorem as_bool (x : Bool) : Val.as ⟨.bool, x⟩ .bool = some x := rfl

/-- A sort check that holds by reflexivity, at any instance. -/
theorem dite_self {α : Sort u} {s : Srt} {_ : Decidable (s = s)} (t : s = s → α) (e : ¬s = s → α) :
    dite (s = s) t e = t rfl := by
  simp

/-! A Boolean value as a value of sort `bool`: the same type, though not syntactically. -/

theorem isSome_some_den (b : Srt.Den .bool) : @Option.isSome Bool (@some (Srt.Den .bool) b) = true :=
  rfl

theorem some_den_eq_some_true (b : Srt.Den .bool) :
    (@Eq (Option Bool) (@some (Srt.Den .bool) b) (some true)) = ((b : Bool) = true) :=
  propext (Option.some_inj (α := Bool))

theorem holds_of {b : Bool} (h : b = true) : (some b : Option Bool) = some true := by
  rw [h]

theorem of_imp {a b : Bool} (h : a = true → b = true) : (!a || b) = true := by
  cases a <;> cases b <;> simp_all

theorem of_imp_and {a b : Bool} (h : a = true → b = true) : (!(a && true) || b) = true := by
  cases a <;> cases b <;> simp_all

/-- Unfolds the definitions of the exported module (namespace `SolarVerifyVc`) in the goal. -/
def unfoldExported : TacticM Unit := withMainContext do
  let g ← getMainGoal
  let t ← instantiateMVars (← g.getType)
  let t' ← deltaExpand t (fun n => n.getRoot == `SolarVerifyVc)
  replaceMainGoal [← g.replaceTargetDefEq t']

/-- Replaces `Srt.Den s` by the Lean type it is for each concrete sort `s` in `e` (`Int`, `Bool`,
functions for arrays), and a classical `decide` by `qdec`. The result is definitionally equal; `simp` needs it, since it matches
`Bool` against `Srt.Den .bool` only up to reducible definitions. -/
partial def normDen (e : Expr) : MetaM Expr :=
  Meta.transform e (post := fun e => do
    if e.isAppOfArity ``SolarVerify.Smt.Srt.Den 1 && !(e.getArg! 0).hasLooseBVars && !(e.getArg! 0).hasFVar then
      let r ← whnfD e
      if r != e then return .done (← normDen r)
    -- a classical `decide` whose instance names its proposition before `simp` rewrote it
    if e.isAppOfArity ``Decidable.decide 2 && (e.getArg! 1).isAppOfArity ``Classical.propDecidable 1 then
      return .done (mkApp (mkConst ``SolarVerify.Discharge.qdec) (e.getArg! 0))
    return .continue)

/-- The subterms of `e` that are values of `env`'s symbols: `env.fn f [] s ()`, `env.divZero` and
`env.modZero`, with no loose bound variables. -/
partial def envAtoms (env : Expr) (e : Expr) (acc : Array Expr := #[]) : Array Expr :=
  let isAtom :=
    !e.hasLooseBVars &&
    ((e.isAppOfArity ``SolarVerify.Smt.Env.fn 5 && e.getArg! 0 == env && (e.getArg! 2).isAppOfArity `List.nil 1) ||
     ((e.isAppOfArity ``SolarVerify.Smt.Env.divZero 1 || e.isAppOfArity ``SolarVerify.Smt.Env.modZero 1) && e.getArg! 0 == env))
  if isAtom then (if acc.contains e then acc else acc.push e) else
  match e with
  | .app f a => envAtoms env a (envAtoms env f acc)
  | .lam _ t b _ | .forallE _ t b _ => envAtoms env b (envAtoms env t acc)
  | .letE _ t v b _ => envAtoms env b (envAtoms env v (envAtoms env t acc))
  | .mdata _ b | .proj _ _ b => envAtoms env b acc
  | _ => acc

/-- Replaces each value of a symbol in the goal by a variable of its sort's Lean type, universally
quantified: the goal becomes `∀ x, G[x]`, which implies `G[env.fn f [] s ()]`. -/
def generalizeAtoms (env : Expr) : TacticM Unit := withMainContext do
  let g ← getMainGoal
  let t ← instantiateMVars (← g.getType)
  let atoms := envAtoms env t
  let mut binders : Array (Name × Expr) := #[]
  for (e, i) in atoms.zipIdx do
    let ty ← normDen (← Meta.reduce (← inferType e) (skipTypes := false))
    let k := if e.getAppNumArgs ≥ 2 then (e.getArg! 1).nat? else none
    binders := binders.push (Name.mkSimple (match k with | some k => s!"x{k}" | none => s!"z{i}"), ty)
  let newTy ← withLocalDeclsD (binders.map fun (n, ty) => (n, fun _ => pure ty)) fun xs => do
    let mut b := t
    for (x, e) in xs.zip atoms do
      b := (← kabstract b e).instantiate1 x
    normDen (← mkForallFVars xs b)
  let ng ← mkFreshExprSyntheticOpaqueMVar newTy
  g.assign (mkAppN ng atoms)
  replaceMainGoal [ng.mvarId!]

end SolarVerify.Discharge

/-- Runs the tactic unless an earlier step already closed the goal. -/
def SolarVerify.Discharge.step (tac : Syntax) : TacticM Unit := do
  unless (← getGoals).isEmpty do evalTactic tac

/-- Evaluates the exported terms symbolically: see the module comment. -/
macro "vc_eval" : tactic => `(tactic| simp -failIfUnchanged only [SolarVerify.Vc.Obligation.Valid, SolarVerify.Vc.Obligation.formula,
    SolarVerify.Smt.Holds, ↓SolarVerify.Discharge.eval_forall_int,
    ↓SolarVerify.Discharge.eval_exists_int, SolarVerify.Smt.Term.eval, SolarVerify.Smt.Term.evalList,
    SolarVerify.Smt.Op.apply, SolarVerify.Smt.boolOf, SolarVerify.Smt.bools, SolarVerify.Smt.ints,
    SolarVerify.Smt.Val.asList, SolarVerify.Smt.intRel, SolarVerify.Smt.chain,
    SolarVerify.Smt.impliesChain, SolarVerify.Smt.foldl2, SolarVerify.Smt.sameSort,
    SolarVerify.Smt.pairwise, SolarVerify.Smt.intDiv, SolarVerify.Smt.intMod, Option.bind,
    ↓SolarVerify.Discharge.dite_self, ↓SolarVerify.Discharge.beq_array, ↓SolarVerify.Discharge.as_int, ↓SolarVerify.Discharge.as_bool, SolarVerify.Smt.Val.as,
    List.take, List.cons_append, List.nil_append, List.all, List.any,
    List.reverse, List.reverseAux, List.foldl, id, bind, pure, List.getElem?_cons_succ,
    List.getElem?_cons_zero, SolarVerify.Smt.Srt.beq, ↓reduceDIte, ↓reduceIte, Option.some.injEq, SolarVerify.Discharge.ite_val,
    SolarVerify.Smt.bindAll, SolarVerify.Smt.Srt.unpack, List.reverse_cons, List.reverse_nil,
    List.singleton_append, SolarVerify.Discharge.forall_denList_nil,
    SolarVerify.Discharge.forall_denList_cons, SolarVerify.Discharge.exists_denList_nil,
    SolarVerify.Discharge.exists_denList_cons, SolarVerify.Discharge.forall_den_int,
    SolarVerify.Discharge.forall_den_bool, SolarVerify.Discharge.exists_den_int,
    SolarVerify.Discharge.exists_den_bool, SolarVerify.Discharge.decide_any_eq_true,
    Option.isSome_some, implies_true, SolarVerify.Discharge.qite_true,
    SolarVerify.Discharge.qdec_eq_true, eq_self_iff_true, dite_true,
    SolarVerify.Discharge.isSome_some_den, SolarVerify.Discharge.some_den_eq_some_true,
    Bool.and_true, Bool.not_not])

/-- Rewrites the goal with `normDen`. -/
def SolarVerify.Discharge.normDenGoal : TacticM Unit := withMainContext do
  let g ← getMainGoal
  let t ← instantiateMVars (← g.getType)
  replaceMainGoal [← g.replaceTargetDefEq (← normDen t)]

open SolarVerify.Discharge in
/-- Reduces `Obligation.Valid ob` to a shallow goal `False` under its hypotheses. -/
elab "vc_valid_prep" : tactic => do
  unfoldExported
  evalTactic (← `(tactic| unfold SolarVerify.Vc.Obligation.Valid))
  let (envFv, g) ← (← getMainGoal).intro `env
  replaceMainGoal [g]
  evalTactic (← `(tactic| vc_eval))
  unless (← getGoals).isEmpty do normDenGoal
  step (← `(tactic| vc_eval))
  step (← `(tactic| first | refine SolarVerify.Discharge.holds_of ?_ | skip))
  unless (← getGoals).isEmpty do generalizeAtoms (.fvar envFv)
  step (← `(tactic| intros))
  step (← `(tactic| first | apply SolarVerify.Discharge.of_imp_and | apply SolarVerify.Discharge.of_imp))
  step (← `(tactic| intro hyps))
  step (← `(tactic| simp -failIfUnchanged only [Bool.and_eq_true, Bool.or_eq_true, Bool.not_eq_true',
    SolarVerify.Discharge.decide_any_eq_true, SolarVerify.Discharge.qdec_eq_true, decide_eq_true_eq,
    decide_eq_false_iff_not, Bool.and_true, Bool.true_and] at *))
  step (← `(tactic| refine Classical.byContradiction ?_))
  step (← `(tactic| intro))
  step (← `(tactic| vc_split_hyps))

/-- Proves `Obligation.Valid ob` for an exported obligation `ob`: see the module comment. -/
macro "vc_valid" : tactic => `(tactic| (vc_valid_prep <;> vc_search))
