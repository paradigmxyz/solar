import SolarTV.Memory

/-! MIR memory-dse removes a word store overwritten before any read.
The final store has the same range, so it also subsumes the first expansion.
Both the live return and all projected memory bytes remain observable. The
theorem is independent of the incoming bytes, active size and stored values. -/

namespace SolarTV.MIR
open EvmRules

theorem dead_store (memory : Memory.State) (offset : Nat) (a b : Word)
    (result : BitVec width) :
    refines (Outcome.returned (⟨result, Memory.store (Memory.store memory offset a) offset b⟩ :
      Memory.Returned width))
      (Outcome.returned ⟨result, Memory.store memory offset b⟩) := by
  simp [refines_iff_eq, Memory.store, Memory.expand]

end SolarTV.MIR
