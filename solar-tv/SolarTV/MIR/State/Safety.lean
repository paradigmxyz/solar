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
      [⟨0,.word 256,[(1,⟨.local 1,.word 256⟩)]⟩,⟨1,.word 256,[(1,⟨.local 0,.word 256⟩)]⟩]
      ⟨⟨[],[],[],[],[],0⟩,[],[],[(0,.scalar ⟨256,7⟩),(1,.scalar ⟨256,9⟩)],[],0,0⟩).map (·.locals)) =
      .ok [(1,.scalar ⟨256,7⟩),(0,.scalar ⟨256,9⟩)] := by with_unfolding_all rfl

theorem live_memory_is_observable :
    ¬ equivalent
      (.ok ⟨.returned [],⟨[],[],[],[],[],0⟩,[],[],0,0⟩)
      (.ok ⟨.returned [],⟨[],[],[],[],[],0⟩,[1],[],0,0⟩) := by decide +kernel

theorem live_transient_is_observable :
    ¬ equivalent
      (.ok ⟨.returned [],⟨[{address:=1}],[],[],[],[],0⟩,[],[],0,0⟩)
      (.ok ⟨.returned [],⟨[{address:=1,transient:=[(0,7)]}],[],[],[],[],0⟩,[],[],0,0⟩) := by decide +kernel

theorem live_return_data_is_observable :
    ¬ equivalent
      (.ok ⟨.returned [],⟨[],[],[],[],[],0⟩,[],[1],0,0⟩)
      (.ok ⟨.returned [],⟨[],[],[],[],[],0⟩,[],[2],0,0⟩) := by decide +kernel

theorem live_boundary_budgets_are_observable :
    ¬ equivalent
      (.ok ⟨.returned [],⟨[],[],[],[],[],0⟩,[],[],0,0⟩)
      (.ok ⟨.returned [],⟨[],[],[],[],[],0⟩,[],[],0,1⟩) := by decide +kernel

theorem terminal_boundary_budgets_are_dead :
    equivalent
      (.ok ⟨.success [],⟨[],[],[],[],[],0⟩,[],[],0,0⟩)
      (.ok ⟨.success [],⟨[],[],[],[],[],0⟩,[],[],0,1⟩) := by decide +kernel

end SolarTV.MIR.State
