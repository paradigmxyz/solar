/-!
Propositional formulas in conjunctive normal form, with literals coded as natural numbers so
that the kernel evaluates them with its built-in arithmetic.

This is the meaning of what `SolarVerify.Sat.Lrat` checks: a formula is unsatisfiable when every
assignment of its variables falsifies one of its clauses.
-/
namespace SolarVerify.Sat

/-- A literal: `2 * v` is variable `v`, and `2 * v + 1` its negation. -/
abbrev Lit := Nat

/-- A clause: the disjunction of its literals. -/
abbrev Clause := List Lit

/-- A formula: the conjunction of its clauses. -/
abbrev Cnf := List Clause

/-- A value for every variable. -/
abbrev Assignment := Nat → Bool

/-- The literal of variable `v` with polarity `pos`. -/
def Lit.mk (v : Nat) (pos : Bool) : Lit := if pos then 2 * v else 2 * v + 1

/-- The negation of a literal. -/
def Lit.neg (l : Lit) : Lit := bif Nat.beq (l % 2) 0 then l + 1 else l - 1

/-- The value of a literal. -/
def Lit.eval (a : Assignment) (l : Lit) : Bool := if l % 2 = 0 then a (l / 2) else !a (l / 2)

/-- The value of a clause: some literal is true. -/
def Clause.eval (a : Assignment) (c : Clause) : Bool := c.any (Lit.eval a)

/-- The value of a formula: every clause is true. -/
def Cnf.eval (a : Assignment) (f : Cnf) : Bool := f.all (Clause.eval a)

/-- No assignment satisfies the formula. -/
def Cnf.Unsat (f : Cnf) : Prop := ∀ a, Cnf.eval a f = false

theorem Lit.eval_neg (a : Assignment) (l : Nat) : Lit.eval a (Lit.neg l) = !Lit.eval a l := by
  unfold Lit.eval Lit.neg
  by_cases h : l % 2 = 0
  · have h1 : ¬ (l + 1) % 2 = 0 := by omega
    have h2 : (l + 1) / 2 = l / 2 := by omega
    simp [h, h1, h2]
  · have h1 : (l - 1) % 2 = 0 := by omega
    have h2 : (l - 1) / 2 = l / 2 := by omega
    have hb : Nat.beq (l % 2) 0 = false := by
      cases e : Nat.beq (l % 2) 0
      · rfl
      · exact absurd (Nat.eq_of_beq_eq_true e) h
    simp [hb, h, h1, h2]

theorem Lit.eval_mk (a : Assignment) (v : Nat) (pos : Bool) :
    Lit.eval a (Lit.mk v pos) = (a v == pos) := by
  have h1 : 2 * v % 2 = 0 := by omega
  have h2 : 2 * v / 2 = v := by omega
  have h3 : ¬ (2 * v + 1) % 2 = 0 := by omega
  have h4 : (2 * v + 1) / 2 = v := by omega
  cases pos <;> cases h : a v <;> simp [Lit.mk, Lit.eval, h1, h2, h4, h]

theorem Clause.eval_cons (a : Assignment) (l : Lit) (c : Clause) :
    Clause.eval a (l :: c) = (Lit.eval a l || Clause.eval a c) := by
  simp [Clause.eval]

theorem Clause.eval_eq_true (a : Assignment) (c : Clause) :
    Clause.eval a c = true ↔ ∃ l ∈ c, Lit.eval a l = true := by
  simp [Clause.eval]

theorem Cnf.eval_eq_true (a : Assignment) (f : Cnf) :
    Cnf.eval a f = true ↔ ∀ c ∈ f, Clause.eval a c = true := by
  simp [Cnf.eval]

end SolarVerify.Sat
