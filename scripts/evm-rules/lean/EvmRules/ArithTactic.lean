import Lean.Elab.Tactic.Basic
import Lean.Meta.Tactic.Assert
import EvmRules.Arith
import EvmRules.Tactic

/-!
# `evm_arith`

`evm_arith` proves rules over products, quotients and remainders with symbolic operands,
which bit-blasting cannot. It works over natural numbers:

1. It substitutes equal operands, states comparison words as decided propositions and
   the readers' overflow preconditions as bounds on exact products and sums.
2. It states every word as its number, writing each operation with its wrapping
   (`(a + b) % 2 ^ 256`) and EVM's zero divisor.
3. `evm_div_facts` adds the bounds of every quotient and remainder by a non-literal:
   `a / b ≤ a`, `a / b * b ≤ a < a / b * b + b`, `a / b * b + a % b = a` and more.
4. Rewrites whose side conditions `omega` proves from those bounds remove the wrapping
   that cannot happen and the quotients in comparisons.
5. After ordering every product the same way, `omega` closes the goal, treating the
   remaining products, quotients and remainders as opaque terms.

Each step is a proof Lean's kernel checks; nothing is trusted beyond the definitions.
-/

namespace EvmRules

open Lean Meta Elab Tactic

/-- Collects the natural-number quotients (`true`) and remainders (`false`) by a
non-literal in `e` that mention no bound variables; `omega` handles literal divisors. -/
partial def natDivMods (e : Expr) (acc : Array (Bool × Expr × Expr)) :
    Array (Bool × Expr × Expr) :=
  match e with
  | .app .. =>
    let args := e.getAppArgs
    let acc := args.foldl (fun acc a => natDivMods a acc) (natDivMods e.getAppFn acc)
    let isDiv := e.isAppOfArity ``HDiv.hDiv 6
    if (isDiv || e.isAppOfArity ``HMod.hMod 6) && args[0]! == mkConst ``Nat
        && !e.hasLooseBVars && args[5]!.nat?.isNone then
      let entry := (isDiv, args[4]!, args[5]!)
      if acc.contains entry then acc else acc.push entry
    else acc
  | .forallE _ d b _ | .lam _ d b _ => natDivMods b (natDivMods d acc)
  | .mdata _ b | .proj _ _ b => natDivMods b acc
  | .letE _ t v b _ => natDivMods b (natDivMods v (natDivMods t acc))
  | _ => acc

/-- Adds the bounds of every natural-number quotient and remainder by a non-literal in
the goal and its hypotheses, so that `omega` can treat them as opaque terms. -/
elab "evm_div_facts" : tactic => withMainContext do
  let mut found := natDivMods (← instantiateMVars (← getMainTarget)) #[]
  for decl in ← getLCtx do
    unless decl.isImplementationDetail do
      found := natDivMods (← instantiateMVars decl.type) found
  for (isDiv, a, b) in found do
    let lemmas := if isDiv then
        #[``Nat.div_le_self, ``Nat.div_mul_le_self, ``div_le_half, ``lt_div_mul_add_of_pos]
      else #[``Nat.mod_le, ``mod_lt_of_pos, ``Nat.div_add_mod']
    for lemma in lemmas do
      let proof ← mkAppM lemma #[a, b]
      let type ← inferType proof
      liftMetaTactic fun goal => do
        let goal ← goal.assert `bound type proof
        let (_, goal) ← goal.intro1P
        return [goal]

end EvmRules

/-- Proves a rule over products, quotients and remainders; see `EvmRules/ArithTactic.lean`. -/
macro "evm_arith" : tactic => `(tactic| (
  subst_vars
  try simp only [↓ EvmRules.mul_fits_iff, ↓ EvmRules.add_fits_iff, EvmRules.lt_bits,
    EvmRules.gt_bits, EvmRules.eq_bits, EvmRules.ne_bits, EvmRules.iszero_bits,
    EvmRules.or_bits, ← Bool.decide_or, ite_true, ite_false, EvmRules.bits_eq_bits,
    EvmRules.bits_eq_one, EvmRules.bits_eq_zero] at *
  try simp only [BitVec.toNat_eq, BitVec.le_def, BitVec.lt_def, ne_eq, EvmRules.toNat_div,
    EvmRules.toNat_mul, EvmRules.toNat_add_wrap, EvmRules.toNat_sub_wrap,
    EvmRules.toNat_mod_ite, BitVec.ofNat_eq_ofNat, BitVec.toNat_ofNat, Nat.reducePow,
    Nat.reduceMod] at *
  evm_div_facts
  try simp (disch := omega) only [Nat.mod_eq_of_lt, EvmRules.sub_wrap_of_le, ite_eq_left,
    ite_eq_right, Nat.div_div_eq_div_mul, EvmRules.div_mul_div_cancel, Nat.div_lt_iff_lt_mul,
    Nat.le_div_iff_mul_le, Nat.div_eq_zero_iff_lt, Nat.lt_div_iff_mul_lt,
    Nat.div_le_iff_le_mul_add_pred, EvmRules.lt_div_succ_iff, EvmRules.wrapped_div_eq_iff,
    EvmRules.eq_wrapped_div_iff, EvmRules.checked_product_left,
    EvmRules.checked_product_left', EvmRules.not_lt_share, Nat.div_eq_of_lt, not_false_eq_true,
    not_true_eq_false, iff_self, false_iff, iff_false, true_iff, iff_true] at *
  try simp only [Nat.mul_comm, Nat.mul_left_comm] at *
  all_goals omega))

open Lean in
/-- Records the tactic that proved a theorem, for the verification report. -/
elab "evm_proved_by " name:str : tactic => logInfo m!"proved by {name.getString}"

/-- Tries `evm_arith`, then `evm_decide` with a SAT limit of `n` seconds, and records which
one proved the goal. -/
macro "evm_auto " n:num : tactic => `(tactic| first
  | (evm_arith; evm_proved_by "evm_arith")
  | (evm_decide $n; evm_proved_by "evm_decide"))
