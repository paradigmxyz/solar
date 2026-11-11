import SolarTV.EVMIR.Gas

/-! Resource refinement for the real commutative-swap peephole.
Dropping SWAP1 before ADD preserves the outgoing stack and memory while saving
three gas. For every budget at which the original completes, the candidate
completes with the same observation and at least as much remaining gas. Original
out-of-gas is unconstrained by this policy; exact gas equality would reject it. -/

namespace SolarTV.EVM.Gas
open EvmRules

theorem swap_add_refines (ctx : Context) (a b destination : Word)
    (memory : SolarTV.Memory.State) (gas : Nat) :
    refines
      (execute ctx [.instruction (.stack (.swap 0)), .instruction (.stack .add)]
        (.control (.jump destination)) [a, b] memory gas)
      (execute ctx [.instruction (.stack .add)]
        (.control (.jump destination)) [a, b] memory gas) := by
  simp [refines, execute, finish, cost, baseCost, pureCost, touched, expansionCost,
    halted, exhausted, Memory.step, Memory.finish, EVM.stepChecked, EVM.step,
    EVM.finish, Except.map, Evm.add, BitVec.add_comm]
  repeat' (first | omega | (split <;> simp_all))
  all_goals omega

end SolarTV.EVM.Gas
