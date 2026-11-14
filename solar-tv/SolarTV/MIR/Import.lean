import SolarTV.MIR.State.Interpreter

/-! Observation projection for independently imported pure scalar MIR.
Unsupported execution and non-scalar returns do not become program outcomes.
This projection is admitted only by the scalar importer's no-memory/no-effects
fragment; it is not a justification for erasing stateful function effects. -/

namespace SolarTV.MIR.State

def scalarOutcome (width : Nat) (execution : Run) : Option (Outcome (BitVec width)) :=
  match execution with
  | .error _ => none
  | .ok result => match result.completion with
    | .returned [word] => some (.returned (word.setWidth width))
    | .reverted data => some (.reverted (RevertData.fromBytes data))
    | .invalid => some .invalid
    | _ => none

end SolarTV.MIR.State
