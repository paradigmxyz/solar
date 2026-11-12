import SolarTV.MIR.State.Observe

/-! Executable semantic invariants for the direct MIR interpreter. Phi operands
are simultaneous even on cyclic edges; work limits cannot establish equivalence.
Normal returns expose memory and transient cells, unlike transaction termination.
These checks do not invoke the backend or its physical frame conventions. -/

namespace SolarTV.MIR.State
open EvmRules SolarTV.Execution

theorem work_limit_is_not_equivalence (reason : String) (other : Run) :
    ¬ equivalent (.error reason) other := by
  simp [equivalent]

theorem simultaneous_phis :
    ((enter (some 1)
      [⟨0,256,[(1,⟨.local 1,256⟩)]⟩,⟨1,256,[(1,⟨.local 0,256⟩)]⟩]
      ⟨⟨[],[],[],[],[],0⟩,[],[],[(0,7),(1,9)]⟩).map (·.locals)) =
      .ok [(1,7),(0,9)] := by rfl

theorem live_memory_is_observable :
    ¬ equivalent
      (.ok ⟨.returned [],⟨[],[],[],[],[],0⟩,[]⟩)
      (.ok ⟨.returned [],⟨[],[],[],[],[],0⟩,[1]⟩) := by decide +kernel

theorem live_transient_is_observable :
    ¬ equivalent
      (.ok ⟨.returned [],⟨[{address:=1}],[],[],[],[],0⟩,[]⟩)
      (.ok ⟨.returned [],⟨[{address:=1,transient:=[(0,7)]}],[],[],[],[],0⟩,[]⟩) := by decide +kernel

end SolarTV.MIR.State
