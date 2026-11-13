import SolarTV.MIR.State.Observe

/-! Executable semantic invariants for the direct MIR interpreter. Phi operands
are simultaneous even on cyclic edges; work limits cannot establish equivalence.
Normal returns expose memory and transient cells, unlike transaction termination.
A failed `assume` is undefined: it admits any completed target but is never one.
These checks do not invoke the backend or its physical frame conventions. -/

namespace SolarTV.MIR.State
open EvmRules SolarTV.Execution

theorem work_limit_is_not_equivalence (reason : String) (other : Run) :
    ¬ equivalent (.error reason) other := by
  simp [equivalent]

theorem simultaneous_phis :
    ((enter (some 1)
      [⟨0,.word 256,[(1,⟨.local 1,.word 256⟩)]⟩,⟨1,.word 256,[(1,⟨.local 0,.word 256⟩)]⟩]
      ⟨⟨[],[],[],[],[],0⟩,[],[],[(0,.scalar ⟨256,7⟩),(1,.scalar ⟨256,9⟩)],[],0⟩).map (·.locals)) =
      .ok [(1,.scalar ⟨256,7⟩),(0,.scalar ⟨256,9⟩)] := by with_unfolding_all rfl

theorem live_memory_is_observable :
    ¬ equivalent
      (.ok ⟨.returned [],⟨[],[],[],[],[],0⟩,[],[],0⟩)
      (.ok ⟨.returned [],⟨[],[],[],[],[],0⟩,[1],[],0⟩) := by decide +kernel

theorem live_transient_is_observable :
    ¬ equivalent
      (.ok ⟨.returned [],⟨[{address:=1}],[],[],[],[],0⟩,[],[],0⟩)
      (.ok ⟨.returned [],⟨[{address:=1,transient:=[(0,7)]}],[],[],[],[],0⟩,[],[],0⟩) := by decide +kernel

theorem live_return_data_is_observable :
    ¬ equivalent
      (.ok ⟨.returned [],⟨[],[],[],[],[],0⟩,[],[1],0⟩)
      (.ok ⟨.returned [],⟨[],[],[],[],[],0⟩,[],[2],0⟩) := by decide +kernel

theorem undefined_target_is_not_equivalence :
    ¬ equivalent
      (.ok ⟨.returned [],⟨[],[],[],[],[],0⟩,[],[],0⟩)
      (.ok ⟨.undefined,⟨[],[],[],[],[],0⟩,[],[],0⟩) := by decide +kernel

theorem undefined_source_admits_any_completion :
    equivalent
      (.ok ⟨.undefined,⟨[],[],[],[],[],0⟩,[],[],0⟩)
      (.ok ⟨.returned [7],⟨[],[],[],[],[],0⟩,[],[],0⟩) := by decide +kernel

theorem undefined_source_does_not_admit_model_errors (reason : String) :
    ¬ equivalent (.ok ⟨.undefined,⟨[],[],[],[],[],0⟩,[],[],0⟩) (.error reason) := by
  simp [equivalent]

theorem failed_assume_is_undefined (cfg : Config) :
    (transition 1 cfg
      [(0,⟨[],[],[⟨[],[⟨none,.word 256,none,.assume .eq,
        [⟨.constant (.ofWord 256 1),.word 256⟩,⟨.constant (.ofWord 256 2),.word 256⟩]⟩],.stop⟩]⟩)]
      ⟨{function := 0,entered := true},[],⟨⟨[],[],[],[],[],0⟩,[],[],[],[],0⟩⟩).map
        (fun | .finished r => some r.completion | .next .. => none) = .ok (some .undefined) := by
  with_unfolding_all rfl

end SolarTV.MIR.State
