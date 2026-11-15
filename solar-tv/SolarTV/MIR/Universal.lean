import SolarTV.MIR.State.Observe

/-! Function-local validation over arbitrary direct-MIR caller state.

A proof quantifies the initial journal, byte memory, return data and caller
context. The 64 KiB aligned-memory precondition is the direct interpreter's
explicit work domain, not an assumption that memory starts at zero. Results
are compared at the normal internal-return boundary, where memory, transient
storage, access warmth and return data remain live. Unsupported execution and
work exhaustion cannot establish equivalence.

This layer reuses the typed SSA interpreter rather than erasing aggregates,
assigning backend addresses or interpreting pointers as integers at interfaces. -/

namespace SolarTV.MIR.Universal
open EvmRules

/-- The supported initial byte-memory domain; contents remain arbitrary. -/
def MemoryValid (frame : State.Frame) : Prop :=
  frame.memory.length ≤ 65536 ∧ frame.memory.length % 32 = 0

/-- An accepted pair has two actual executions, not two matching model errors. -/
theorem executions_defined {before after : State.Run}
    (proof : State.equivalent before after) :
    ∃ a b, before = .ok a ∧ after = .ok b ∧ State.observe a = State.observe b := by
  cases before <;> cases after <;> simp_all [State.equivalent]

/-- Explicitly retain the unsupported-path rejection at the generic boundary. -/
theorem unsupported_is_not_equivalent (reason : String) (other : State.Run) :
    ¬ State.equivalent (.error reason) other := by
  cases other <;> simp [State.equivalent]

/-- Successful normal returns expose all result words in declaration order. -/
theorem returned_words_live (a b : List Word) (frame : State.Frame)
    (proof : State.equivalent
      (.ok (State.finish (.returned a) frame))
      (.ok (State.finish (.returned b) frame))) : a = b := by
  have equal := congrArg State.Observation.values proof
  simpa [State.observe, State.finish] using equal

/-- Expansion stays defined for a constant checked range in arbitrary valid memory. -/
theorem expand_defined (memory : List Execution.Byte) (offset count : Nat)
    (bound : memory.length ≤ 65536) (range : offset + count + 31 ≤ 65536 + 31) :
    ∃ next, Execution.expand memory offset count = .ok next := by
  unfold Execution.expand Execution.requiredWords Execution.memoryWords
  split
  · exact ⟨_, ite_eq_right (by omega)⟩
  · exact ⟨_, ite_eq_right (by omega)⟩

end SolarTV.MIR.Universal
