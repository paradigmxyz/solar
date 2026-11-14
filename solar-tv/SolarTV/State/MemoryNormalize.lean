import SolarTV.State.MemoryLaws
import Lean

/-! Reduce closed byte-memory reads using definitional equality. Initial fixture
bytes can determine an allocation cursor even inside a symbolic execution. Keep
reads containing free variables opaque so range-proved read-over-write laws
retain their structure. Every reduction is checked by the kernel; this uses no
VM evaluation or accepted-result axiom. -/

namespace SolarTV.Execution
open Lean Meta Simp

dsimproc_decl reduceClosedRead (readWord _ _) := fun expression => do
  if expression.hasFVar || expression.hasMVar then return .continue
  return .done (← withTransparency .all (reduce expression))

end SolarTV.Execution
