import SolarTV.EVMIR.Ops

/-! EVM IR peephole: replace EQ followed by ISZERO with SUB before a conditional jump. -/

namespace SolarTV.EVM
open EvmRules

/-- After EQ (or SUB), the window has room for its single PUSH on any legal
entry stack. No memory, storage or return data is touched by these opcodes. -/
theorem eq_iszero_jumpi (ctx : Context) (stack : Stack) (dst fallthrough : Word)
    (legal : stack.length ≤ 1024) :
    execute ctx [.eq, .iszero] (.branch dst fallthrough) stack =
    execute ctx [.sub] (.branch dst fallthrough) stack := by
  rcases stack with _ | ⟨a, _ | ⟨b, rest⟩⟩ <;> try rfl
  have h : ¬ 1024 < rest.length + 1 + 1 := by simpa using legal
  have h' : ¬ 1024 < rest.length + 1 := by omega
  have zero (x y : Word) : Evm.sub x y = 0#256 ↔ x = y := by
    simp [Evm.sub, BitVec.sub_eq_iff_eq_add]
  by_cases same : a = b <;> simp [execute, stepChecked, step, finish, Evm.eq, Evm.iszero, zero, same, h, h']

theorem eq_iszero_zero (a b : Word) : Evm.iszero (Evm.eq a b) = 0#256 ↔ a = b := by
  by_cases same : a = b <;> simp [Evm.eq,Evm.iszero,same]

theorem sub_zero (a b : Word) : Evm.sub a b = 0#256 ↔ a = b := by
  simp [Evm.sub,BitVec.sub_eq_iff_eq_add]

end SolarTV.EVM
