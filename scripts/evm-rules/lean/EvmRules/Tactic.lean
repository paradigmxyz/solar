import EvmRules.Bitblast

/-!
# Automatic rule proofs

`evm_decide` unfolds every EVM operation into `BitVec` primitives and bit-blasts the goal
with `bv_decide`, which checks the SAT solver's LRAT certificate in Lean. An optional
argument sets the SAT solver's limit in seconds.
-/

namespace EvmRules

/-- Unfold the EVM operations in the goal and every hypothesis into `BitVec` primitives. -/
macro "evm_unfold" : tactic =>
  `(tactic| (
    try simp only [Evm.add, Evm.sub, Evm.mul, Evm.div, Evm.mod, Evm.sdiv, Evm.smod, Evm.addmod,
      Evm.mulmod, Evm.exp, Evm.and, Evm.or, Evm.xor, Evm.not, Evm.shl_eq, Evm.shr_eq, Evm.sar_eq,
      Evm.lt, Evm.gt, Evm.slt, Evm.sgt, Evm.eq, Evm.ne, Evm.iszero, Evm.select, Evm.byte,
      Evm.signextend, Evm.clz, Evm.address, Evm.balance, Evm.selfbalance] at *))

/-- Unfold the EVM operations, then bit-blast whatever goal remains, with an optional SAT
limit in seconds. -/
syntax "evm_decide" (ppSpace num)? : tactic

macro_rules
  | `(tactic| evm_decide) => `(tactic| evm_decide 10)
  | `(tactic| evm_decide $timeout:num) =>
    `(tactic| (evm_unfold <;> bv_decide (config := { timeout := $timeout })))

end EvmRules
