import SolarVerify.Sat
/-!
Checks of the SAT certificate library on a small proof, run with
`lake env lean test/sat/Sat.lean` from `crates/verify/lean`.

`b8.cnf` is Lean's bit-blasting of `(x &&& y) + (x ||| y) = x + y` over 8 bits, negated, and
`b8.lrat` CaDiCaL's LRAT proof that it is unsatisfiable.
-/
open SolarVerify.Sat

-- The whole proof in one kernel evaluation.
sat_load b8 "b8.cnf" "b8.lrat"
theorem b8_check : Cnf.Unsat b8.cnf := check_sound b8.cnf b8.proof (by decide +kernel)

/-- info: 'b8_check' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms b8_check

-- One kernel-checked lemma per step.
namespace stepwise
sat_prove b8 "b8.cnf" "b8.lrat"
end stepwise

/-- info: 'stepwise.b8.unsat' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms stepwise.b8.unsat

-- x1 and not x1: the empty clause follows from clauses 1 and 2.
example : check [[2], [3]] (.step ⟨3, [], [1, 2]⟩) = true := by decide +kernel
-- x1 and x2 are satisfiable: the same step does not check.
example : check [[2], [4]] (.step ⟨3, [], [1, 2]⟩) = false := by decide +kernel
-- A hint naming no clause fails.
example : check [[2], [3]] (.step ⟨3, [], [1, 7]⟩) = false := by decide +kernel
-- A proof that never adds the empty clause fails.
example : check [[2, 4], [3]] (.step ⟨3, [4], [1, 2]⟩) = false := by decide +kernel
