import SolarTV.EVMIR.Region

/-! Extracting a terminal memory-store tail, including concrete transfer growth.
The incoming stack and memory remain arbitrary. In particular, the added label
push is checked at the 1024-word boundary; it is not erased from the semantics.
The example is the custom-error/panic tail shape in the pinned ERC20 corpus. -/

namespace SolarTV.EVM.Transforms.TailMerge
open EvmRules

def tail : Region.Code :=
  .instructions [.stack (.push 32), .store, .stack (.push 0x4e487b71),
    .stack (.push 0), .store, .stack (.push 36), .stack (.push 28)]
    (.terminal (.output true))

/-- The extracted tail already needs the temporary word used by the transfer. -/
theorem panic_tail (ctx : Context) (stack : Stack) (memory : SolarTV.Memory.State) :
    Region.execute ctx (.instructions [.stack (.push 17)] tail) stack memory =
      Region.execute ctx (.instructions [.stack (.push 17)] (.jump 1 tail)) stack memory := by
  by_cases full : 1024 < stack.length + 1
  · simp [Region.execute, Region.instructions, Memory.step, stepChecked, step,
      Except.bind, Except.map, Bind.bind, full]
  · by_cases edge : 1024 < stack.length + 2
    · have h : stack.length = 1023 := by omega
      simp [tail, Region.execute, Region.instructions, Memory.step, stepChecked, step,
        Except.bind, Except.map, Bind.bind, h]
    · have bound : ¬1024 < stack.length := by omega
      simp [tail, Region.execute, Region.instructions, Memory.step, Memory.finish,
        stepChecked, step, Except.bind, Except.map, Bind.bind, full, edge, bound, Nat.add_assoc]

end SolarTV.EVM.Transforms.TailMerge
