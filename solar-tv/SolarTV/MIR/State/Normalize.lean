import SolarTV.MIR.State.Interpreter
import SolarTV.MIR.State.Observe
import SolarTV.MIR.State.ScalarNormalize
import SolarTV.State.MemoryNormalize
import Lean

/-! Proof-producing execution of one known MIR control position at a time.
The simplifier must not unfold recursive execution under an unknown continuation:
that expands the work budget rather than the program's actual path. Each rewrite
uses a proved transition equation and kernel-checkable reduction of that finite
step. Symbolic branches expose one layer; unknown control positions stay opaque.
No VM/native evaluation, rewrite-name recognition or accepted-result axiom is used. -/

namespace SolarTV.MIR.State
open Lean Meta Simp

-- Resolve a known opcode before traversing its availability predicate, which
-- otherwise expands a fork's entire opcode table under an unknown argument.
attribute [simp↓] Option.any_some Option.any_none

dsimproc_decl reduceFits (Value.fits _ _ _) := fun expression => do
  let args := expression.getAppArgs
  unless args.size == 3 do return .continue
  unless (← getNatValue? args[0]!).isSome do return .continue
  let ty ← whnf args[1]!
  let value ← whnf args[2]!
  unless ty.isAppOf ``ValueType.word || ty.isAppOf ``ValueType.pointer ||
      ty.isAppOf ``ValueType.product || ty.isAppOf ``ValueType.slice do return .continue
  unless value.isAppOf ``Value.scalar || value.isAppOf ``Value.aggregate ||
      value.isAppOf ``Value.undefined do return .continue
  -- Do not unfold a recursive type check under an unapplied list predicate.
  -- Its arguments become known after the list combinator reduces.
  return .visit (← withTransparency .default (whnf expression))

-- Keep the source program shared instead of copying its function table into
-- every symbolic branch. A concrete lookup still reduces by kernel equality.
dsimproc_decl reduceFunction (function _ _) := fun expression => do
  let args := expression.getAppArgs
  unless args.size == 2 do return .continue
  unless (← getNatValue? args[1]!).isSome do return .continue
  let head ← withTransparency .default (whnf expression)
  unless head.isAppOf ``Except.ok || head.isAppOf ``Except.error do return .continue
  return .visit head

-- Observation is a terminal boundary, not a symbolic-execution step. Expanding
-- its projections under an unresolved run duplicates every pending branch.
private def terminalObservation? (execution : Expr) : MetaM (Option Expr) := do
  unless execution.isAppOf ``Except.ok do return none
  let result ← withTransparency .default (whnf execution.appArg!)
  unless result.isAppOfArity ``Result.mk 7 do return none
  let completion ← withTransparency .default (whnf result.getAppArgs[0]!)
  let .const name _ := completion.getAppFn | return none
  let .ctorInfo info ← getConstInfo name | return none
  unless info.induct == ``Completion do return none
  let observation ← mkAppM ``observe #[result]
  return some (← withTransparency .default (whnf observation))

dsimproc_decl reduceEquivalent (equivalent _ _) := fun expression => do
  let args := expression.getAppArgs
  unless args.size == 2 do return .continue
  let left ← withTransparency .default (whnf args[0]!)
  let right ← withTransparency .default (whnf args[1]!)
  if left.isAppOf ``Except.error || right.isAppOf ``Except.error then
    return .visit (mkConst ``False)
  let some left ← terminalObservation? left | return .continue
  let some right ← terminalObservation? right | return .continue
  return .visit (← mkEq left right)

dsimproc_decl reduceConclude (conclude _ _ _) := fun expression => do
  let args := expression.getAppArgs
  unless args.size == 3 do return .continue
  let execution ← withTransparency .default (whnf args[2]!)
  unless execution.isAppOf ``Except.ok || execution.isAppOf ``Except.error do return .continue
  return .visit (← withTransparency .default (whnf expression))

