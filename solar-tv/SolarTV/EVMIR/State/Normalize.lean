import SolarTV.EVMIR.State.Interpreter
import Lean

/-! Proof-producing execution at a known bytecode position. Symbolic data must not
cause the simplifier to unfold every remaining unit of global work fuel. Reduce
one known instruction and use its proved equation, leaving unknown control flow
opaque. This produces kernel-checkable terms, not native evaluation evidence. -/

namespace SolarTV.EVM.State
open Lean Meta Simp

simproc_decl reduceRun (run _ _ _ _) := fun expression => do
  let args := expression.getAppArgs
  let some fuel ← getNatValue? args[0]! | return .continue
  if fuel = 0 then return .done {expr := ← whnf expression}
  let frame ← whnf args[3]!
  unless frame.isAppOf ``Frame.mk do return .continue
  let step ← mkAppM ``instruction #[args[1]!,args[2]!,args[3]!]
  let reduction ← simp step
  let reduced ← whnf reduction.expr
  let proof ← reduction.getProof
  let parameters := #[toExpr (fuel - 1),args[1]!,args[2]!,args[3]!]
  let equation ← if reduced.isAppOf ``Except.error then
      pure (mkAppN (mkConst ``run_error) (parameters ++ #[reduced.appArg!,proof]))
    else if reduced.isAppOf ``Except.ok then
      let signal ← whnf reduced.appArg!
      if signal.isAppOf ``Step.next then
        pure (mkAppN (mkConst ``run_next) (parameters ++ signal.getAppArgs ++ #[proof]))
      else if signal.isAppOf ``Step.done then
        pure (mkAppN (mkConst ``run_done) (parameters ++ #[signal.appArg!,proof]))
      else return .continue
    else return .continue
  return .visit {expr := (← inferType equation).appArg!,proof? := some equation}

end SolarTV.EVM.State
