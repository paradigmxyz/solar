import Lean
/-!
A leaf for the discharge search: hypotheses without free variables are decided by kernel
evaluation.

After `vc_valid_prep`, a hypothesis built only from literals (such as one about
`(511#256 &&& 255#256).toNat`) still carries the casts `Val.as` leaves at bit-vector sorts and
`List.foldl` from n-ary operators, which `omega` cannot read. Their value needs no search: the
kernel computes it, reducing the casts over proofs of `s = s` as it reduces any `Eq.rec` whose
indices agree.

`vc_ground` looks for a closed hypothesis `h : P` whose negation the kernel decides:
`decide (¬P)` must be definitionally `true` to the kernel itself (`Kernel.isDefEq`, not the
elaborator's reduction). It then closes the goal with `absurd h (of_decide_eq_true rfl)`, which
the kernel checks again when the theorem is added. Nothing runs natively and nothing joins the trusted base.
It fails, leaving the goal to the rest of the search, when no closed hypothesis is false.
-/
namespace SolarVerify.Sat
open Lean Elab Tactic Meta

/-- A `Decidable` instance the kernel reduces, built along `¬`, `∧`, `∨` and `=`; an equation
is decided by the instance of its type's weak head normal form, so that `Srt.Den .bool` reads as
`Bool`. -/
partial def mkDecidable (p : Expr) : MetaM (Option Expr) := do
  match_expr p with
  | Not q =>
    let some d ← mkDecidable q | return none
    return some (mkApp2 (mkConst ``instDecidableNot) q d)
  | And a b =>
    let some da ← mkDecidable a | return none
    let some db ← mkDecidable b | return none
    return some (mkApp4 (mkConst ``instDecidableAnd) a b da db)
  | Or a b =>
    let some da ← mkDecidable a | return none
    let some db ← mkDecidable b | return none
    return some (mkApp4 (mkConst ``instDecidableOr) a b da db)
  | Eq ty a b =>
    let ty' ← whnfD ty
    let some u ← (try pure (some (← getLevel ty')) catch _ => pure none) | return none
    let some inst ← (try synthInstance? (mkApp (mkConst ``DecidableEq [u]) ty') catch _ => pure none)
      | return none
    return some (mkApp2 inst a b)
  | _ => try synthInstance? (mkApp (mkConst ``Decidable) p) catch _ => pure none

/-- `absurd h (of_decide_eq_true rfl)` for a closed hypothesis `h : P` that the kernel decides
false, or none. -/
def refuteClosed (goalTy : Expr) (d : LocalDecl) : MetaM (Option Expr) := do
  let ty ← instantiateMVars d.type
  if ty.hasFVar || ty.hasMVar || ty.hasLooseBVars then return none
  unless ← isProp ty do return none
  let notTy := mkNot ty
  let some inst ← mkDecidable notTy | return none
  let inst ← instantiateMVars inst
  let dec := mkApp2 (mkConst ``Decidable.decide) notTy inst
  let ok ← try ofExceptKernelException (Kernel.isDefEq (← getEnv) {} dec (mkConst ``Bool.true))
    catch _ => pure false
  unless ok do return none
  let rflTrue := mkApp2 (mkConst ``Eq.refl [1]) (mkConst ``Bool) (mkConst ``Bool.true)
  let hn := mkApp3 (mkConst ``of_decide_eq_true) notTy inst rflTrue
  return some (mkApp4 (mkConst ``absurd [← getLevel goalTy]) ty goalTy d.toExpr hn)

elab "vc_ground" : tactic => withMainContext do
  let g ← getMainGoal
  let goalTy ← g.getType
  for d in ← getLCtx do
    if d.isImplementationDetail then continue
    if let some p ← refuteClosed goalTy d then
      g.assign p
      replaceMainGoal []
      return
  throwError "vc_ground: no closed hypothesis is false"

end SolarVerify.Sat
