import SolarTV.EVMIR.Ops

namespace SolarTV.EVM
open EvmRules

/-- After EQ (or SUB), the window has room for its single PUSH on any legal
entry stack. No memory, storage or return data is touched by these opcodes. -/
theorem eq_iszero_jumpi (ctx : Context) (stack : Stack) (dst fallthrough : Word) :
    run ctx fallthrough [.eq, .iszero, .push dst] stack =
    run ctx fallthrough [.sub, .push dst] stack := by
  rcases stack with _ | ⟨a, _ | ⟨b, rest⟩⟩ <;> try rfl
  have zero (x y : Word) : Evm.sub x y = 0#256 ↔ x = y := by
    simp [Evm.sub, BitVec.sub_eq_iff_eq_add]
  by_cases h : a = b <;> simp [run, step, Evm.eq, Evm.iszero, zero, h]

end SolarTV.EVM
