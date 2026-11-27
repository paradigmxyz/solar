import SolarTV.MIR.State.Interpreter
import SolarTV.MIR.Transforms.LoopSimulation

/-! The primitive step used by handwritten typed-MIR loop certificates.
The message work bound is an explicit parameter of the interpreter transition.
A model error is retained as an error exit in the proof. An executable
certificate must discharge absence of unsupported exits on its admitted paths. -/

namespace SolarTV.MIR.State
open LoopSimulation

/-- Observe effects only after the corresponding typed MIR instruction
completes. Logical equivalence does not compare interpreter work counters. -/
def loopStep (cfg : Config) (functions : Functions) (messageWork : Nat)
    (record : Machine → Option Event) : Loop Machine Event Run :=
  fun current =>
    match transition messageWork cfg functions current with
    | .ok (.next _ next) => .again (record current).toList next
    | .ok (.finished result) => .done [] (.ok result)
    | .error reason => .done [] (.error reason)

end SolarTV.MIR.State
