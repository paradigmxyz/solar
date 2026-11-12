import Lean

/-! A fail-closed axiom check for generated proofs.

`#axioms_within thm` fails unless every axiom `thm` depends on is `propext`,
`Classical.choice` or `Quot.sound`, and otherwise reports that it is within them, so that
`#guard_msgs` can pin the check where a generated proof's exact axiom set is not known in
advance (some runs need `Classical.choice`, some do not). `sorryAx`, `Lean.ofReduceBool`
(`native_decide`) and any declared axiom fail it. -/

namespace SolarTV.Milestone.W1

open Lean Elab Command in
/-- Fail unless the constant's axioms are among `propext`, `Classical.choice` and
`Quot.sound`. -/
elab "#axioms_within " id:ident : command => do
  let name ← liftCoreM <| realizeGlobalConstNoOverloadWithInfo id
  let axioms ← liftCoreM <| collectAxioms name
  let allowed := [``propext, ``Classical.choice, ``Quot.sound]
  let bad := axioms.toList.filter (!allowed.contains ·)
  unless bad.isEmpty do
    throwError "'{name}' depends on axioms outside [propext, Classical.choice, Quot.sound]: {bad}"
  logInfo m!"'{name}' depends on axioms within [propext, Classical.choice, Quot.sound]"

end SolarTV.Milestone.W1
