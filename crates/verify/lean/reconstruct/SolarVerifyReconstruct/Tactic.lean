import SolarVerify
import Smt

/-!
`vc_smt` proves an exported obligation `Obligation.Valid ob` from a cvc5 proof:

1. `vc_valid_prep` (SolarVerify.Discharge) reduces it to a shallow goal `False` over Int, Bool and
   functions;
2. `vc_smt_norm` substitutes away the definitions `x = t` of local symbols and turns Booleans,
   conditions of `if` included, into propositions, so that a quantifier the export evaluates
   classically (`qdec (∀ x, …)`) reaches cvc5 as a quantified formula rather than an opaque Bool;
3. lean-smt's `smt` asks cvc5 for a refutation of the hypotheses and reconstructs its proof
   (CPC rules) as a Lean proof term, step by step. cvc5 gets 60 seconds; `unknown` fails the proof.

The kernel checks the term; cvc5 and lean-smt's tactic code are not trusted. A step lean-smt cannot
reconstruct becomes a hole (`sorryAx`), which reconstruct.sh rejects, so the proof fails closed.
-/

namespace SolarVerify.Reconstruct

/-- An `if` at any instance is the classical one, whose condition `simp` can rewrite without a
`Decidable` instance for the result (such as a condition with a quantifier). -/
theorem ite_eq_qite {α : Type} (c : Prop) [h : Decidable c] (a b : α) :
    @ite α c h a b = SolarVerify.Discharge.qite c a b := by
  unfold SolarVerify.Discharge.qite; congr

end SolarVerify.Reconstruct

/-- Substitutes local definitions and turns Booleans into propositions: see the module comment. -/
macro "vc_smt_norm" : tactic => `(tactic| (
  try subst_vars
  try simp -failIfUnchanged only [SolarVerify.Reconstruct.ite_eq_qite] at *
  try simp -failIfUnchanged only [Bool.and_eq_true, Bool.or_eq_true, Bool.not_eq_true',
    Bool.not_eq_eq_eq_not, Bool.not_true, Bool.not_false, Bool.and_true, Bool.true_and,
    Bool.and_false, Bool.false_and, Bool.or_true, Bool.true_or, Bool.or_false, Bool.false_or,
    SolarVerify.Discharge.decide_any_eq_true, SolarVerify.Discharge.qdec_eq_true,
    decide_eq_true_eq, decide_eq_false_iff_not, Bool.decide_and, Bool.decide_or,
    decide_not, Bool.eq_false_iff, ne_eq, SolarVerify.Discharge.true_eq_iff,
    SolarVerify.Discharge.false_eq_iff] at *
  try simp -failIfUnchanged only [SolarVerify.Discharge.qite] at *))

macro "vc_smt" : tactic => `(tactic| (vc_valid_prep <;> (intros; vc_smt_norm; smt (timeout := .some 60) [*])))
