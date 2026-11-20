import SolarTV.EVMIR.Ops

/-! The commutative-swap peephole removes SWAP1 before ADD.
Both versions use the same live-in stack. At a valid entry height the swap
preserves the height, ADD removes one word, and the outgoing label push fits.
The proof observes the entire resulting stack at the outgoing edge. -/

namespace SolarTV.EVM
open EvmRules

theorem swap_add (ctx : Context) (a b destination : Word) (rest : Stack)
    (legal : rest.length ≤ 1022) :
    execute ctx [.swap 0, .add] (.jump destination) (a :: b :: rest) =
    execute ctx [.add] (.jump destination) (a :: b :: rest) := by
  have h : ¬ 1024 < rest.length + 1 + 1 := by omega
  have h' : ¬ 1024 < rest.length + 1 := by omega
  simp [execute, stepChecked, step, finish, h, h', Evm.add, BitVec.add_comm]

end SolarTV.EVM
