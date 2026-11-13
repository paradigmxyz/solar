import SolarTV.EVMIR.Gas

/-! Kernel checks for opcode prices, debit order, expansion and exhaustion.
The quadratic boundary at 23 words pins the complete formula even though the
current importer admits accesses only within its 256-byte memory projection. -/

namespace SolarTV.EVM.Gas
open EvmRules
set_option maxRecDepth 4096

example : memoryCost 22 = 66 := by decide
example : memoryCost 23 = 70 := by decide
example : pureCost (.push 0) = 2 ∧ pureCost (.push 1) = 3 := by decide

example : execute ⟨0, 0⟩ [.gas] (.control (.jump 1)) [] (SolarTV.Memory.initial [] 0) 13 =
    ⟨.execution (.edge 1 [11] (SolarTV.Memory.initial [] 0)), 0⟩ := by decide

example : execute ⟨0, 0⟩ [.gas] (.control .stop) [] (SolarTV.Memory.initial [] 0) 1 =
    exhausted := by decide

example : execute ⟨0, 0⟩ [.gas] (.control .stop) [] (SolarTV.Memory.initial [] 0) 2 =
    ⟨.execution (.success []), 0⟩ := by decide

example : finish (.control .invalid) [] (SolarTV.Memory.initial [] 0) 100 =
    ⟨.execution (.failed .invalid), 0⟩ := by decide

example : finish (.output true) [0, 32] (SolarTV.Memory.initial [] 0) 2 = exhausted := by decide
example : (finish (.output true) [0, 32] (SolarTV.Memory.initial [] 0) 10).remaining = 7 := by decide

example : expansionCost (SolarTV.Memory.initial [] 22)
    (SolarTV.Memory.expand (SolarTV.Memory.initial [] 22) 704 32) = 4 := by decide

example : refines exhausted ⟨.execution (.success []), 10⟩ := by decide
example : ¬ refines ⟨.execution (.reverted []), 0⟩ ⟨.execution (.success []), 10⟩ := by decide

example : execute ⟨0, 0⟩
    [.instruction (.stack (.push 0)), .instruction .load, .instruction (.stack .pop),
     .instruction (.stack (.push 0)), .instruction .load, .instruction (.stack .pop)]
    (.control .stop) [] (SolarTV.Memory.initial [] 0) 17 =
    ⟨.execution (.success []), 0⟩ := by decide

end SolarTV.EVM.Gas
