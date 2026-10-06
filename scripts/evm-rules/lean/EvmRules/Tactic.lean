import EvmRules.Simp

/-!
# Automatic rule proofs

`evm_decide` rewrites with the structural identities of `Casts.lean`, unfolds every EVM
operation into `BitVec` primitives and bit-blasts the goal with `bv_decide`, which checks the
SAT solver's LRAT certificate in Lean. An optional argument sets the SAT solver's limit in
seconds.

Bit-blasting struggles with two shapes, which the other tactics prove without a SAT solver:

* `evm_bits` proves an equation bit by bit with the lemmas of `Bits.lean`, turning variable
  shift counts and extension widths into linear arithmetic over bit indices for `grind`.
* `evm_ring` lets `grind`'s commutative-ring normalization prove identities over products of
  variables, such as distributivity, after splitting conditions that are known to be
  booleans.

`evm_simp` is the pipeline of paradigmxyz/solar#1648: it simplifies the goal and every
hypothesis with the lemmas of `Simp.lean` and `Casts.lean`, then decides what remains with
`bv_omega`, or bit-blasts it after normalizing associative and commutative operations.
-/

namespace EvmRules

/-- Unfold the EVM operations in the goal and every hypothesis into `BitVec` primitives. -/
macro "evm_unfold" : tactic =>
  `(tactic| (
    try simp only [Evm.add, Evm.sub, Evm.mul, Evm.div, Evm.mod, Evm.sdiv, Evm.smod, Evm.addmod,
      Evm.mulmod, Evm.exp, Evm.and, Evm.or, Evm.xor, Evm.not, Evm.shl_eq, Evm.shr_eq, Evm.sar_eq,
      Evm.lt, Evm.gt, Evm.slt, Evm.sgt, Evm.eq, Evm.ne, Evm.iszero, Evm.select, Evm.byte,
      Evm.signextend, Evm.clz, Evm.address, Evm.balance, Evm.selfbalance] at *))

/-- Rewrite nested sign extensions and casts and divisions by powers of two with the
identities of `Casts.lean`, discharging their preconditions from the hypotheses. Only the side
goals substitute equal variables: the goal keeps every variable, so that a counterexample
`bv_decide` reports afterwards still assigns each of them. -/
macro "evm_struct" : tactic =>
  `(tactic| (
    try simp (disch := (try subst_vars) <;> assumption) only [signextend_signextend,
      select_same, sext_sext, eq_sext_sext, ne_sext_sext, div_shl_one] at *))

/-- Rewrite structurally, unfold the EVM operations, then bit-blast whatever goal remains,
with an optional SAT limit in seconds. -/
syntax "evm_decide" (ppSpace num)? : tactic

macro_rules
  | `(tactic| evm_decide) => `(tactic| evm_decide 10)
  | `(tactic| evm_decide $timeout:num) =>
    `(tactic| (
      evm_struct
      all_goals (evm_unfold <;> bv_decide (config := { timeout := $timeout }))))

/-- Prove a word equation bit by bit: state the goal and every word equation among the
hypotheses as equalities of all 256 bits, write each bit of a shift, extension or mask as a bit
of its operand, and let `grind` relate the bit indices. -/
macro "evm_bits" : tactic =>
  `(tactic| (
    subst_vars
    try simp only [and_seven_eq_zero] at *
    simp only [BitVec.eq_of_getLsbD_eq_iff, Evm.and, Evm.or, Evm.xor, Evm.not,
      BitVec.getLsbD_and, BitVec.getLsbD_or, BitVec.getLsbD_xor, BitVec.getLsbD_not, bits_shl,
      bits_shr, bits_sar, bits_mask, bits_signextend, bits_max, bits_address_mask, bits_one,
      bits_zero, toNat_select_lt, toNat_select_gt, toNat_shr, toNat_add_wrap, toNat_sub_wrap,
      toNat_mul, BitVec.lt_def, BitVec.le_def, Nat.add_sub_cancel, Nat.add_sub_cancel_left] at *
    try simp only [BitVec.ofNat_eq_ofNat, BitVec.toNat_ofNat, Nat.reducePow, Nat.reduceMod] at *
    grind))

/-- Prove a ring identity with `grind`: split boolean conditions, write left shifts as products
with powers of two and normalize both sides as commutative-ring polynomials. -/
macro "evm_ring" : tactic =>
  `(tactic| (
    subst_vars
    try simp only [le_one_iff] at *
    try simp only [Evm.add, Evm.sub, Evm.mul, Evm.and, Evm.or, Evm.xor, Evm.not, Evm.shl_eq,
      BitVec.shiftLeft_eq', BitVec.shiftLeft_eq_mul_twoPow, Evm.shr, Evm.sar, Evm.select, Evm.lt,
      Evm.gt, Evm.eq, Evm.ne, Evm.iszero] at *
    grind))

/-- Simplify with the lemmas of `Simp.lean` and `Casts.lean` everywhere, unfold the EVM
operations, then decide the rest over natural numbers with `bv_omega` or bit-blast it with
associative and commutative operations normalized, with an optional SAT limit in seconds. -/
syntax "evm_simp" (ppSpace num)? : tactic

macro_rules
  | `(tactic| evm_simp) => `(tactic| evm_simp 10)
  | `(tactic| evm_simp $timeout:num) =>
    `(tactic| (
      try subst_vars
      try simp_all only [mul_shl_one, shl_one_mul, div_lt, mod_lt, mod_self, smod_self,
        mod_shl_one, div_clz, mod_clz, exp_two, exp_square, exp_zero, exp_one, one_exp,
        shift_sum_eq, shl_shl, shr_shr, sar_sar, and_shl, or_shl, xor_shl, add_shl, and_shr,
        or_shr, xor_shr, and_sar, or_sar, xor_sar, sext_recover, signextend_signextend,
        select_same, sext_sext, eq_sext_sext, ne_sext_sext, div_shl_one]
      all_goals (evm_unfold; try simp only [shiftSum] at *)
      all_goals first
        | bv_omega
        | bv_decide (config := { timeout := $timeout, acNf := true })))

end EvmRules
