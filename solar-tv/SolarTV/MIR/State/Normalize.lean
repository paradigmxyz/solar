import SolarTV.MIR.State.Interpreter
import Lean

/-! Proof-producing execution of one known MIR control position at a time.
The simplifier must not unfold recursive execution under an unknown continuation:
that expands the work budget rather than the program's actual path. Each rewrite
uses a proved transition equation and kernel-checkable reduction of that finite
step. Symbolic branches expose one layer; unknown control positions stay opaque.
No VM/native evaluation, rewrite-name recognition or accepted-result axiom is used. -/

namespace SolarTV.MIR.State
open Lean Meta Simp

simproc_decl reduceRun (run _ _ _ _) := fun expression => do
  let args := expression.getAppArgs
  let some fuel ← getNatValue? args[0]! | return .continue
  if fuel = 0 then return .done {expr := ← whnf expression}
  let machine ← whnf args[3]!
  unless machine.isAppOf ``Machine.mk do return .continue
  let cursor ← whnf machine.getAppArgs[0]!
  unless cursor.isAppOf ``Cursor.mk do return .continue
  for index in [0,1,2] do
    unless (← getNatValue? (← withTransparency .default (whnf cursor.getAppArgs[index]!))).isSome do return .continue
  let step ← mkAppM ``transition #[toExpr (fuel - 1),args[1]!,args[2]!,args[3]!]
  let reduction ← simp step
  let reduced ← whnf reduction.expr
  let proof ← reduction.getProof
  let parameters := #[toExpr (fuel - 1),args[1]!,args[2]!,args[3]!]
  let equation ← if reduced.isAppOf ``Except.error then
      pure (mkAppN (mkConst ``run_error) (parameters ++ #[reduced.appArg!,proof]))
    else if reduced.isAppOf ``Except.ok then
      let signal ← whnf reduced.appArg!
      if signal.isAppOf ``Transition.next then
        pure (mkAppN (mkConst ``run_next) (parameters ++ signal.getAppArgs ++ #[proof]))
      else if signal.isAppOf ``Transition.finished then
        pure (mkAppN (mkConst ``run_finished) (parameters ++ #[signal.appArg!,proof]))
      else mkAppM ``run_one parameters
    else mkAppM ``run_one parameters
  let rhs := (← inferType equation).appArg!
  return .visit {expr := rhs,proof? := some equation}

end SolarTV.MIR.State
