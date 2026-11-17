import SolarTV.MIR.State.Ops
import SolarTV.MIR.Effects

/-! Primitive correspondence for free-memory-pointer lowering.

Reading the FMP is precisely a load from byte address 64, including expansion
and model-limit failures. The pointer carrier cast is the identity at width
256. No initial cursor, valid allocation, heap floor or fresh memory is assumed.
Constructor base/end queries remain distinct requests to the unchanged context,
including when that context is unavailable. Their values are never synthesized.

The effect-tree importer uses these request constructors. These primitive
lemmas do not certify its whole SSA projection or price physical MIR gas.
-/

namespace SolarTV.MIR.LowerAlloc
open EvmRules

def freePointerRequest : Effects.Request := ⟨.opcode 0x51, [64], true⟩

def constructorRequest (isEnd : Bool) : Effects.Request :=
  ⟨.constructorRange isEnd, [], true⟩

/-- Concrete interpretation of the two normalized primitive requests. -/
def requestStep (cfg : State.Config) (frame : State.Frame) : Effects.Request → State.Step
  | ⟨.opcode 0x51, [address], true⟩ => State.operation cfg frame (.memory .load) [address]
  | ⟨.constructorRange isEnd, [], true⟩ =>
    State.operation cfg frame (.context (.constructorRange isEnd)) []
  | _ => .error "not an FMP-lowering primitive"

theorem free_pointer_request (cfg : State.Config) (frame : State.Frame) :
    requestStep cfg frame freePointerRequest = State.operation cfg frame .freePointer [] := by
  rfl

theorem pointer_carrier (word : Word) : word.setWidth 256 = word := by simp

theorem constructor_request (cfg : State.Config) (frame : State.Frame) (isEnd : Bool) :
    requestStep cfg frame (constructorRequest isEnd) =
      State.operation cfg frame (.context (.constructorRange isEnd)) [] := by
  rfl

theorem constructor_missing (cfg : State.Config) (frame : State.Frame) (isEnd : Bool)
    (missing : cfg.layout.constructor = none) :
    requestStep cfg frame (constructorRequest isEnd) = .error "missing constructor range" := by
  simp [requestStep, constructorRequest, State.operation, State.layoutOperation,
    State.required, missing]
  rfl

theorem constructor_ranges_distinct : constructorRequest false ≠ constructorRequest true := by
  decide

end SolarTV.MIR.LowerAlloc
