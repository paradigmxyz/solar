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

-- Mutations of the real proof's inputs. Each flips the first literal of one clause of b8.cnf;
-- CaDiCaL finds every one of these mutated formulas satisfiable, so the proof must not check.
def flipFirst : Cnf → Nat → Cnf
  | [], _ => []
  | c :: cs, 0 => (match c with | [] => [] | l :: ls => Lit.neg l :: ls) :: cs
  | c :: cs, n + 1 => c :: flipFirst cs n

example : check (flipFirst b8.cnf 4) b8.proof = false := by decide +kernel
example : check (flipFirst b8.cnf 120) b8.proof = false := by decide +kernel
example : check (flipFirst b8.cnf 346) b8.proof = false := by decide +kernel
-- Dropping the last clause shifts no IDs but removes a clause the proof needs.
example : check b8.cnf.dropLast b8.proof = false := by decide +kernel

-- vc_ground: a closed hypothesis that the kernel evaluates to false closes the goal.
example (h : ¬ ((511#256 &&& 255#256).toNat = 255)) : False := by vc_ground
example (h : (0xff#8 - (1#8 ||| 2#8 ||| 4#8)) &&& 0x05#8 ≠ 0#8) : False := by vc_ground
-- A closed hypothesis that holds, or one with a free variable, gives no contradiction.
/-- error: vc_ground: no closed hypothesis is false -/
#guard_msgs in
example (h : (511#256 &&& 255#256).toNat = 255) : False := by vc_ground
/-- error: vc_ground: no closed hypothesis is false -/
#guard_msgs in
example (x : BitVec 8) (h : x &&& 0xf8#8 ≠ 0#8) : False := by vc_ground