simproc_decl reduceRun (run _ _ _ _) := fun expression => do
  let args := expression.getAppArgs
  unless args.size == 4 do return .continue
  let some fuel ← getNatValue? args[0]! | return .continue
  if fuel = 0 then return .done {expr := ← whnf expression}
  let machine ← whnf args[3]!
  unless machine.isAppOfArity ``Machine.mk 3 do return .continue
  let cursor ← whnf machine.getAppArgs[0]!
  unless cursor.isAppOfArity ``Cursor.mk 6 do return .continue
  let position := cursor.getAppArgs
  unless (← getNatValue? (← withTransparency .default (whnf position[0]!))).isSome do return .continue
  unless (← getNatValue? (← withTransparency .default (whnf position[1]!))).isSome do return .continue
  unless (← getNatValue? (← withTransparency .default (whnf position[2]!))).isSome do return .continue
  let step ← mkAppM ``transition #[toExpr (fuel - 1),args[1]!,args[2]!,args[3]!]
  -- A known constructor needs only definitional equality. This avoids unfolding
  -- instruction/type matches under the transition's unapplied continuations.
  let head ← withTransparency .default (whnf step)
  let reduction ← if head.isAppOf ``Except.ok || head.isAppOf ``Except.error then
      pure {expr := head,proof? := some (← mkEqRefl step) : Simp.Result}
    else simp step
  let reduced ← whnf reduction.expr
  let proof ← reduction.getProof
  let parameters := #[toExpr (fuel - 1),args[1]!,args[2]!,args[3]!]
  let mut resolved := true
  let equation ← if reduced.isAppOf ``Except.error then
      pure (mkAppN (mkConst ``run_error) (parameters ++ #[reduced.appArg!,proof]))
    else if reduced.isAppOf ``Except.ok then
      let signal ← whnf reduced.appArg!
      if signal.isAppOf ``Transition.next then
        pure (mkAppN (mkConst ``run_next) (parameters ++ signal.getAppArgs ++ #[proof]))
      else if signal.isAppOf ``Transition.finished then
        pure (mkAppN (mkConst ``run_finished) (parameters ++ #[signal.appArg!,proof]))
      else do
        resolved := false
        mkAppM ``run_one parameters
    else do
      resolved := false
      mkAppM ``run_one parameters
  let rhs := (← inferType equation).appArg!
  let result := {expr := rhs,proof? := some equation : Simp.Result}
  -- Expose one unresolved step without normalizing both continuations. A later
  -- path split supplies its guard before execution resumes.
  return if resolved then .visit result else .done result

open Lean.Elab.Tactic in
elab "split_mir_guard" : tactic => do
  let goal ← getMainGoal
  goal.withContext do
    let target ← goal.getType
    -- Only the proposition must be closed at this position. Branches may use
    -- the surrounding continuation's bound variables; splitting the closed
    -- proposition remains valid under that binder.
    let predicate := fun e => (e.isAppOfArity ``ite 5 || e.isAppOfArity ``dite 5) &&
      !e.getAppArgs[1]!.hasLooseBVars
    let some first := target.find? predicate | throwError "no unresolved MIR guard"
    let mut term := first
    -- Resolve nested predicates before splitting the interpreter result. The
    -- latter would generalize away the relation between a branch and its frame.
    while (term.getAppArgs[1]!.find? predicate).isSome do
      term := (term.getAppArgs[1]!.find? predicate).get!
    let name ← mkFreshUserName `guard
    let (yes,no) ← goal.byCases term.getAppArgs[1]! name
    replaceMainGoal [yes.mvarId,no.mvarId]
    -- Rewrite the exact guard before arithmetic normalization turns, for
    -- example, `¬ a < b` into `b ≤ a` and obscures the branch predicate.
    evalTactic (← `(tactic| all_goals simp only [$(mkIdent name):term]))

-- A partially applied conditional is a function, not a branch to split.
example : (@ite Nat) = (@ite Nat) := by
  fail_if_success split_mir_guard
  rfl

example (condition : Prop) [Decidable condition] :
    (fun x : Nat => if condition then x else x + 1) =
      (if condition then id else fun x => x + 1) := by
  split_mir_guard <;> simp_all <;> rfl

example (cfg : Config) (functions : Functions) (function block index : Nat) (frame : Frame)
    (result : Run) (known : run 10 cfg functions ⟨⟨function,block,index,true,none,none⟩,[],frame⟩ = result) :
    run 10 cfg functions ⟨⟨function,block,index,true,none,none⟩,[],frame⟩ = result := by
  fail_if_success simp only [↓reduceRun]
  exact known

end SolarTV.MIR.State
